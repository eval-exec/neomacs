//! E5.3 / E5.4: thread control for opt-in parallel owners over the L6 serial
//! ring and Green `Control` (`E5.3-E5.4-THREAD-CONTROL.md`, Q1–Q8 rulings).
//!
//! Keeps `serial_turn.rs` minimal and self-contained for `e51_turn_contracts.rs`
//! (Q5a) while providing:
//! - [`ParkWait`] (Q5b): the typed wait reasons a parallel serial turn parks on;
//! - [`TurnEntryKind`]: distinguishes normal turn entry from V8 `SerialUnwind`
//!   entry (which must not deliver a pending signal at entry, `E5.1-TYPES.md §6`);
//! - [`ParallelOwnerControl`]: owner-thread-confined (`!Send, !Sync`) control
//!   state holding the owner's `ThreadKey`, `OwnerWake`, `SignalHint`, and
//!   `Arc<Control>`;
//! - [`with_world_parkable`]: scoped in-stack park bridge (Q6a, same pattern as
//!   L6.7c(c)'s `LiveSerial::with_world_collectible` + `SerialTurn::with_world_rebind`)
//!   so blocking Lisp builtins called inside a serial turn park the live turn
//!   cleanly instead of falling through to simulated threading.

use super::super::Context;
use super::super::green::{
    ConditionId, Control, HostRequestKey, MutexId, OwnerWake, ParallelWaitOutcome, SignalHint,
    ThreadKey, Wait,
};
use super::serial_detach::{ActiveSerialTurn, HostRefusal};
use super::serial_park::{
    KeeperPort, LendGuard, LiveSerial, PortRefusal, TurnStartRefusal, give_back, receive,
};
use super::serial_ticket::{LocalSerialOwner, ReceiveReason};
use super::serial_turn::SerialHost;
use neovm_value::error::{EvalResult, Flow, LispCondition, signal};
use neovm_value::value::Value;
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

type Confined = PhantomData<Rc<()>>;

/// What a parallel owner's parked serial turn waits on (E5.1-TYPES §1, Q5b).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParkWait {
    /// `(thread-yield)` inside a serial turn: release ticket, yield, re-request
    /// at the ring tail (GNU `yield_callback`, `src/thread.c:725-732`).
    Yield,
    /// `(mutex-lock MUTEX)` or `condition-wait` mutex relock.
    Mutex(MutexId),
    /// `(condition-wait COND)`.
    Condition(ConditionId),
    /// `(thread-join THREAD)`.
    Join(ThreadKey),
    /// Placeholder until E5.5 wires `NativePlan`: not interrupted by
    /// `thread-signal` (GNU `thread_select`, `src/thread.c:612-640`; Row 3).
    Native,
    /// Routed host request (`HostRequestKey`): not interrupted by `thread-signal`
    /// directly (Row 3).
    Host(HostRequestKey),
}

/// How a parallel owner is entering a serial turn for D2 signal delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurnEntryKind {
    /// Ordinary escape or post-wait resume: delivers a pending signal when the
    /// owner's `condition_stack` is non-empty (GNU `thread.c:146-154`).
    Normal,
    /// V8 `EscapeWhy::SerialUnwind` (`E5.1-TYPES.md §6`): MUST NOT deliver a
    /// pending signal at turn entry, because delivery could pick a handler that
    /// the same unwind request is about to pop.
    SerialUnwind,
}

/// Pre-detach failures inside [`with_world_parkable`]: the owner's `Context` is
/// still attached on this OS thread and still holds its serial ticket, so the
/// wait registration is rolled back, `SERIAL_TURN_PARK_SCOPE` is restored, and
/// a Lisp error `Flow` can safely be returned.
#[derive(Debug)]
pub(crate) enum PreDetachParkError {
    MissingEpoch,
    Detach(HostRefusal),
}

/// Post-detach failures inside [`with_world_parkable`]: carrier TLS is already
/// parked and the serial ticket has already been returned. Failing closed via
/// panic (matching L6.7c(c)'s `gc_collect_serial_turn`, F6.1) guarantees no
/// Lisp `Value` is allocated with parked carrier TLS, no broken scope is
/// restored, and no Lisp handler can resume on a detached `Context`.
#[derive(Debug)]
pub(crate) enum PostDetachParkError {
    Poisoned,
    Failed,
    Receive(ReceiveReason),
    Activate,
    Reattach(HostRefusal),
}

/// Typed failure split for [`with_world_parkable`]: separates recoverable
/// pre-detach refusals from fail-closed post-detach breaches.
#[derive(Debug)]
pub(crate) enum SerialParkFailure {
    PreDetach(PreDetachParkError),
    PostDetach(PostDetachParkError),
}

impl PreDetachParkError {
    fn into_flow(self) -> Flow {
        signal(
            LispCondition::Error,
            vec![Value::string(format!(
                "neovm internal error: parallel serial park refused before detach: {self:?}"
            ))],
        )
    }
}

/// Convert a [`ParallelWaitOutcome::Deadlock`] into the exact same Lisp signal
/// as Baton's `DeadlockVerdict` (`green/switch.rs:1085-1102`, Q2b).
pub(crate) fn parallel_deadlock_flow(
    invariant: Result<(), crate::emacs_core::eval::green::SchedulerIndexInconsistency>,
) -> Flow {
    match invariant {
        Ok(()) => signal(
            LispCondition::Error,
            vec![Value::string(
                "All threads are blocked: this one would wait forever \
                 (GNU Emacs hangs here; neomacs signals)",
            )],
        ),
        Err(error) => {
            let message =
                format!("neomacs internal error: scheduler wait-index invariant violated: {error}");
            tracing::error!("{message}");
            signal(LispCondition::Error, vec![Value::string(message)])
        }
    }
}

/// Per-parallel-owner control state confined to the owner's OS thread
/// (`!Send, !Sync`, Q5a). Registered with Green `Control` on creation and
/// paired with `OwnerSerialState` on the owner's OS thread.
///
/// Implements `Drop` (Nit D8-1) so even if the owner thread unwinds before
/// calling `unregister()`, its `Participant::Parallel` record is retired from
/// `Control` and never leaves a zombie parked entry that could skew D8's
/// `enrolled_owners <= parallel_participant_count` check.
#[allow(dead_code)] // Dormant E5.3a; E5.3b/E5.4a/E6-P wire full lifecycle.
pub(crate) struct ParallelOwnerControl {
    key: ThreadKey,
    wake: OwnerWake,
    signal_hint: SignalHint,
    control: Arc<Control>,
    registered: bool,
    confined: Confined,
}

static_assertions::assert_not_impl_any!(ParallelOwnerControl: Send, Sync);

impl Drop for ParallelOwnerControl {
    fn drop(&mut self) {
        if self.registered {
            self.registered = false;
            self.control.unregister_parallel(self.key);
        }
    }
}

#[allow(dead_code)]
impl ParallelOwnerControl {
    /// Register KEY as a `Participant::Parallel` in `control` and return its
    /// thread-confined control handle.
    pub(crate) fn register(control: Arc<Control>, key: ThreadKey) -> Self {
        let (wake, signal_hint) = control.register_parallel(key);
        Self {
            key,
            wake,
            signal_hint,
            control,
            registered: true,
            confined: PhantomData,
        }
    }

    /// Unregister this parallel owner from `Control` (publishes `Wait::Exited`
    /// and removes its participant record).
    pub(crate) fn unregister(mut self) {
        if self.registered {
            self.registered = false;
            self.control.unregister_parallel(self.key);
        }
    }

    #[inline]
    pub(crate) fn key(&self) -> ThreadKey {
        self.key
    }

    #[inline]
    pub(crate) fn wake(&self) -> &OwnerWake {
        &self.wake
    }

    #[inline]
    pub(crate) fn signal_hint(&self) -> &SignalHint {
        &self.signal_hint
    }

    #[inline]
    pub(crate) fn control(&self) -> &Arc<Control> {
        &self.control
    }

    /// Owner-local fast-path poll (`Relaxed`, E5.3 D2). Returns `true` when
    /// another thread has written a pending signal and the owner should enter a
    /// serial turn to deliver it.
    #[inline]
    pub(crate) fn poll_signal_hint(&self) -> bool {
        self.signal_hint.peek_relaxed()
    }

    /// Deliver a pending signal at serial-turn entry, honoring both:
    /// - `TurnEntryKind::SerialUnwind` (never delivers at entry, `E5.1-TYPES §6`),
    /// - GNU `post_acquire_global_lock` (`src/thread.c:146-154`, Q6c): defers
    ///   while `ctx.condition_stack.is_empty()` (newborn before top-level
    ///   handler installation).
    pub(crate) fn deliver_signal_at_entry(
        &self,
        ctx: &mut Context,
        entry_kind: TurnEntryKind,
    ) -> Result<(), Flow> {
        match entry_kind {
            TurnEntryKind::SerialUnwind => Ok(()),
            TurnEntryKind::Normal => {
                if !self.signal_hint.check_acquire()
                    && !ctx.threads.has_pending_thread_signal(self.key.id())
                {
                    return Ok(());
                }
                ctx.deliver_parallel_turn_signal(self.key)
            }
        }
    }

    /// E5.3 D6: `(thread-yield)` called OUTSIDE a serial turn (owner-local
    /// parallel execution, Row 5).
    ///
    /// Never takes a serial ticket when no signal is pending: polls
    /// `SignalHint` (relaxed) and calls `std::thread::yield_now()`. If a signal
    /// IS pending, invokes `on_signal` so the caller enters a serial turn and
    /// delivers it.
    pub(crate) fn yield_outside_turn<R>(
        &self,
        on_signal: impl FnOnce() -> Result<R, Flow>,
    ) -> Result<Option<R>, Flow> {
        if self.poll_signal_hint() {
            return on_signal().map(Some);
        }
        std::thread::yield_now();
        if self.poll_signal_hint() {
            return on_signal().map(Some);
        }
        Ok(None)
    }
}

mod erased_bridge {
    use super::{Context, ParallelWaitOutcome, ParkWait, SerialParkFailure};

    /// Type-erased trampoline function pointer paired with a matching `*mut ()`
    /// closure pointer inside [`ErasedParkBridge`].
    ///
    /// # Safety
    /// Callers must pass a `data` pointer that:
    /// - is non-null, properly aligned for the exact concrete `F` used to
    ///   instantiate the trampoline, and points to a live, initialized `F`;
    /// - remains valid and uniquely accessible (no aliasing `&F` or `&mut F`
    ///   anywhere) for the entire duration of the call.
    type SerialParkBridgeCall = unsafe fn(
        data: *mut (),
        ev: &mut Context,
        wait: ParkWait,
        native_body: Option<Box<dyn FnOnce()>>,
    ) -> Result<ParallelWaitOutcome, SerialParkFailure>;

    /// Sealed `(data, call)` trampoline pair whose fields are private to this
    /// module so a mismatched `(data, call)` pair is unconstructible even
    /// inside `thread_control`.
    pub(super) struct ErasedParkBridge {
        data: *mut (),
        call: SerialParkBridgeCall,
    }

    impl ErasedParkBridge {
        /// Construct a type-paired [`ErasedParkBridge`] from a mutable
        /// reference to a concrete closure `F`.
        ///
        /// Both `data` (`std::ptr::from_mut(bridge_fn).cast::<()>()`) and
        /// `call` (`Self::call_bridge::<F>`) are produced in the same generic
        /// instantiation, so `call`'s `F` is `data`'s pointee type; the scope
        /// cannot be installed or reused with a mismatched pair.
        pub(super) fn new<F>(bridge_fn: &mut F) -> Self
        where
            F: FnMut(
                &mut Context,
                ParkWait,
                Option<Box<dyn FnOnce()>>,
            ) -> Result<ParallelWaitOutcome, SerialParkFailure>,
        {
            Self {
                data: std::ptr::from_mut(bridge_fn).cast::<()>(),
                call: Self::call_bridge::<F>,
            }
        }

        /// Invoke the erased bridge closure.
        ///
        /// # Safety
        /// [`Self::new`] guarantees `self.call` and `self.data` share the exact
        /// same concrete type `F` (proper type and alignment). The caller must
        /// guarantee that:
        /// - the stack-allocated `F` originally passed to [`Self::new`] is
        ///   still live and initialized for the entire duration of this call;
        /// - no other reference (`&F` or `&mut F`) aliases that `F` for the
        ///   entire duration of this call (including across re-entrant calls on
        ///   the same OS thread).
        pub(super) unsafe fn call(
            &self,
            ev: &mut Context,
            wait: ParkWait,
            native_body: Option<Box<dyn FnOnce()>>,
        ) -> Result<ParallelWaitOutcome, SerialParkFailure> {
            // SAFETY: By construction in `Self::new::<F>`, `self.data` is
            // non-null, properly aligned for `self.call`'s concrete `F`, and
            // has `F` as its pointee type. Liveness, initialization, and
            // exclusive `&mut F` access for the duration of this call are
            // discharged by this method's `# Safety` preconditions.
            unsafe { (self.call)(self.data, ev, wait, native_body) }
        }

        /// Monomorphized trampoline for [`Self::new::<F>`].
        ///
        /// # Safety
        /// Callers must pass a `data` pointer that is non-null, properly
        /// aligned for `F`, points to a live and initialized `F`, and has
        /// exclusive access (no aliasing `&F` or `&mut F`) for the entire
        /// duration of the call.
        unsafe fn call_bridge<F>(
            data: *mut (),
            ev: &mut Context,
            wait: ParkWait,
            native_body: Option<Box<dyn FnOnce()>>,
        ) -> Result<ParallelWaitOutcome, SerialParkFailure>
        where
            F: FnMut(
                &mut Context,
                ParkWait,
                Option<Box<dyn FnOnce()>>,
            ) -> Result<ParallelWaitOutcome, SerialParkFailure>,
        {
            // SAFETY: Discharged by `call_bridge<F>`'s `# Safety`
            // preconditions: `data` is non-null, properly aligned for `F`,
            // points to a live and initialized `F`, and is uniquely accessible
            // (no aliasing `&F` or `&mut F`) for the duration of this call.
            let bridge = unsafe { &mut *data.cast::<F>() };
            bridge(ev, wait, native_body)
        }
    }
}

use erased_bridge::ErasedParkBridge;

/// Thread-local RAII hook for in-stack parking inside [`with_world_parkable`].
/// Reuses [`ActiveSerialTurn`] (`{ ctx_addr, heap, context_id }`, NIT-1) as the
/// closed, deny-by-default exact identity key, storing zero heap `Value`s and
/// zero `TaggedHeap` pointers (Q2 / L6.7c(c) pattern), so `park_carrier_tls()`
/// never interacts with it.
struct ActiveSerialParkScope {
    owner: ActiveSerialTurn,
    key: ThreadKey,
    bridge: ErasedParkBridge,
}

std::thread_local! {
    static SERIAL_TURN_PARK_SCOPE: Cell<Option<ActiveSerialParkScope>> = const { Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn has_active_serial_park_scope_for_test() -> bool {
    SERIAL_TURN_PARK_SCOPE.with(|slot| {
        let taken = slot.take();
        let active = taken.is_some();
        slot.set(taken);
        active
    })
}

struct SerialParkScopeGuard {
    previous: Option<ActiveSerialParkScope>,
}

impl SerialParkScopeGuard {
    fn install(scope: ActiveSerialParkScope) -> Self {
        let previous = SERIAL_TURN_PARK_SCOPE.with(|slot| slot.replace(Some(scope)));
        Self { previous }
    }
}

impl Drop for SerialParkScopeGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        SERIAL_TURN_PARK_SCOPE.with(|slot| slot.set(previous));
    }
}

/// Lend the owner's `Context` to `body` while routing blocking Lisp primitives
/// (`thread-yield` in E5.3a; `thread-join` in E5.3b; `mutex-lock` and
/// `condition-wait` in E5.4a; `Native`/`Host` waits in Row 3 / E5.5) through
/// the L6 serial-ring park/resume protocol (Q6a, same pattern as L6.7c(c)'s
/// [`LiveSerial::with_world_collectible`]).
#[cold]
#[inline(never)]
#[allow(dead_code)] // Dormant E5.3a; E6-P parallel owner loop is the production caller.
pub(crate) fn with_world_parkable<G, P, A, R>(
    live: &mut LiveSerial<'_, Context, G>,
    owner: &mut ParallelOwnerControl,
    port: &mut P,
    mut activate: impl FnMut(&mut LocalSerialOwner) -> Result<(), A>,
    body: impl for<'g> FnOnce(&'g mut Context) -> R,
) -> R
where
    G: LendGuard,
    P: KeeperPort<LendGuard = G>,
{
    let home = live.home;
    let active_slot = &mut live.active;
    let owner_key = owner.key;
    let owner_wake = owner.wake.clone();
    let control = Arc::clone(&owner.control);

    live.turn.with_world_rebind(|ctx, local_slot, mut ops| {
        let owner_identity = ActiveSerialTurn::for_context(ctx);
        let mut bridge_fn = |ev: &mut Context,
                             wait: ParkWait,
                             native_body: Option<Box<dyn FnOnce()>>|
         -> Result<ParallelWaitOutcome, SerialParkFailure> {
            // 1. Obtain the era's facade-mark exclusion epoch BEFORE mutating
            //    `Control` wait state or detaching (Blocker 1: every parked
            //    serial turn MUST publish a `RootBatchLease` covering its
            //    private stack/specpdl/lexenv/catch_stack/TLS/JIT roots so GC
            //    running while parked across Yield/Mutex/Condition/Join/Native/
            //    Host can never sweep live values).
            let epoch = port
                .collect_root_epoch()
                .ok_or(SerialParkFailure::PreDetach(
                    PreDetachParkError::MissingEpoch,
                ))?;

            // 2. Register wait in Green `Control` BEFORE detaching/returning
            //    the ticket so wakes/deadlock checks see the wait atomically.
            let registered_wait = !matches!(wait, ParkWait::Yield);
            match wait {
                ParkWait::Yield => {}
                ParkWait::Mutex(id) => control.begin_parallel_wait(owner_key, Wait::Mutex(id)),
                ParkWait::Condition(id) => {
                    control.begin_parallel_wait(owner_key, Wait::Condition(id));
                }
                ParkWait::Join(target) => {
                    control.begin_parallel_wait(owner_key, Wait::Join(target));
                }
                ParkWait::Native => control.begin_parallel_wait(owner_key, Wait::Native),
                ParkWait::Host(hk) => {
                    control.begin_parallel_wait(owner_key, Wait::HostRequest(hk));
                }
            }

            // 3. Publish parked roots (`RootBatchLease` into `TransportRootTable`
            //    + `GreenRuntime::parked`), detach host, and return the ticket
            //    before blocking. If detach refuses, roll back `Control`'s wait
            //    registration (Blocker 2) while still attached under the ticket.
            if let Err(refusal) = ops.detach_with(|t| ev.detach_serial_owner_for_collect(epoch, t))
            {
                if registered_wait {
                    control.end_parallel_wait(owner_key);
                }
                return Err(SerialParkFailure::PreDetach(PreDetachParkError::Detach(
                    refusal,
                )));
            }
            let local = local_slot
                .take()
                .expect("live serial turn holds its local owner before park");
            let (seat, guard) = active_slot
                .take()
                .expect("live serial turn holds its seat and guard before park");
            give_back(seat, local, port);
            guard.disarm();

            // 4. Block outside the ticket (holding zero World/Context views,
            //    with all private Lisp roots published in `TransportRootTable`).
            let outcome = match wait {
                ParkWait::Yield => {
                    std::thread::yield_now();
                    ParallelWaitOutcome::Woken
                }
                ParkWait::Mutex(_) | ParkWait::Condition(_) | ParkWait::Join(_) => {
                    control.wait_parallel_parked(owner_key, &owner_wake, port)
                }
                ParkWait::Native | ParkWait::Host(_) => {
                    // Row 3: `thread-signal` does NOT wake `Wait::Native` or
                    // `Wait::HostRequest`; run the native/host wait to its own
                    // completion, then transition back to `ParallelActive`.
                    if let Some(run_wait) = native_body {
                        run_wait();
                    }
                    control.end_parallel_wait(owner_key);
                    ParallelWaitOutcome::Woken
                }
            };

            // 5. Re-request a fresh serial turn at the ring tail and reattach
            //    (reattach retires the `RootBatchLease` and restores JIT pins).
            let (grant, guard2) = loop {
                match port.request_turn() {
                    Ok(granted) => break granted,
                    Err(PortRefusal::Retry) => std::thread::yield_now(),
                    Err(PortRefusal::Poisoned) => {
                        return Err(SerialParkFailure::PostDetach(PostDetachParkError::Poisoned));
                    }
                    Err(PortRefusal::Failed) => {
                        return Err(SerialParkFailure::PostDetach(PostDetachParkError::Failed));
                    }
                }
            };
            let (seat2, local2, guard2) = match receive::<P, std::convert::Infallible, A>(
                home,
                grant,
                guard2,
                port,
                |local| activate(local),
            ) {
                Ok(resumed) => resumed,
                Err(TurnStartRefusal::Receive(reason)) => {
                    return Err(SerialParkFailure::PostDetach(PostDetachParkError::Receive(
                        reason,
                    )));
                }
                Err(TurnStartRefusal::Activate(_)) => {
                    return Err(SerialParkFailure::PostDetach(PostDetachParkError::Activate));
                }
                Err(TurnStartRefusal::Port(PortRefusal::Poisoned)) => {
                    return Err(SerialParkFailure::PostDetach(PostDetachParkError::Poisoned));
                }
                Err(TurnStartRefusal::Port(PortRefusal::Failed | PortRefusal::Retry)) => {
                    return Err(SerialParkFailure::PostDetach(PostDetachParkError::Failed));
                }
                Err(TurnStartRefusal::Turn(_)) => {
                    unreachable!("receive does not call Turn::resume")
                }
            };
            *local_slot = Some(local2);
            *active_slot = Some((seat2, guard2));
            ops.reattach_with(|d, t| ev.reattach(d, t))
                .map_err(|r| SerialParkFailure::PostDetach(PostDetachParkError::Reattach(r)))?;
            Ok(outcome)
        };

        let _scope_guard = SerialParkScopeGuard::install(ActiveSerialParkScope {
            owner: owner_identity,
            key: owner_key,
            bridge: ErasedParkBridge::new(&mut bridge_fn),
        });
        body(ctx)
    })
}

impl Context {
    /// Park the currently active parallel serial turn through the scoped
    /// [`with_world_parkable`] bridge (Q6a). Fails loudly if no bridge is
    /// installed for this exact `(ctx_addr, HeapIdentity, context_instance_id)`
    /// (`ActiveSerialTurn`, NIT-1).
    ///
    /// Error discipline (Blocker 2 / L6.7c(c) F6.1):
    /// - `Ok(outcome)` or `Err(PreDetach(_))`: `Context` is attached and holds
    ///   its serial ticket, so `SERIAL_TURN_PARK_SCOPE` is restored cleanly.
    /// - `Err(PostDetach(error))`: carrier TLS is parked and the ticket was
    ///   returned, so we fail closed via `panic!` without restoring the broken
    ///   scope, without allocating on the detached heap, and without returning
    ///   a catchable Lisp `Flow::Signal`.
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_park_in_turn(
        &mut self,
        op_name: &str,
        wait: ParkWait,
        native_body: Option<Box<dyn FnOnce()>>,
    ) -> Result<(ThreadKey, ParallelWaitOutcome), Flow> {
        let actual = ActiveSerialTurn::for_context(self);
        let taken = SERIAL_TURN_PARK_SCOPE.with(Cell::take);
        let Some(scope) = taken else {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(format!(
                    "neovm internal error: {op_name} called in an unbridged parallel serial turn"
                ))],
            ));
        };
        if scope.owner != actual {
            let expected = scope.owner;
            SERIAL_TURN_PARK_SCOPE.with(|slot| slot.set(Some(scope)));
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(format!(
                    "neovm internal error: {op_name} park bridge mismatch for parallel serial turn (expected {expected:?}, got {actual:?})"
                ))],
            ));
        }
        let key = scope.key;
        // SAFETY:
        // - Type pairing & alignment: `scope.bridge` is an `ErasedParkBridge`
        //   constructed exclusively via `ErasedParkBridge::new::<F>(&mut bridge_fn)`,
        //   so `call`'s `F` is `data`'s pointee type and alignment.
        // - Stack liveness & initialization: `with_world_parkable` is
        //   synchronous and declares `_scope_guard` strictly after `bridge_fn`,
        //   so Rust drops `_scope_guard` (removing `scope` from TLS) strictly
        //   before `bridge_fn` drops on both normal return and panic unwind;
        //   `bridge_fn` is live and initialized throughout this call.
        // - Unique access (no aliasing): `scope` matched this exact
        //   `ActiveSerialTurn` (`ctx_addr`, `HeapIdentity`, `context_instance_id`)
        //   and was moved out of `SERIAL_TURN_PARK_SCOPE` via `Cell::take`
        //   before invoking `call`, so TLS holds `None` across the entire
        //   dynamic extent of this call and no re-entrant `&mut F` borrow can
        //   coexist.
        match unsafe { scope.bridge.call(self, wait, native_body) } {
            Ok(outcome) => {
                SERIAL_TURN_PARK_SCOPE.with(|slot| slot.set(Some(scope)));
                Ok((key, outcome))
            }
            Err(SerialParkFailure::PreDetach(error)) => {
                SERIAL_TURN_PARK_SCOPE.with(|slot| slot.set(Some(scope)));
                Err(error.into_flow())
            }
            Err(SerialParkFailure::PostDetach(error)) => {
                panic!(
                    "parallel serial-turn park failed closed after detach ({op_name}): {error:?}"
                )
            }
        }
    }

    /// `(thread-yield)` inside a parallel owner's serial turn (E5.3 D6, Q6a):
    /// parks via `ParkWait::Yield` (releasing the ticket and re-requesting at
    /// the ring tail), then runs GNU `post_acquire_global_lock` signal delivery
    /// (`src/thread.c:725-732`).
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_thread_yield(&mut self) -> EvalResult {
        let (key, outcome) = self.parallel_park_in_turn("thread-yield", ParkWait::Yield, None)?;
        if let ParallelWaitOutcome::Deadlock { invariant } = outcome {
            return Err(parallel_deadlock_flow(invariant));
        }
        self.deliver_parallel_turn_signal(key)?;
        Ok(Value::NIL)
    }
}
