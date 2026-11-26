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
use super::super::owner_bindings::OwnerSlabHandle;
use super::keeper_driver::ChannelPort;
use super::serial_detach::{ActiveSerialTurn, HostRefusal};
use super::serial_park::{
    KeeperPort, LendGuard, LiveSerial, PortRefusal, SerialHome, TurnStartRefusal, give_back,
    receive,
};
use super::serial_ticket::{LocalSerialOwner, ReceiveReason};
use super::serial_turn::{OwnerSerialState, SerialHost, TurnRefusal};
use neovm_value::error::{EvalResult, Flow, LispCondition, expect_args, signal};
use neovm_value::tagged::gc::RendezvousError;
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

    /// G5 Step 6 (E5.3b): consume `RetiredCompletion`, publish `Wait::Exited`,
    /// wake all Baton and parallel joiners, remove this participant from
    /// `Control`, and disarm `Drop`.
    pub(crate) fn publish_exit_and_unregister(
        mut self,
        completion: crate::emacs_core::threads::RetiredCompletion,
    ) {
        if self.registered {
            self.registered = false;
            self.control
                .publish_parallel_owner_exit(self.key, completion);
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

    /// `(thread-join THREAD)` inside a parallel owner's serial turn (E5.3b D5,
    /// Row 4; GNU `Fthread_join`, `src/thread.c:1118-1144`):
    /// - Rejects self-join (`"Cannot join current thread"`).
    /// - Snapshots TARGET's injected `thread-signal` error BEFORE waiting and
    ///   roots both `args[0]` and every `Value` in that snapshot in a VM root
    ///   frame across `parallel_park_in_turn` so GC while parked cannot sweep
    ///   snapshot payloads even if TARGET catches and clears its own error.
    /// - When TARGET has not exited yet (`!control.exited(target_key)`), sets
    ///   `thread_blocker` and `WaitCondvar::Join` for this parallel thread's id
    ///   (`self.threads.current_thread_id()`, NOT `green_current_id()`) and
    ///   parks via `ParkWait::Join(target_key)` until TARGET exits or this
    ///   joiner receives a pending `thread-signal`.
    /// - Clears `wait_condvar`, `thread_blocker`, and pops the VM root frame
    ///   unconditionally on every Lisp/recoverable exit path before propagating
    ///   `wait_result?` or delivering this joiner's own pending signal AHEAD of
    ///   TARGET's snapshot (GNU `thread_join_callback`, `src/thread.c:1110-1115`).
    ///   (On a fail-closed `PostDetachParkError` panic, carrier TLS is already
    ///   parked, the serial ticket was already returned, and the abandoned
    ///   `Context` is never used again.)
    /// - Marks join observed, re-raises TARGET's snapshot if present, else
    ///   returns TARGET's stored result.
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_thread_join(&mut self, args: &[Value]) -> EvalResult {
        expect_args("thread-join", args, 1)?;
        let target_id = crate::emacs_core::threads::expect_thread_id(&self.threads, &args[0])?;
        let joiner_id = self.threads.current_thread_id();
        if target_id == joiner_id {
            return Err(signal(
                "error",
                vec![Value::string("Cannot join current thread")],
            ));
        }
        let pending = self.threads.pending_thread_signal_for_value(args[0]);
        let target_key = ThreadKey::from_id(target_id);
        let control = self.green_control();
        if !control.exited(target_key) {
            let joiner_key = ThreadKey::from_id(joiner_id);
            self.push_vm_root_frame();
            self.push_vm_frame_root(args[0]);
            if let Some(sig) = pending.as_ref().and_then(Flow::as_signal) {
                for &value in &sig.data {
                    self.push_vm_frame_root(value);
                }
            }
            // Coupling invariant: `set_thread_blocker` stores `args[0]` in the
            // blocker slot for the same span that `args[0]` is rooted in the
            // VM root frame pushed above; `clear_thread_blocker` MUST run
            // before `pop_vm_root_frame()`.
            self.threads.set_thread_blocker(joiner_id, args[0]);
            self.threads.set_wait_condvar(
                joiner_id,
                Some(crate::emacs_core::threads::WaitCondvar::Join),
            );
            let wait_result = (|| -> Result<(), Flow> {
                while !control.exited(target_key)
                    && !self.threads.has_pending_thread_signal(joiner_id)
                {
                    let (_k, outcome) = self.parallel_park_in_turn(
                        "thread-join",
                        ParkWait::Join(target_key),
                        None,
                    )?;
                    if let ParallelWaitOutcome::Deadlock { invariant } = outcome {
                        return Err(parallel_deadlock_flow(invariant));
                    }
                }
                Ok(())
            })();
            self.threads.set_wait_condvar(joiner_id, None);
            self.threads.clear_thread_blocker(joiner_id);
            self.pop_vm_root_frame();
            wait_result?;
            self.deliver_parallel_turn_signal(joiner_key)?;
        }
        self.threads.join_thread_for_value(args[0]);
        if let Some(flow) = pending {
            return Err(flow);
        }
        Ok(self.threads.thread_result_for_value(args[0]))
    }
}

/// Summary returned by [`retire_parallel_owner`] once all 6 G5 steps complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Dormant E5.3b; E6-P parallel owner loop is the production caller.
pub(crate) struct ParallelRetireReport {
    /// True if this owner had a registered `OwnerSlab` and G5 Step 5 removed
    /// and freed it from `World::owner_slabs`.
    pub(crate) slab_unregistered: bool,
}

/// Failure reported by [`retire_parallel_owner`].
///
/// Pre-retirement refusals (`Port`, `Receive`, `Activate`, `Begin`) occur
/// before `prepare_thread_completion`, `Context::drop`, or slab unregistration.
/// Once `prepare_thread_completion` has run, non-poisoned failures in Steps
/// 1+2, 3a, 4, 5, and 6 are terminal (`std::process::abort()`). The sole
/// post-retirement `Err` is [`Self::EraPoisonedAfterRetire`]: the era was
/// already poisoned (`RendezvousError::Poisoned`) when Step 4 retired the
/// `Mutator`; Step 6 exit publication still runs to wake any parked joiners,
/// and the caller MUST NOT continue using the poisoned era.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // Dormant E5.3b; E6-P parallel owner loop is the production caller.
pub(crate) enum ParallelRetireError {
    Port(PortRefusal),
    Receive(ReceiveReason),
    Activate,
    Begin(TurnRefusal<HostRefusal>),
    /// Step 4 observed `RendezvousError::Poisoned` after the owner had already
    /// retired (`Context` dropped, completion retired, slab unregistered).
    /// Step 6 exit publication still ran to wake parked joiners; the caller
    /// MUST NOT continue using this poisoned era.
    EraPoisonedAfterRetire,
}

/// Retire a parallel owner's [`Mutator`](neovm_value::tagged::gc::Mutator) from
/// the collector (G5 Step 4), retrying transient `RendezvousError::Busy` until
/// retired or permanently refused.
///
/// Gated on `any(test, feature = "test-support")` only because
/// `Mutator::retire_rooted` in `neovm-value` carries that gate until E6-P
/// opens the production mutator-retirement method; [`retire_parallel_owner`]
/// itself compiles identically with `--no-default-features`.
#[cfg(any(test, feature = "test-support"))]
#[cold]
#[inline(never)]
pub(crate) fn retire_mutator_rooted(
    mut mutator: neovm_value::tagged::gc::Mutator<'_>,
) -> Result<(), RendezvousError> {
    loop {
        match mutator.retire_rooted(&[]) {
            Ok(()) => return Ok(()),
            Err(failure) if failure.reason == RendezvousError::Busy => {
                mutator = failure.into_owner();
                std::thread::yield_now();
            }
            Err(failure) => return Err(failure.reason),
        }
    }
}

/// G5 6-step parallel-owner retirement (`E6P-INTEGRATION-MAP.md` §4 G5,
/// `E5.3-E5.4-THREAD-CONTROL.md` Q7 ruling).
///
/// Order of operations:
/// 0. Drop `slab_handle: Option<OwnerSlabHandle>` first so no handle (and
///    therefore no `OwnerSlabLease`) can survive into Step 5.
/// 1. Acquire a final serial turn, enter `owner.key()`, run `finalize(ev)`
///    inside the turn (zero unrooted `Value`s cross the turn boundary), record
///    the result / internal error + `thread-last-error` via
///    `finish_make_thread_result`.
/// 2. Unwind all remaining `specpdl` entries to depth 0 (`ev.unbind_to(0)` +
///    assert `ev.specpdl.is_empty()`, GNU `src/thread.c:743-748`), then mark
///    the thread terminal via `prepare_thread_completion(..., Executed)`
///    (`CompletingThread`), detach the shared `dynamic_module_registry`, and
///    capture `ParallelRetireWorld` + `ev.owner.raw_slab_ptr()`.
/// 3a. Run the final detach (`turn.end()` -> `park_owner`, stow World carriers,
///     close alloc regions, `park_carrier_tls`).
/// 5. Still holding the exclusive final serial ticket `(seat, local, guard)`
///    BEFORE `give_back` (excluding concurrent `RefCell` / collector access):
///    drop `state` (`Context::drop` runs safely while detached), retire the
///    native live-thread root edge (`retire_world.retire_thread_completion` ->
///    `RetiredCompletion`), and unregister/free the owner's slab
///    (`retire_world.unregister_owner_slab`).
/// 3b. Return the final serial ticket (`give_back` + `guard.disarm()`).
/// 4. Send `KeeperMsg::Exit` (`let mutator = port.exit()`) and retire the
///    `Mutator` from the collector via `retire_mutator(mutator)`.
/// 6. Publish exit to Green `Control` (`owner.publish_exit_and_unregister(retired)`),
///    consuming `RetiredCompletion` as typestate proof of Steps 1–5, setting
///    `Wait::Exited`, waking both `Participant::Baton` and
///    `Participant::Parallel` joiners, and removing the participant record.
#[cold]
#[inline(never)]
#[allow(dead_code)] // Dormant E5.3b; E6-P parallel owner loop is the production caller.
#[allow(clippy::too_many_arguments)]
pub(crate) fn retire_parallel_owner<'era, A>(
    mut state: OwnerSerialState<Context, LocalSerialOwner>,
    owner: ParallelOwnerControl,
    slab_handle: Option<OwnerSlabHandle>,
    mut port: ChannelPort<'era>,
    home: SerialHome,
    activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
    finalize: impl FnOnce(&mut Context) -> EvalResult,
    retire_mutator: impl FnOnce(neovm_value::tagged::gc::Mutator<'era>) -> Result<(), RendezvousError>,
) -> Result<ParallelRetireReport, ParallelRetireError> {
    // Drop the parallel slab handle first so no handle or `OwnerSlabLease`
    // can outlive Step 5.
    drop(slab_handle);

    let (grant, guard) = loop {
        match port.request_turn() {
            Ok(granted) => break granted,
            Err(PortRefusal::Retry) => std::thread::yield_now(),
            Err(PortRefusal::Poisoned) => {
                return Err(ParallelRetireError::Port(PortRefusal::Poisoned));
            }
            Err(PortRefusal::Failed) => {
                return Err(ParallelRetireError::Port(PortRefusal::Failed));
            }
        }
    };
    let (seat, local, guard) =
        match receive::<ChannelPort<'_>, HostRefusal, A>(home, grant, guard, &mut port, activate) {
            Ok(resumed) => resumed,
            Err(TurnStartRefusal::Receive(reason)) => {
                return Err(ParallelRetireError::Receive(reason));
            }
            Err(TurnStartRefusal::Activate(_)) => return Err(ParallelRetireError::Activate),
            Err(TurnStartRefusal::Port(refusal)) => return Err(ParallelRetireError::Port(refusal)),
            Err(TurnStartRefusal::Turn(_)) => unreachable!("receive does not call Turn::begin"),
        };

    let key = owner.key();
    let (completion, retire_world, slab_ptr, local) = {
        let mut turn = match state.begin(local) {
            Ok(turn) => turn,
            Err((local, refusal)) => {
                give_back(seat, local, &mut port);
                guard.disarm();
                return Err(ParallelRetireError::Begin(refusal));
            }
        };
        let (completion, retire_world, slab_ptr) = turn.with_world(|ev| {
            // G5 Step 1: finalize inside the final serial turn so zero
            // unrooted `Value`s cross a turn boundary, then record the outcome
            // (`ThreadPayload.result` or internal error + `thread-last-error`).
            ev.threads.enter_thread(key.id());
            let completion_id = match ev.threads.get_thread(key.id()) {
                Some(thread) => thread.id(),
                None => std::process::abort(),
            };
            let outcome = finalize(ev);
            let _ = crate::emacs_core::threads::finish_make_thread_result(
                &mut ev.threads,
                key.id(),
                outcome,
            );
            // G5 Step 2: unwind any remaining `specpdl` bindings to depth 0
            // while this thread is still entered (GNU `src/thread.c:743-748`).
            // `outcome` is already rooted in `ThreadPayload` above.
            ev.unbind_to(0);
            assert!(
                ev.specpdl.is_empty(),
                "G5 Step 2: parallel owner specpdl must be empty after unbind_to(0)"
            );
            // Mark thread terminal (`thread-live-p` -> nil) and mint the
            // linear `CompletingThread` token.
            let completion = match ev.threads.prepare_thread_completion(
                completion_id,
                crate::emacs_core::threads::CompletionExit::Executed,
            ) {
                Ok(completion) => completion,
                Err(error) => error.fail_stop(),
            };
            // Detach the World's shared dynamic-module registry before
            // `Context::drop`, matching `Context::retire_green_owner`.
            ev.detach_shared_dynamic_module_registry_for_retire();
            let retire_world = ev.parallel_retire_world();
            let slab_ptr = ev.owner.raw_slab_ptr();
            (completion, retire_world, slab_ptr)
        });
        // G5 Step 3a: final detach (`park_owner`, stow World carriers, close
        // alloc regions, `park_carrier_tls`). After `prepare_thread_completion`
        // no rollback is valid; a detach refusal fails closed like Baton's
        // `AbortOnPanic` / `CompletionError::fail_stop`.
        let local = match turn.end() {
            Ok(local) => local,
            Err((_turn, refusal)) => {
                tracing::error!(
                    ?refusal,
                    "G5 Step 3a: final detach refused after thread completion; aborting fail-closed"
                );
                std::process::abort();
            }
        };
        (completion, retire_world, slab_ptr, local)
    };

    // G5 Step 5 (under exclusive final serial ticket `(seat, local, guard)`
    // BEFORE `give_back` so `World::owner_slabs` and `ThreadManager::threads`
    // cannot race with Baton or the collector):
    // 1. Drop `state` (`Context::drop` runs safely while detached, with its
    //    shared dynamic-module registry detached and slab still live).
    // 2. Retire the native live-thread root edge -> `RetiredCompletion`.
    // 3. Remove and free this owner's `OwnerSlab` from `OwnerSlabRegistry`.
    drop(state);
    let retired = retire_world.retire_thread_completion(&local, completion);
    let slab_unregistered = if let Some(slab) = slab_ptr {
        // SAFETY: `slab_handle` was dropped at entry, `state` (owning the
        // retiring `Context` and its `OwnerSlot`) was just dropped above, and
        // `&local` proves exclusive serial-turn World authority on this thread.
        unsafe { retire_world.unregister_owner_slab(&local, slab) }
    } else {
        false
    };
    drop(retire_world);

    // G5 Step 3b: return the final serial ticket so the keeper can leave the
    // stop and process `KeeperMsg::Exit` + `Mutator::retire_rooted`.
    give_back(seat, local, &mut port);
    guard.disarm();

    // G5 Step 4: send `KeeperMsg::Exit` (shrinks `Driver::owners`) and retire
    // the `Mutator` from the collector.
    //
    // At this point the owner is ALREADY retired (`Context` dropped, live
    // completion unlinked, slab unregistered, final ticket returned):
    // - `Ok(())`: proceed to Step 6 normally.
    // - `Err(RendezvousError::Poisoned)`: the era is already poisoned; still
    //   run Step 6 (`publish_exit_and_unregister`) so parked joiners wake
    //   instead of wedging, then return `Err(EraPoisonedAfterRetire)` (caller
    //   MUST NOT continue using the poisoned era).
    // - Any other `Err(reason)` in a live, non-poisoned era: terminal
    //   `abort()`, matching Steps 1+2 and 3a, rather than leaving a registered
    //   `Mutator` with no owner.
    let mutator = port.exit();
    match retire_mutator(mutator) {
        Ok(()) => {
            // G5 Step 6: publish `Wait::Exited` in `Control` (consuming
            // `RetiredCompletion` as proof of Steps 1–5), wake all Baton and
            // parallel joiners, and remove the retired participant record.
            owner.publish_exit_and_unregister(retired);
            Ok(ParallelRetireReport { slab_unregistered })
        }
        Err(RendezvousError::Poisoned) => {
            owner.publish_exit_and_unregister(retired);
            Err(ParallelRetireError::EraPoisonedAfterRetire)
        }
        Err(reason) => {
            tracing::error!(
                ?reason,
                "G5 Step 4: owner already retired (Context dropped, completion retired, slab unregistered) but mutator retirement refused in a live era; aborting fail-closed rather than leaving a registered mutator with no owner"
            );
            std::process::abort();
        }
    }
}
