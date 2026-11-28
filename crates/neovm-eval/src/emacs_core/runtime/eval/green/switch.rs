//! Thread switches (plan-3d §3): a Lisp thread gives the Baton up at a
//! switch point and runs again when it comes back.
//!
//! Parking publishes the owner's private roots into the World (§4): a
//! collection run by another owner traces them there, and never reads the
//! parked Context, which stays exclusively its own thread's.

use super::super::world::{Baton, TurnRight};
use super::super::{Context, Flow, Value};
use super::carrier::CarrierJob;
use super::host_service::{
    self, HostRequestKey, MainNativeDrain, NativeSettlement, OwnerReceive, PreparedHostCall,
    RestoredHostOwner, StepKey,
};
use super::scheduler::{
    Control, Handoff, Lifecycle, NativeRelease, SchedulerIndexInconsistency, ServiceReturn,
    ThreadKey, Wait, Wake,
};
use crate::host_effect::contracts::OwnedNativeData;
use crate::host_effect::native_outcome::{CancelReason, NativeOutcome, NativeReply};
use crate::host_effect::selection::{SelectionReply, SelectionRequest};
use crate::host_effect::wake::native_wait::{NativeWaitHandoff, RealWaitPermit};
use neovm_value::error::LispCondition;
use neovm_value::error::signal;
use neovm_value::intern::SymId;
use neovm_value::tagged::gc::MarkPhase;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

#[doc(hidden)]
pub use crate::host_effect::wake::SelectTurn;

/// A selection request's terminal, decoded on the requesting owner.
enum SelectionTerminal {
    Reply(SelectionReply),
    Interrupted(CancelReason),
    HostClosed,
}

/// Decode the native settlement of a selection host request. The service's
/// executor sends exactly one `SelectionReply`; any other payload at this
/// point is a service invariant violation, not a Lisp condition.
fn decode_selection_settlement(
    _owner: &RestoredHostOwner<'_>,
    settlement: &NativeSettlement,
) -> Result<SelectionTerminal, Flow> {
    let outcome = match settlement {
        NativeSettlement::Completed(outcome) => outcome,
        NativeSettlement::CompletedAfterCancellation { outcome, .. } => outcome,
    };
    match outcome {
        NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::SelectionReply(reply))) => {
            Ok(SelectionTerminal::Reply(reply.clone()))
        }
        NativeOutcome::Reply(NativeReply::LispError(OwnedNativeData::SelectionReply(
            SelectionReply::HostError(message),
        ))) => Ok(SelectionTerminal::Reply(SelectionReply::HostError(
            message.clone(),
        ))),
        NativeOutcome::Interrupted(reason) => Ok(SelectionTerminal::Interrupted(*reason)),
        NativeOutcome::HostClosed => Ok(SelectionTerminal::HostClosed),
        other => {
            tracing::error!(
                ?other,
                "selection settlement carried a foreign native payload"
            );
            Err(selection_internal_error(
                "selection settlement payload does not match the request",
            ))
        }
    }
}

/// The existing invariant-violation shape: a debug assertion, an error log,
/// and a signal that names itself internal, so it cannot be read as GNU
/// behavior.
fn selection_internal_error(message: &str) -> Flow {
    debug_assert!(false, "selection host invariant: {message}");
    tracing::error!(message, "selection host invariant violated");
    signal(
        LispCondition::Error,
        vec![Value::string(format!("neovm internal error: {message}"))],
    )
}

/// What the scheduler concluded when no owner could run.
enum DeadlockVerdict {
    /// No runnable owner and the wait index agrees: the GNU deadlock signal.
    Blocked,
    /// No runnable owner but the wait index disagrees with the wait map: a
    /// typed internal error instead of the misleading deadlock message.
    IndexInconsistent(SchedulerIndexInconsistency),
}

/// Waits that gave the Baton up (tests read it).
#[cfg(any(test, feature = "test-support"))]
pub(crate) static SELECT_PARKS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// A failed GNU binding exchange, kept as allocation-free metadata until delivery.
#[derive(Clone, Copy, Debug)]
enum BindingResumeError {
    Constant(SymId),
    Forward(neovm_value::forward::ForwardStoreError, Value),
}

/// Ordinary waits and host service share the exact owner publication/restore path.
#[allow(
    dead_code,
    reason = "routed host calls remain dormant until hosted activation"
)]
enum ParkRequest {
    Ordinary(Wait),
    MainNative {
        wait: crate::emacs_core::eval::process_hooks::NativeWaitGuard,
        wait_end: Instant,
    },
    Host {
        call: PreparedHostCall,
        native: OwnedNativeData,
    },
    /// The already-admitted parent's next closed semantic turn.
    HostContinue(HostRequestKey),
    /// Cooperative `BetweenCommands` fairness release (E7-P.2 §1.7 Q-B).
    FairnessSlice {
        slice: Duration,
    },
}

/// The consumer runs only on its restored requesting owner. It cannot borrow
/// a mutable evaluator or run Lisp while registered reply roots are retired.
#[allow(
    dead_code,
    reason = "routed host calls remain dormant until hosted activation"
)]
enum ParkCompletion<'owner> {
    Ordinary,
    /// Published parent roots remain live; there is no terminal acknowledgement.
    Intermediate(StepKey),
    MainNative(std::io::Result<crate::host_effect::wake::ControlWakeOutcome>),
    Host {
        owner: &'owner RestoredHostOwner<'owner>,
        settlement: &'owner NativeSettlement,
    },
}

/// One canonical park may finish the parent or return its next owner step.
/// The terminal decoder is never invoked for an intermediate return.
#[allow(
    dead_code,
    reason = "owned host adapters are installed in a later range"
)]
pub(super) enum HostOwnerTurn<R> {
    Final(R),
    NeedOwner(StepKey),
}

/// Native service selects this disposition before any owner state is restored.
/// It contains only metadata; materialization waits for the canonical restore.
enum OwnerRestore {
    Ordinary,
    Intermediate(StepKey),
}

struct RestoringTurn {
    baton: TurnRight,
    disposition: OwnerRestore,
}

impl RestoringTurn {
    #[doc(hidden)]
    pub fn ordinary(baton: TurnRight) -> Self {
        Self {
            baton,
            disposition: OwnerRestore::Ordinary,
        }
    }

    /// Initial carrier entry and an ordinary native input wait cannot consume
    /// another invocation's intermediate owner continuation.
    pub(crate) fn into_ordinary_baton(self) -> TurnRight {
        match self.disposition {
            OwnerRestore::Ordinary => self.baton,
            OwnerRestore::Intermediate(_) => std::process::abort(),
        }
    }
}

/// Holds native/scheduler ownership only while main's Context is published.
enum MainWaitAuthority {
    Held(TurnRight),
    Released(NativeRelease),
    Transition,
}

#[doc(hidden)]
pub struct MainNativeHandoff<'native> {
    control: &'native Control,
    receiver: &'native mut OwnerReceive,
    wait_end: Instant,
    authority: MainWaitAuthority,
    /// The display host main attached, if any. Native waits run unhosted too
    /// (batch select/sleep), so this is optional; a hosted session keeps it
    /// available across the wait so a worker's service turn can be served.
    host: Option<&'native mut (dyn crate::emacs_core::eval::protocol::DisplayHost + 'static)>,
}

impl NativeWaitHandoff for MainNativeHandoff<'_> {
    fn release(
        &mut self,
        permit: RealWaitPermit,
    ) -> crate::host_effect::wake::native_wait::ReleaseWaitBound {
        let MainWaitAuthority::Held(baton) =
            std::mem::replace(&mut self.authority, MainWaitAuthority::Transition)
        else {
            std::process::abort()
        };
        let released = self.control.release_main_wait(permit, baton, self.wait_end);
        self.authority = MainWaitAuthority::Released(released.release);
        released.wait_bound
    }

    fn reacquire(&mut self) {
        let MainWaitAuthority::Released(release) =
            std::mem::replace(&mut self.authority, MainWaitAuthority::Transition)
        else {
            std::process::abort()
        };
        let wake = self
            .control
            .reacquire_main_wait(release, self.receiver.as_channel());
        let MainNativeHandoff {
            control,
            receiver,
            host,
            ..
        } = self;
        self.authority = MainWaitAuthority::Held(
            receive_owner_turn(ThreadKey::Main, wake, control, receiver, host)
                .into_ordinary_baton(),
        );
    }
    #[cfg(any(test, feature = "test-support"))]
    fn physical_block_entered(&mut self) {
        self.control.record_main_native_event_for_test(
            super::scheduler::MainNativeWaitEvent::PhysicalBlockEntered,
        );
    }
    #[cfg(any(test, feature = "test-support"))]
    fn physical_block_returned(&mut self) {
        self.control.record_main_native_event_for_test(
            super::scheduler::MainNativeWaitEvent::PhysicalBlockReturned,
        );
    }
}

impl MainNativeHandoff<'_> {
    pub(crate) fn into_baton(self) -> TurnRight {
        match self.authority {
            MainWaitAuthority::Held(baton) => baton,
            MainWaitAuthority::Released(_) | MainWaitAuthority::Transition => std::process::abort(),
        }
    }
}

/// Proof that a seated Green `Context` has parked its host bindings, JIT pins,
/// and private-root `RootBatchLease` into `GreenRuntime::parked`.
#[derive(Debug)]
pub(crate) struct WorldSeatParkedRoots {
    parked: super::super::ParkedOwner,
    #[cfg(feature = "jit")]
    jit: crate::emacs_core::jit::green_owner::ParkedJit,
    pub(crate) key: ThreadKey,
}

static_assertions::assert_not_impl_any!(WorldSeatParkedRoots: Send, Sync, Clone, Copy);

/// Why `Context::park_world_seat_with_roots` refused before stowing World
/// carriers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorldSeatRootError {
    Unseated,
    Retention,
    #[cfg(feature = "jit")]
    JitPrepare,
    BatchPrepare,
    BatchFinalize,
    BatchPublish,
}

/// A Lisp thread's place in its Green runtime, kept in its owner slab.
#[doc(hidden)]
pub struct GreenSeat {
    pub(crate) key: ThreadKey,
    pub(super) control: Arc<Control>,
    receiver: Option<OwnerReceive>,
    /// The World's Baton, while this thread runs.
    baton: Option<TurnRight>,
    /// A constant or native forwarder refused a rebind. Deliver this after
    /// condition-wait retakes its mutex. The owner slab traces a forwarded
    /// error's offending Lisp value, including while this owner is running.
    binding_resume_error: Option<BindingResumeError>,
    /// Native continuation only: the existing subr argument/image retains the
    /// condition and its canonical child schema retains the associated mutex.
    pub(super) condition_wait: super::condition_wait::ConditionWait,
    /// Empty census scratch retained on this owner. Publication copies its
    /// words into the typed root batch before this buffer is cleared.
    pub(super) parked_roots_buffer: Vec<Value>,
    /// Reusable empty typed storage while running. Its live lease belongs to
    /// the World's parked map until this owner has completely resumed.
    pub(super) parked_root_batch: Option<neovm_value::tagged::transport::PreparedRootBatch>,
    #[cfg(feature = "jit")]
    pub(super) parked_jit_scratch: crate::emacs_core::jit::green_owner::ParkedJitScratch,
}

impl GreenSeat {
    pub(crate) fn control(&self) -> &Arc<Control> {
        &self.control
    }

    pub(crate) fn baton_ref(&self) -> Option<&TurnRight> {
        self.baton.as_ref()
    }

    pub(crate) fn take_baton(&mut self) -> Option<TurnRight> {
        self.baton.take()
    }

    pub(crate) fn install_baton(&mut self, right: TurnRight) {
        self.baton = Some(right);
    }

    pub(crate) fn take_owner_step_baton(
        &mut self,
    ) -> Result<TurnRight, host_service::HostRootError> {
        self.baton
            .take()
            .ok_or(host_service::HostRootError::BatonRequired)
    }

    pub(crate) fn restore_owner_step_baton(&mut self, baton: TurnRight) {
        self.control.debug_assert_turn_authority(&baton);
        if self.baton.is_some() {
            tracing::error!("owner step attempted to duplicate its Green Baton");
            std::process::abort();
        }
        self.baton = Some(baton);
    }

    pub(crate) fn owner_step_receive(
        &mut self,
    ) -> Result<&mut OwnerReceive, host_service::HostRootError> {
        self.receiver
            .as_mut()
            .ok_or(host_service::HostRootError::OwnerNotParked)
    }
}

// Wake envelopes change only the cold channel payload, never the owner slot.
static_assertions::const_assert_eq!(
    std::mem::size_of::<mpsc::Receiver<Baton>>(),
    std::mem::size_of::<mpsc::Receiver<Wake>>()
);
static_assertions::const_assert_eq!(
    std::mem::size_of::<mpsc::Receiver<TurnRight>>(),
    std::mem::size_of::<mpsc::Receiver<Wake>>()
);

impl GreenSeat {
    pub(crate) fn trace_binding_resume_error(&self, visit: &mut dyn FnMut(Value)) {
        if let Some(BindingResumeError::Forward(_, value)) = self.binding_resume_error {
            visit(value);
        }
    }
}

impl std::fmt::Debug for GreenSeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GreenSeat")
            .field("key", &self.key)
            .field("running", &self.baton.is_some())
            .finish_non_exhaustive()
    }
}

/// Why a thread may not give the Baton up here.
pub(crate) fn switch_refusal(ctx: &Context) -> Option<&'static str> {
    if ctx.gc_inhibit_depth != 0 {
        Some("Cannot switch threads while garbage collection is inhibited")
    } else if ctx.gc_driver_active {
        Some("Cannot switch threads while a collection driver is active")
    } else if super::super::lifecycle::dynamic_module_call_active() {
        Some("Cannot switch threads inside a module call")
    } else {
        None
    }
}

impl Context {
    /// This owner's Green seat, if it runs in a Green runtime.
    pub(crate) fn green_seat(&mut self) -> Option<&mut GreenSeat> {
        self.owner.slab_mut()?.green_seat_mut()
    }

    /// The running thread's key, if this owner runs in a Green runtime.
    pub(crate) fn green_key(&mut self) -> Option<ThreadKey> {
        self.green_seat().map(|seat| seat.key)
    }

    /// Whether this owner is a non-main Green worker. Read-only: no seat,
    /// receiver or Baton access, so a builtin selector can call it.
    pub(crate) fn is_green_worker_owner(&self) -> bool {
        self.owner
            .slab()
            .and_then(|slab| slab.green_seat())
            .is_some_and(|seat| seat.key != ThreadKey::Main)
    }

    /// Seat this owner, which holds BATON, in the runtime CONTROL as KEY.
    pub(crate) fn take_green_seat(
        &mut self,
        key: ThreadKey,
        control: Arc<Control>,
        receiver: mpsc::Receiver<Wake>,
        baton: TurnRight,
    ) {
        control.debug_assert_turn_authority(&baton);
        self.ensure_owner_slab()
            .install_green_seat(Box::new(GreenSeat {
                key,
                control,
                receiver: Some(OwnerReceive::new(key, receiver)),
                baton: Some(baton),
                binding_resume_error: None,
                condition_wait: super::condition_wait::ConditionWait::Completed,
                parked_roots_buffer: Vec::new(),
                parked_root_batch: None,
                #[cfg(feature = "jit")]
                parked_jit_scratch: Default::default(),
            }));
    }

    /// Seat a test worker during an open parallel era where the keeper holds
    /// the World's `Baton`.
    #[cfg(test)]
    pub(super) fn take_green_seat_without_baton_for_test(
        &mut self,
        key: ThreadKey,
        control: Arc<Control>,
        receiver: mpsc::Receiver<Wake>,
    ) {
        self.ensure_owner_slab()
            .install_green_seat(Box::new(GreenSeat {
                key,
                control,
                receiver: Some(OwnerReceive::new(key, receiver)),
                baton: None,
                binding_resume_error: None,
                condition_wait: super::condition_wait::ConditionWait::Completed,
                parked_roots_buffer: Vec::new(),
                parked_root_batch: None,
                #[cfg(feature = "jit")]
                parked_jit_scratch: Default::default(),
            }));
    }

    /// Install the hosted main's real native adapter on this owner's pump.
    /// False when this owner is not the seated main (a worker, or no Green
    /// runtime yet); the caller degrades to `HostClosed` rather than aborting.
    pub(crate) fn install_main_host_effects(&mut self) -> bool {
        let Some(seat) = self.green_seat() else {
            return false;
        };
        if seat.key != ThreadKey::Main {
            return false;
        }
        let Some(receiver) = seat.receiver.as_mut() else {
            return false;
        };
        receiver.install_host_effects()
    }

    /// Copy every root only this owner holds: its private execution state
    /// and its thread-local roots, without the World's storage.
    pub(crate) fn collect_parked_roots(&mut self) -> Vec<Value> {
        let mut roots = std::mem::take(
            &mut self
                .green_seat()
                .expect("a parked owner is seated")
                .parked_roots_buffer,
        );
        debug_assert!(roots.is_empty(), "retirement clears the reusable buffer");
        // The vm_frames group includes drain_unwind_to's carried result or
        // Flow and PoppedBindingScope's popped entry and restoration lexenv.
        // These copies are published before a cleanup runs Lisp, so parking
        // inside that cleanup never exposes native-stack state to a peer.
        #[cfg(feature = "jit")]
        {
            let mut scratch =
                std::mem::take(&mut self.green_seat().expect("seated above").parked_jit_scratch);
            self.trace_private_roots_reusing(&mut roots, &mut scratch.called);
            self.green_seat().expect("seated above").parked_jit_scratch = scratch;
        }
        #[cfg(not(feature = "jit"))]
        self.trace_private_roots(&mut roots);
        // Parking needs values only, without the collection helper's group
        // timing, tagged copies, or temporary allocation for every group.
        let heap = self.tagged_heap.tagged_heap_identity();
        let epoch = self.tagged_heap.gc_collections();
        let scan = neovm_value::tagged::gc::CacheRootScan::Parked;
        super::super::for_each_thread_local_root_source(heap, epoch, scan, |_, collect| {
            collect(&mut roots);
        });
        roots
    }

    /// Give the Baton up, waiting for WAIT, and run again when it comes
    /// back (GNU's release and re-acquisition of the global lock). A no-op
    /// outside a Green runtime. Signals when switching is refused here, when
    /// no thread can run and this one waits for something only a running
    /// thread could give (GNU hangs there; neomacs reports the deadlock), or
    /// when the process is exiting. A pending `thread-signal` stays pending:
    /// [`Context::green_wait`] raises it.
    pub(crate) fn green_park(&mut self, wait: Wait) -> Result<(), Flow> {
        self.green_park_impl(ParkRequest::Ordinary(wait), |completion| match completion {
            ParkCompletion::Ordinary => Ok(()),
            ParkCompletion::Host { .. }
            | ParkCompletion::Intermediate(_)
            | ParkCompletion::MainNative(_) => std::process::abort(),
        })
    }

    /// Cooperative `BetweenCommands` fairness release (E7-P.2 §1.7 Q-B):
    /// stows Main's live seat through the single `green_park_impl` path,
    /// releases `WorldTurn` to `GroupTurn::Idle` for `slice`, resumes Main, and
    /// delivers any pending `thread-signal` symmetrically with [`Self::green_wait`]
    /// (MEDIUM #8).
    pub(crate) fn green_fairness_slice(&mut self, slice: Duration) -> Result<(), Flow> {
        let Some(key) = self.green_key() else {
            return Ok(());
        };
        self.green_park_impl(
            ParkRequest::FairnessSlice { slice },
            |completion| match completion {
                ParkCompletion::Ordinary => Ok(()),
                ParkCompletion::Host { .. }
                | ParkCompletion::Intermediate(_)
                | ParkCompletion::MainNative(_) => std::process::abort(),
            },
        )?;
        self.green_raise_pending_signal(key)
    }

    /// Consume unpublished admission before collector preflight. Every registry
    /// borrow ends before Context drains a collector; failure leaves drain-only
    /// roots in the World and cannot expose stale registered records.
    #[cold]
    #[inline(never)]
    pub(crate) fn green_abandon_prepared_host_call(
        &mut self,
        call: PreparedHostCall,
    ) -> Result<(), Flow> {
        fn root_error(error: host_service::HostRootError) -> Flow {
            signal(LispCondition::Error, vec![Value::string(error.to_string())])
        }
        let abandoned = self
            .with_held_host_registry(|registry, baton, heap, owner| {
                registry.abandon_prepared(baton, heap, owner, call)
            })
            .map_err(root_error)?;
        self.prepare_parked_root_retirement()?;
        self.with_held_host_registry(|registry, baton, heap, owner| {
            registry.retire_abandoned(baton, heap, owner, abandoned)
        })
        .map_err(root_error)
    }

    /// Borrow the seated Baton in place. No take/restore pair or registry borrow
    /// survives the closure, and the closure receives no evaluator access.
    pub(crate) fn with_held_host_registry<R>(
        &mut self,
        operation: impl FnOnce(
            &mut host_service::HostRegistry,
            &TurnRight,
            &neovm_value::tagged::gc::TaggedHeap,
            ThreadKey,
        ) -> Result<R, host_service::HostRootError>,
    ) -> Result<R, host_service::HostRootError> {
        let heap = &self.tagged_heap;
        let seat = self
            .owner
            .slab_mut()
            .and_then(|slab| slab.green_seat_mut())
            .ok_or(host_service::HostRootError::ForeignOwner)?;
        let baton = seat
            .baton
            .as_ref()
            .ok_or(host_service::HostRootError::BatonRequired)?;
        let runtime = baton.green_runtime();
        let runtime = runtime
            .as_ref()
            .ok_or(host_service::HostRootError::BatonRequired)?;
        operation(runtime.parked.borrow_mut().hosts(), baton, heap, seat.key)
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn abandon_unpublished_park_request(
        &mut self,
        request: ParkRequest,
    ) -> Result<(), Flow> {
        match request {
            ParkRequest::Host { call, native: _ } => self.green_abandon_prepared_host_call(call),
            ParkRequest::Ordinary(_)
            | ParkRequest::HostContinue(_)
            | ParkRequest::MainNative { .. }
            | ParkRequest::FairnessSlice { .. } => Ok(()),
        }
    }

    /// Reserve request roots while this original owner still holds the Baton.
    ///
    /// # Safety
    /// Every input must be this owner's live same-heap Lisp value; no callback,
    /// GC or handoff may occur until the retained batch has been published.
    #[allow(
        dead_code,
        reason = "owned host adapters are installed in a later range"
    )]
    pub(crate) unsafe fn green_prepare_host_call(
        &mut self,
        roots: &[Value],
    ) -> Result<PreparedHostCall, host_service::HostRootError> {
        let heap = &self.tagged_heap;
        let seat = self
            .owner
            .slab_mut()
            .and_then(|slab| slab.green_seat_mut())
            .ok_or(host_service::HostRootError::ForeignOwner)?;
        let baton = seat
            .baton
            .as_ref()
            .ok_or(host_service::HostRootError::BatonRequired)?;
        let runtime = baton.green_runtime();
        let runtime = runtime
            .as_ref()
            .ok_or(host_service::HostRootError::BatonRequired)?;
        // SAFETY: private original-owner admission keeps the supplied roots live
        // through retained publication while the unique Baton remains held.
        unsafe {
            runtime
                .parked
                .borrow_mut()
                .hosts()
                .prepare(baton, heap, seat.key, roots)
        }
    }

    /// Dormant routed boundary. Native service never receives this consumer;
    /// response decoding and errors stay on the requesting worker's OS thread.
    #[allow(
        dead_code,
        reason = "owned host adapters are installed in a later range"
    )]
    pub(crate) fn green_host_call<R>(
        &mut self,
        call: PreparedHostCall,
        native: OwnedNativeData,
        consume: impl FnOnce(&RestoredHostOwner<'_>, &NativeSettlement) -> Result<R, Flow>,
    ) -> Result<R, Flow> {
        self.green_park_impl(
            ParkRequest::Host { call, native },
            |completion| match completion {
                ParkCompletion::Host { owner, settlement } => consume(owner, settlement),
                ParkCompletion::Ordinary
                | ParkCompletion::Intermediate(_)
                | ParkCompletion::MainNative(_) => std::process::abort(),
            },
        )
    }

    /// One suspendable routed turn. The immutable terminal decoder is invoked
    /// only when native semantic completion has actually settled the parent.
    #[cold]
    pub(crate) fn green_host_call_turn<R>(
        &mut self,
        call: PreparedHostCall,
        native: OwnedNativeData,
        consume: impl FnOnce(&RestoredHostOwner<'_>, &NativeSettlement) -> Result<R, Flow>,
    ) -> Result<HostOwnerTurn<R>, Flow> {
        self.green_park_impl(
            ParkRequest::Host { call, native },
            |completion| match completion {
                ParkCompletion::Host { owner, settlement } => {
                    consume(owner, settlement).map(HostOwnerTurn::Final)
                }
                ParkCompletion::Intermediate(step) => Ok(HostOwnerTurn::NeedOwner(step)),
                ParkCompletion::Ordinary | ParkCompletion::MainNative(_) => std::process::abort(),
            },
        )
    }

    /// Continue after the registered owner frame has published its typed result.
    /// No new parent publication, FIFO hop or automatic signal check is added.
    #[allow(
        dead_code,
        reason = "owned host adapters are installed in a later range"
    )]
    #[cold]
    pub(crate) fn green_continue_host_turn<R>(
        &mut self,
        parent: HostRequestKey,
        consume: impl FnOnce(&RestoredHostOwner<'_>, &NativeSettlement) -> Result<R, Flow>,
    ) -> Result<HostOwnerTurn<R>, Flow> {
        self.green_park_impl(
            ParkRequest::HostContinue(parent),
            |completion| match completion {
                ParkCompletion::Host { owner, settlement } => {
                    consume(owner, settlement).map(HostOwnerTurn::Final)
                }
                ParkCompletion::Intermediate(step) => Ok(HostOwnerTurn::NeedOwner(step)),
                ParkCompletion::Ordinary | ParkCompletion::MainNative(_) => std::process::abort(),
            },
        )
    }

    /// Route one selection host request from a non-main owner to the owning
    /// main owner and decode its settlement. The request is already converted
    /// (arity, nil/string/other, result and error construction stay on this
    /// caller); no Lisp value crosses and the published root batch is empty.
    ///
    /// GNU raises a pending `thread-signal` when the global lock is
    /// reacquired (`post_acquire_global_lock`, src/thread.c:141-154); this
    /// call therefore raises it before returning a value. A signal that the
    /// cancellation reported but that is no longer pending is an internal
    /// invariant violation, never a Lisp error.
    #[cold]
    pub(crate) fn green_host_selection(
        &mut self,
        request: SelectionRequest,
    ) -> Result<SelectionReply, Flow> {
        // SAFETY: no Lisp value crosses; this original owner holds the Baton
        // and the sole mutator, and the extent publishes no roots (capacity
        // 0). No Lisp, collection or ownership handoff runs here.
        let call = match unsafe { self.green_prepare_host_call(&[]) } {
            Ok(call) => call,
            Err(error) => {
                tracing::error!(?error, "selection host request was not prepared");
                return Err(signal(
                    LispCondition::Error,
                    vec![Value::string(error.to_string())],
                ));
            }
        };
        let turn = self.green_host_call_turn(
            call,
            OwnedNativeData::SelectionRequest(request),
            decode_selection_settlement,
        )?;
        match turn {
            HostOwnerTurn::Final(SelectionTerminal::Reply(reply)) => Ok(reply),
            HostOwnerTurn::Final(SelectionTerminal::HostClosed) => Err(signal(
                LispCondition::Error,
                vec![Value::string("Display host is closed")],
            )),
            HostOwnerTurn::Final(SelectionTerminal::Interrupted(reason)) => match reason {
                CancelReason::TargetSignal => {
                    let key = self.green_key().expect("a routed owner is seated");
                    self.green_raise_pending_signal(key)?;
                    // A cancellation that reports a target signal while no
                    // signal is pending is an internal inconsistency; a Lisp
                    // error here would be mistaken for GNU behavior.
                    Err(selection_internal_error(
                        "target signal vanished before its delivery point",
                    ))
                }
                // A target quit is not a pending thread signal: GNU delivers
                // it as a quit at the target's next quit check, so surface the
                // quit flow instead of the signal-pending check.
                CancelReason::TargetQuit => Err(signal(LispCondition::Quit, vec![])),
                CancelReason::OwnerEnded => Err(signal(
                    LispCondition::Error,
                    vec![Value::string("Host request interrupted: owner ended")],
                )),
                CancelReason::SessionClosing => Err(signal(
                    LispCondition::Error,
                    vec![Value::string("Host request interrupted: session closing")],
                )),
            },
            HostOwnerTurn::NeedOwner(_) => {
                tracing::error!(
                    "selection host request produced an owner step, but its direct body runs no Lisp"
                );
                std::process::abort()
            }
        }
    }

    pub(crate) fn green_park_impl<R>(
        &mut self,
        request: ParkRequest,
        consume: impl FnOnce(ParkCompletion<'_>) -> Result<R, Flow>,
    ) -> Result<R, Flow> {
        let Some(seat) = self.green_seat() else {
            return match request {
                ParkRequest::Ordinary(_) | ParkRequest::FairnessSlice { .. } => {
                    consume(ParkCompletion::Ordinary)
                }
                ParkRequest::Host { .. }
                | ParkRequest::HostContinue(_)
                | ParkRequest::MainNative { .. } => Err(signal(
                    LispCondition::Error,
                    vec![Value::string("Host request requires a Green owner")],
                )),
            };
        };
        let key = seat.key;
        let control = Arc::clone(&seat.control);
        let is_era_park = seat.baton.as_ref().is_some_and(TurnRight::is_era);
        if let Some(refusal) = switch_refusal(self) {
            self.abandon_unpublished_park_request(request)?;
            return Err(signal("error", vec![Value::string(refusal)]));
        }
        // A failed retirement kept the restored owner's lease in the World.
        // Finish that obligation before publishing another census for this key.
        // Inside an open era, use the Q2 non-marking phase guard without
        // `drain_to_quiescent` and close open bump-allocation regions so
        // stopped collections or era exit never see `OpenRegion`.
        if is_era_park {
            self.try_retire_world_seat_parked_roots(key);
            self.tagged_heap.close_alloc_regions();
        } else if let Err(error) = self.finish_retained_parked_roots(key) {
            self.abandon_unpublished_park_request(request)?;
            return Err(error);
        }
        // Capture exact sources before any binding exchange. Operand roots
        // are copied below from trace_roots' jit_window on this owner only.
        #[cfg(feature = "jit")]
        let parked_jit = {
            let mut scratch =
                std::mem::take(&mut self.green_seat().expect("seated above").parked_jit_scratch);
            let prepared =
                crate::emacs_core::jit::green_owner::ParkedJit::prepare_reusing(self, &mut scratch);
            self.green_seat().expect("seated above").parked_jit_scratch = scratch;
            match prepared {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.abandon_unpublished_park_request(request)?;
                    return Err(signal("error", vec![Value::string(error.to_string())]));
                }
            }
        };
        let parked = self.park_owner();
        #[allow(unused_mut)]
        let mut roots = self.collect_parked_roots();
        #[cfg(feature = "jit")]
        roots.extend_from_slice(parked_jit.roots());
        macro_rules! cancel_park {
            ($error:expr) => {{
                self.return_parked_root_buffer(roots);
                #[cfg(feature = "jit")]
                parked_jit.return_scratch(
                    &mut self.green_seat().expect("seated above").parked_jit_scratch,
                );
                self.resume_owner(parked);
                self.abandon_unpublished_park_request(request)?;
                return Err(signal("error", vec![Value::string($error.to_string())]));
            }};
        }
        let prepared = self
            .green_seat()
            .expect("seated above")
            .parked_root_batch
            .take();
        let prepared = match prepared {
            Some(mut prepared) => match prepared.ensure_capacity(roots.len()) {
                Ok(()) => Ok(prepared),
                Err(error) => {
                    self.green_seat().expect("seated above").parked_root_batch = Some(prepared);
                    Err(error)
                }
            },
            None => self
                .owner
                .slab_mut()
                .and_then(|slab| slab.green_seat_mut())
                .and_then(|seat| seat.baton.as_ref())
                .expect("the running thread holds the Baton")
                .prepare_parked_batch(&self.tagged_heap, roots.len()),
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => cancel_park!(error),
        };
        // SAFETY: this is the original sole mutator, holding the Baton and
        // its marking exclusion after park_owner completed. The census was
        // copied from this heap's live roots; no GC, Lisp or handoff occurs
        // between its collection, finalization and publication.
        let finalized = match unsafe { prepared.finalize(&roots) } {
            Ok(finalized) => finalized,
            Err(failure) => {
                let (prepared, error) = failure.into_parts();
                self.green_seat().expect("seated above").parked_root_batch = Some(prepared);
                cancel_park!(error);
            }
        };
        let publication = match finalized.publish_finalized_root_batch() {
            Ok(publication) => publication,
            Err(failure) => {
                let (finalized, error) = failure.into_parts();
                drop(finalized);
                cancel_park!(error);
            }
        };
        // The immutable typed publication now owns the census. Native scratch
        // can be cleared and retained before another owner receives the Baton.
        self.return_parked_root_buffer(roots);
        let seat = self.green_seat().expect("seated above");
        let mut baton = seat
            .baton
            .take()
            .expect("the running thread holds the Baton");
        let mut receiver = seat.receiver.take().expect("a seat keeps its channel");
        // The thread record keeps what another thread may read or change
        // while this one is parked: its current buffer and its errors.
        let current = self.edit.buffers.current_buffer_id();
        self.threads.set_thread_current_buffer(key.id(), current);
        if let Some(runtime) = baton.green_runtime().as_ref() {
            runtime.outgoing_constant_error.set(
                self.failed_constant_owner_binding(true)
                    .map(|symbol| (key, symbol)),
            );
        }
        let parked_era_tls = baton.is_era();
        {
            let _era_stow_guard = parked_era_tls.then_some(EraTransitionAbortOnPanic("era-park"));
            stow_owner_state(self, &baton);
            stow_carried_state(&baton);
            if parked_era_tls {
                self.publish_parked_roots_retaining(&baton, key, publication);
                // SAFETY: owner state and carried World state are stowed above and no
                // TLS heap loan is live; the prologue resets carrier TLS before reuse.
                unsafe { super::carrier_tls::park_carrier_tls() };
                let proof =
                    super::super::parallel_entry::world_seat::prove_stowed_for_era_switch(self);
                baton.set_stowed_for_return(proof);
            } else {
                self.publish_parked_roots(&baton, key, publication);
            }
        }
        // All World/registry borrows have ended before the unique Baton moves.
        let (outcome, host_resume, native_resume) = match request {
            ParkRequest::Ordinary(wait) => (
                control.transfer(key, wait, baton, receiver.as_channel()),
                None,
                None,
            ),
            ParkRequest::FairnessSlice { slice } => (
                control.transfer_era_fairness_slice(key, baton, receiver.as_channel(), slice),
                None,
                None,
            ),
            ParkRequest::MainNative { mut wait, wait_end } => {
                let mut handoff = MainNativeHandoff {
                    control: &control,
                    receiver: &mut receiver,
                    wait_end,
                    authority: MainWaitAuthority::Held(baton),
                    host: self.display_host.as_mut().map(|host| {
                        &mut **host as &mut dyn crate::emacs_core::eval::protocol::DisplayHost
                    }),
                };
                let result = wait.wait_with(&mut handoff);
                (Ok(Wake::Resume(handoff.into_baton())), None, Some(result))
            }
            ParkRequest::Host { call, native } => {
                let (wake, resume) = host_service::dispatch_host(
                    call,
                    native,
                    baton,
                    &control,
                    receiver.as_channel(),
                );
                (Ok(wake), Some(resume), None)
            }
            ParkRequest::HostContinue(parent) => {
                let wake =
                    host_service::resume_owner_step(parent, baton, &control, receiver.as_channel());
                (
                    Ok(wake),
                    Some(host_service::HostResume::Actor(parent)),
                    None,
                )
            }
        };
        let (wake, verdict) = match outcome {
            Ok(wake) => (wake, None),
            Err(Handoff::NoRunnable { baton, invariant }) => {
                let verdict = match invariant {
                    Ok(()) => DeadlockVerdict::Blocked,
                    Err(error) => DeadlockVerdict::IndexInconsistent(error),
                };
                (Wake::Resume(baton), Some(verdict))
            }
        };
        // A service turn must not restore main's Lisp continuation, JIT pins,
        // bindings or TLS, or consume its original semantic Wait.
        let mut host = self
            .display_host
            .as_mut()
            .map(|host| &mut **host as &mut dyn crate::emacs_core::eval::protocol::DisplayHost);
        let RestoringTurn {
            mut baton,
            disposition,
        } = receive_owner_turn(key, wake, &control, &mut receiver, &mut host);
        let resumed_era = parked_era_tls || baton.is_era();
        let (exit, case_table_changed, moved, case_table) = {
            let _era_resume_guard = resumed_era.then_some(EraTransitionAbortOnPanic("era-resume"));
            if resumed_era {
                baton.clear_stowed_for_return();
                if !parked_era_tls {
                    // This Baton worker parked before the parallel era opened
                    // (Solo mode), so `park_carrier_tls` did not run on the way
                    // out. Park its stale pre-era carrier TLS now while holding
                    // the exclusive `TurnRight::Era` before unstowing so
                    // `reset_carrier_tls_for_resume` observes its parked-TLS
                    // precondition cleanly (E7-P.2 D2 pre-era Baton survival).
                    //
                    // SAFETY: this carrier currently holds the exclusive
                    // `TurnRight::Era`, World carriers are still stowed, and no
                    // Lisp/GC/heap loan is live on this OS thread yet.
                    unsafe { super::carrier_tls::park_carrier_tls() };
                }
                let restored = self.reset_carrier_tls_for_resume();
                debug_assert!(
                    restored,
                    "green_park_impl era-resume requires a GreenRuntime"
                );
            }
            // Only the returning continuation refreshes its private dispatch
            // header from canonical cells. A peer's function write updates its own
            // header, so comparing our stale header would miss that redefinition.
            self.obarray.refresh_admission();
            // Invalidate shared leaf slots before dispatch or owner mirror work,
            // and restore old-source pins while the immutable batch still roots them.
            #[cfg(feature = "jit")]
            {
                let mut scratch = std::mem::take(
                    &mut self.green_seat().expect("seated above").parked_jit_scratch,
                );
                let resumed = parked_jit.resume_reusing(self, &mut scratch);
                self.green_seat().expect("seated above").parked_jit_scratch = scratch;
                resumed.expect("the original owner resumes in its own heap");
            }
            unstow_carried_state(&baton);
            unstow_owner_state(self, &baton);
            let changed_owner = self.threads.current_thread_id() != key.id();
            let outgoing_error = baton
                .green_runtime()
                .as_ref()
                .and_then(|runtime| runtime.outgoing_constant_error.take())
                .filter(|(previous, _)| *previous != key)
                .map(|(_, symbol)| symbol);
            let returning_error = changed_owner
                .then(|| self.failed_constant_owner_binding(false))
                .flatten();
            self.threads.enter_thread(key.id());
            let (exit, case_table) = baton
                .green_runtime()
                .as_ref()
                .map(|runtime| {
                    (
                        runtime.exit_for(key, &control),
                        runtime.standard_case_table.get(),
                    )
                })
                .unwrap_or((None, self.cached_standard_case_table));
            let seat = self.green_seat().expect("seated above");
            seat.receiver = Some(receiver);
            // GNU enters this owner before unbinding the previous one. Its
            // failure wins over this owner's own rebind, and condition-wait
            // still retakes its mutex before either error is delivered.
            if let Some(symbol) = outgoing_error.or(returning_error) {
                self.green_defer_binding_resume_error(symbol);
            }
            // GNU `post_acquire_global_lock` sets the thread's buffer again: a
            // thread that killed it moved this one to another, which the
            // owner's mirrors must read from as they come back.
            let moved = self
                .threads
                .thread_current_buffer(key.id())
                .filter(|buffer| {
                    current != Some(*buffer) && self.edit.buffers.get(*buffer).is_some()
                });
            if let Some(buffer) = moved {
                let _ = self.edit.buffers.switch_current_unrecorded(buffer);
            }
            let case_table_changed =
                case_table.map(Value::bits) != self.cached_standard_case_table.map(Value::bits);
            self.cached_standard_case_table = case_table;
            self.resume_owner(parked);
            (exit, case_table_changed, moved, case_table)
        };
        if case_table_changed {
            neovm_text::emacs_core::casetab::activate_casetab_thread_locals(case_table);
        }
        if moved.is_some() {
            let _ = self.sync_current_buffer_runtime_state();
        }
        // Another thread may have edited strings in place or changed the
        // syntax tables these caches answer from.
        neovm_value::string_pos_cache::reset_string_pos_cache();
        neovm_text::emacs_core::regex::reset_regex_thread_locals();
        // Registered results/errors are decoded, acknowledged and retired on
        // this restored owner BEFORE its ordinary parked publication retires.
        let completion = match disposition {
            OwnerRestore::Intermediate(step) => {
                // This is only a metadata return. The registered frame will be
                // materialized after this complete restore and census retirement.
                // Do not decode a terminal, acknowledge the parent, or raise a
                // pending signal at an artificial owner/native hop.
                consume(ParkCompletion::Intermediate(step))
            }
            OwnerRestore::Ordinary => match host_resume {
                Some(resume) => {
                    host_service::finish_owner_reply(self, &baton, resume, |owner, settlement| {
                        consume(ParkCompletion::Host { owner, settlement })
                    })
                }
                None => match native_resume {
                    Some(result) => consume(ParkCompletion::MainNative(result)),
                    None => consume(ParkCompletion::Ordinary),
                },
            },
        };
        self.green_seat().expect("restored owner is seated").baton = Some(baton);
        let retirement = if resumed_era {
            self.try_retire_world_seat_parked_roots(key);
            Ok(())
        } else {
            self.retire_parked_roots(key)
        };
        if let Some(request) = exit {
            // The process exits: like GNU's `exit`, no cleanup form of this
            // thread runs on the way out.
            self.shutdown_request = Some(request);
            self.command_loop.running = false;
            return Err(Flow::shutdown(request));
        }
        retirement?;
        match verdict {
            Some(DeadlockVerdict::Blocked) => {
                return Err(signal(
                    "error",
                    vec![Value::string(
                        "All threads are blocked: this one would wait forever \
                         (GNU Emacs hangs here; neomacs signals)",
                    )],
                ));
            }
            Some(DeadlockVerdict::IndexInconsistent(error)) => {
                let message = format!(
                    "neomacs internal error: scheduler wait-index invariant violated: {error}"
                );
                // A handler may swallow the signal; the log keeps the
                // maintenance bug visible. Cold path, so this costs nothing.
                tracing::error!("{message}");
                return Err(signal("error", vec![Value::string(message)]));
            }
            None => {}
        }
        completion
    }

    /// [`Context::green_park`], then raise an error another thread signalled
    /// into this one (GNU `post_acquire_global_lock`).
    pub(crate) fn green_wait(&mut self, wait: Wait) -> Result<(), Flow> {
        let Some(key) = self.green_key() else {
            return Ok(());
        };
        self.green_park(wait)?;
        self.green_raise_pending_signal(key)
    }

    /// Preserve GNU's first failed rebind until the safe resume signal point.
    pub(crate) fn green_defer_binding_resume_error(&mut self, symbol: SymId) {
        if let Some(seat) = self.green_seat() {
            seat.binding_resume_error
                .get_or_insert(BindingResumeError::Constant(symbol));
        }
    }

    pub(crate) fn green_defer_forward_resume_error(
        &mut self,
        error: neovm_value::forward::ForwardStoreError,
        value: Value,
    ) {
        if let Some(seat) = self.green_seat() {
            seat.binding_resume_error
                .get_or_insert(BindingResumeError::Forward(error, value));
        }
    }

    /// Raise a failed rebind or an error another thread signalled into KEY.
    pub(crate) fn green_raise_pending_signal(&mut self, key: ThreadKey) -> Result<(), Flow> {
        if let Some(error) = self.green_seat().and_then(|seat| seat.binding_resume_error) {
            // Keep a forwarded offending value rooted in the seat while
            // constructing signal data, which may allocate a Lisp string.
            let flow = match error {
                BindingResumeError::Constant(symbol) => signal(
                    neovm_value::error::LispCondition::SettingConstant,
                    vec![Value::from_sym_id(symbol)],
                ),
                BindingResumeError::Forward(error, value) => {
                    super::super::forward_store_signal(error, value)
                }
            };
            self.green_seat()
                .expect("seated above")
                .binding_resume_error = None;
            return Err(flow);
        }
        match self.threads.take_pending_thread_signal(key.id()) {
            Some(flow) => Err(flow),
            None => Ok(()),
        }
    }

    /// GNU `thread_select`: how a wait of WAIT_TIME for input or a
    /// deadline shares the Baton.
    /// Every permitted blocking wait publishes and parks, even with no ready
    /// peer: GNU releases its global lock before select in that case too.
    /// The idle scheduler observes owned native readiness and the deadline.
    /// Outside Green, at zero timeout, or where switching is refused, the
    /// existing wait backend remains responsible for the block.
    #[doc(hidden)]
    pub fn green_select_switch(&mut self, wait_time: Duration) -> Result<SelectTurn, Flow> {
        if self.in_active_serial_turn() {
            return Err(signal(
                "error",
                vec![Value::string(
                    "neovm internal error: select wait cannot fall through to simulated threading inside a parallel serial turn",
                )],
            ));
        }
        if self.green_seat().is_none() {
            return Ok(SelectTurn::Block(wait_time));
        }
        self.green_select_turn(wait_time)
    }

    /// Arm the next Main select park for one pending TCP connect.
    ///
    /// The scheduler tests that need this live above eval, and they must not
    /// name `Control` or the park types. The returned pieces are: a receiver
    /// that yields once Main is parked on this connect, the sender the peer
    /// uses after it has written, the callback's completion, and a wake that
    /// requests quit and wakes Main.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn arm_main_connect_select_park_for_test(
        &mut self,
        process: neovm_value::heap_types::ProcessId,
    ) -> (
        mpsc::Receiver<()>,
        mpsc::Sender<()>,
        mpsc::Receiver<Result<(), String>>,
        Box<dyn Fn() + Send>,
    ) {
        self.eval_str("(thread-join (make-thread (lambda () nil)))")
            .expect("seat the owned Green scheduler");
        let control = Arc::clone(&self.green_seat().expect("Green main seat").control);
        let (parked_tx, parked_rx) = mpsc::channel();
        let (ack_tx, ack_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let cancel = self.quit_requested.clone();
        let weak = Arc::downgrade(&control);
        let wake_cancel = cancel.clone();
        let wake_weak = weak.clone();
        let wake: Box<dyn Fn() + Send> = Box::new(move || {
            wake_cancel.request();
            if let Some(control) = wake_weak.upgrade() {
                control.wake(ThreadKey::Main);
            }
        });
        control.after_next_select_park_for_test(
            ThreadKey::Main,
            super::scheduler::SelectParkExpectation::MainConnectPending(process),
            move |witness| {
                let outcome = (|| {
                    if witness.owner() != ThreadKey::Main {
                        return Err("unexpected Main Select owner".to_string());
                    }
                    match witness.into_facts() {
                        super::scheduler::SelectParkFacts::MainConnectPending(pending)
                            if pending.process() == process => {}
                        super::scheduler::SelectParkFacts::MainConnectPending(_) => {
                            return Err("wrong pending-connect owner".to_string());
                        }
                        super::scheduler::SelectParkFacts::Ordinary => {
                            return Err("wrong Main Select phase".to_string());
                        }
                    }
                    parked_tx
                        .send(())
                        .map_err(|_| "Main Select witness receiver closed".to_string())?;
                    match ack_rx.recv_timeout(Duration::from_secs(5)) {
                        Ok(()) => Ok(()),
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            Err("real output write acknowledgement missing".to_string())
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            Err("real output writer disconnected".to_string())
                        }
                    }
                })();
                if outcome.is_err() {
                    cancel.request();
                    match weak.upgrade() {
                        Some(control) => control.wake(ThreadKey::Main),
                        None => {
                            let _ = completed_tx
                                .send(Err("Main Select owner closed before failure cancellation"
                                    .to_string()));
                            return;
                        }
                    }
                }
                let _ = completed_tx.send(outcome);
            },
        );
        (parked_rx, ack_tx, completed_rx, wake)
    }

    /// Arm the next Main select park for an ordinary (no pending connect)
    /// park. Same channel contract as [`Self::arm_main_connect_select_park_for_test`];
    /// the witness must be an ordinary Main park, not a connect phase.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn arm_main_ordinary_select_park_for_test(
        &mut self,
    ) -> (
        mpsc::Receiver<()>,
        mpsc::Sender<()>,
        mpsc::Receiver<Result<(), String>>,
        Box<dyn Fn() + Send>,
    ) {
        self.eval_str("(thread-join (make-thread (lambda () nil)))")
            .expect("seat the owned Green scheduler");
        let control = Arc::clone(&self.green_seat().expect("Green main seat").control);
        let (parked_tx, parked_rx) = mpsc::channel();
        let (ack_tx, ack_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let cancel = self.quit_requested.clone();
        let weak = Arc::downgrade(&control);
        let wake_cancel = cancel.clone();
        let wake_weak = weak.clone();
        let wake: Box<dyn Fn() + Send> = Box::new(move || {
            wake_cancel.request();
            if let Some(control) = wake_weak.upgrade() {
                control.wake(ThreadKey::Main);
            }
        });
        control.after_next_select_park_for_test(
            ThreadKey::Main,
            super::scheduler::SelectParkExpectation::Ordinary,
            move |witness| {
                let outcome = (|| {
                    if witness.owner() != ThreadKey::Main {
                        return Err("unexpected Main Select owner".to_string());
                    }
                    match witness.into_facts() {
                        super::scheduler::SelectParkFacts::Ordinary => {}
                        super::scheduler::SelectParkFacts::MainConnectPending(_) => {
                            return Err("wrong Main Select phase".to_string());
                        }
                    }
                    parked_tx
                        .send(())
                        .map_err(|_| "Main Select witness receiver closed".to_string())?;
                    match ack_rx.recv_timeout(Duration::from_secs(5)) {
                        Ok(()) => Ok(()),
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            Err("real output write acknowledgement missing".to_string())
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            Err("real output writer disconnected".to_string())
                        }
                    }
                })();
                if outcome.is_err() {
                    cancel.request();
                    match weak.upgrade() {
                        Some(control) => control.wake(ThreadKey::Main),
                        None => {
                            let _ = completed_tx
                                .send(Err("Main Select owner closed before failure cancellation"
                                    .to_string()));
                            return;
                        }
                    }
                }
                let _ = completed_tx.send(outcome);
            },
        );
        (parked_rx, ack_tx, completed_rx, wake)
    }

    /// Whether Main's scheduler peer set reads Blocked right now. Read-only
    /// projection of `Control::peers` for the re-homed select tests.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn main_select_peers_blocked_for_test(&mut self) -> bool {
        let control = Arc::clone(&self.green_seat().expect("Green main seat").control);
        matches!(
            control.peers(ThreadKey::Main, Instant::now()),
            super::scheduler::Peers::Blocked
        )
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn green_select_turn(&mut self, wait_time: Duration) -> Result<SelectTurn, Flow> {
        if wait_time.is_zero() || switch_refusal(self).is_some() {
            return Ok(SelectTurn::Block(wait_time));
        }
        #[cfg(any(test, feature = "test-support"))]
        let (key, control) = {
            let seat = self.green_seat().expect("seated");
            (seat.key, Arc::clone(&seat.control))
        };
        let wait_end = Instant::now() + wait_time;
        // GNU really_call_select releases the global lock even when no peer
        // is ready. Publish this owner before the scheduler's native wait so
        // a later control publication can wake it without needing the Baton.
        #[cfg(any(test, feature = "test-support"))]
        let prepare_park = |context: &Context| {
            control
                .prepare_select_park_for_test(key, &context.processes)
                .map_err(|error| {
                    signal(LispCondition::Error, vec![Value::string(error.to_string())])
                })
        };
        #[cfg(any(test, feature = "test-support"))]
        SELECT_PARKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if self.green_key() == Some(ThreadKey::Main) {
            let control = Arc::clone(&self.green_seat().expect("seated").control);
            // When main retains an idle Baton, an earlier sleeping peer must
            // still get its deadline. Runnable peers take the Baton themselves.
            let physical_end = match control.peers(ThreadKey::Main, Instant::now()) {
                super::scheduler::Peers::Sleeping(end) => end.min(wait_end),
                super::scheduler::Peers::Ready | super::scheduler::Peers::Blocked => wait_end,
            };
            match crate::emacs_core::eval::process_hooks::prepare_main_wait(self, physical_end) {
                crate::emacs_core::eval::process_hooks::MainWaitPreparation::Ready => {}
                crate::emacs_core::eval::process_hooks::MainWaitPreparation::Native(wait) => {
                    #[cfg(any(test, feature = "test-support"))]
                    prepare_park(self)?;
                    self.green_park_impl(
                        ParkRequest::MainNative { wait, wait_end },
                        |completion| match completion {
                            ParkCompletion::MainNative(result) => {
                                result.map(|_| ()).map_err(|error| {
                                    signal(
                                        LispCondition::Error,
                                        vec![Value::string(error.to_string())],
                                    )
                                })
                            }
                            ParkCompletion::Ordinary
                            | ParkCompletion::Intermediate(_)
                            | ParkCompletion::Host { .. } => std::process::abort(),
                        },
                    )?;
                }
                crate::emacs_core::eval::process_hooks::MainWaitPreparation::Scheduler => {
                    #[cfg(any(test, feature = "test-support"))]
                    prepare_park(self)?;
                    self.green_wait(Wait::Select { wait_end })?;
                    return Ok(SelectTurn::Switched);
                }
            }
            self.green_raise_pending_signal(ThreadKey::Main)?;
        } else {
            #[cfg(any(test, feature = "test-support"))]
            prepare_park(self)?;
            self.green_wait(Wait::Select { wait_end })?;
        }
        Ok(SelectTurn::Switched)
    }

    /// Yield only when another owner can run. A sleeping selector remains
    /// watched at this safepoint, but an owner that reacquires its own turn
    /// needs no binding exchange or parked-root publication (GNU's
    /// post_acquire_global_lock also skips exchange for the same owner).
    pub(crate) fn green_yield(&mut self) -> Result<(), Flow> {
        let Some(seat) = self.green_seat() else {
            return Ok(());
        };
        let key = seat.key;
        let control = Arc::clone(&seat.control);
        if let Some(refusal) = switch_refusal(self) {
            return Err(signal("error", vec![Value::string(refusal)]));
        }
        if self.with_green_runtime(|runtime| runtime.exit.get().is_some())
            || control.yield_peer_ready(key)
        {
            return self.green_wait(Wait::Runnable);
        }
        // GNU's yield reacquires the lock even when no peer runs
        // (src/thread.c:724-732), and that `post_acquire_global_lock` raises
        // a pending error, such as one signalled before this thread ran.
        self.green_raise_pending_signal(key)
    }

    /// Main is ending (its Context drops): every other thread ends without
    /// running more Lisp, as GNU's process exit abandons them, and main
    /// joins their OS threads.
    #[cold]
    #[inline(never)]
    pub(crate) fn green_teardown(&mut self) {
        let Some(seat) = self.green_seat() else {
            return;
        };
        if seat.key != ThreadKey::Main {
            return;
        }
        let control = Arc::clone(&seat.control);
        let mut native_drain = match seat.receiver.as_mut() {
            Some(receiver) => receiver.close_native(),
            // A running main owns its receive-side storage. Losing it cannot
            // authorize dropping outstanding native tickets or parked roots.
            None => std::process::abort(),
        };
        control.close();
        loop {
            match native_drain {
                MainNativeDrain::Drained => {
                    if !control.others_could_run(ThreadKey::Main) {
                        break;
                    }
                }
                MainNativeDrain::Pending => {}
            }
            // Each hand-off ends a thread; main's own exit flow, if any, has
            // nowhere left to go. Native-only HostRequest waiters remain a
            // drain obligation even with no ordinarily runnable peer.
            let _ = self.green_park(Wait::Runnable);
            native_drain = match self.green_seat().and_then(|seat| seat.receiver.as_mut()) {
                Some(receiver) => receiver.native_drain(),
                None => std::process::abort(),
            };
        }
        // E6-P.4 Seam 2 (`E6-P4-I2`): if a live parallel keeper era is open,
        // wake every parked `Participant::Parallel` owner via `Control::close()`,
        // release Main's `TurnRight::Era` while waiting for all owners to finish
        // G5 retirement, close the keeper era, and restore `TurnRight::Solo(Baton)`
        // onto Main before draining carriers and retiring parked roots.
        self.teardown_live_parallel_era();
        // Every carrier is idle here (no Lisp thread can run after the loop
        // above). Wake and join them deterministically; idle carriers hold no
        // Baton and touch no World state.
        self.with_green_runtime(|runtime| runtime.carriers.drain());
        // All continuations have restored or terminated, and their scoped
        // collectors/readers have returned. Finish any retained failure before
        // dropping the main reservation; keep the Baton for post-teardown
        // inspection until this Context itself retires.
        if let Err(blocked) = self.finish_ended_parked_roots() {
            blocked.retain_for_shutdown();
        }
        self.green_seat().expect("main is seated").parked_root_batch = None;
    }

    /// Context retirement, while its ordinary views still keep the World alive.
    #[cold]
    #[inline(never)]
    pub(crate) fn green_release_seat(&mut self) {
        if let Some(slab) = self.owner.slab_mut() {
            drop(slab.take_green_seat());
        }
    }

    /// Test-only: borrow this seated owner's `Control` handle.
    #[cfg(test)]
    pub(crate) fn green_control_for_test(&self) -> Option<Arc<Control>> {
        self.owner
            .slab()
            .and_then(|slab| slab.green_seat())
            .map(|seat| Arc::clone(&seat.control))
    }

    /// Test-only: seat this main owner without a Solo Baton while an era is open.
    #[cfg(test)]
    pub(crate) fn seat_green_main_without_baton_for_test(&mut self, control: Arc<Control>) {
        let (_tx, rx) = mpsc::channel();
        self.take_green_seat_without_baton_for_test(ThreadKey::Main, control, rx);
    }

    /// Test-only: drain carrier pool, retire seated era main's Context, close
    /// alloc regions, stow World carriers, park carrier TLS, and return its
    /// final `TurnRight::Era` carrying `StowedForReturn`.
    #[cfg(test)]
    pub(crate) fn retire_seated_era_main_for_test(mut self) -> TurnRight {
        self.with_green_runtime(|runtime| runtime.carriers.drain());
        if let Some(key) = self.green_key() {
            self.try_retire_world_seat_parked_roots(key);
        }
        self.clear_cached_parked_root_batch_for_test();
        self.tagged_heap.close_alloc_regions();
        let mut baton = self
            .green_seat()
            .expect("seated era main")
            .baton
            .take()
            .expect("seated era main holds TurnRight::Era");
        stow_owner_state(&mut self, &baton);
        self.green_release_seat();
        self.retire_green_owner();
        stow_carried_state(&baton);
        // SAFETY: the seat is released and retired and World state is stowed, so no
        // TLS heap loan is live; the prologue resets carrier TLS before reuse.
        unsafe { super::carrier_tls::park_carrier_tls() };
        let proof =
            super::super::parallel_entry::world_seat::prove_retired_worker_stowed_for_era(&baton);
        baton.set_stowed_for_return(proof);
        baton
    }

    /// Whether this seated owner's `ThreadKey` currently has an active parked
    /// `RootBatchLease` published in `GreenRuntime::parked` (MEDIUM-1 / P1).
    pub(in crate::emacs_core::eval) fn has_published_parked_roots_for_seat(&self) -> bool {
        let Some(key) = self
            .owner
            .slab()
            .and_then(|slab| slab.green_seat())
            .map(|seat| seat.key)
        else {
            return false;
        };
        self.green_runtime_ref()
            .as_ref()
            .is_some_and(|runtime| runtime.parked.borrow().contains_key(&key))
    }

    /// Keep ROOTS traced while KEY is parked inside an open era, preserving any
    /// previously deferred lease in `retained` rather than panicking (Q2).
    fn publish_parked_roots_retaining(
        &self,
        baton: &TurnRight,
        key: ThreadKey,
        publication: neovm_value::tagged::transport::RootBatchLease,
    ) {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            return;
        };
        let mut parked = runtime.parked.borrow_mut();
        if let Some(previous) = parked.insert(key, publication) {
            parked.push_retained(previous);
        }
    }

    /// Keep ROOTS traced while KEY is parked.
    pub(crate) fn publish_parked_roots(
        &self,
        baton: &TurnRight,
        key: ThreadKey,
        publication: neovm_value::tagged::transport::RootBatchLease,
    ) {
        let runtime = baton.green_runtime();
        let runtime = runtime.as_ref().expect("a seated owner has a runtime");
        assert!(
            runtime
                .parked
                .borrow_mut()
                .insert(key, publication)
                .is_none(),
            "an owner parks exactly once before resuming"
        );
    }

    /// KEY runs again: its roots are its own Context's once more.
    pub(crate) fn retire_parked_roots(&mut self, key: ThreadKey) -> Result<(), Flow> {
        self.prepare_parked_root_retirement()?;
        let publication = self
            .with_green_runtime(|runtime| runtime.parked.borrow_mut().remove(&key))
            .expect("each returning owner has a parked batch");
        // Called only after ParkedJit::resume and resume_owner have restored
        // the original continuation. The retained Green epoch excludes a
        // concurrent marker, and no synchronous collection is active here.
        let prepared = match publication.try_recycle() {
            Ok(prepared) => prepared,
            Err(failure) => {
                // Storage reuse is optional. Busy, generation exhaustion or
                // a retained observer must never abandon the rooted lease.
                let (publication, _) = failure.into_parts();
                return self.finish_restored_publication(key, publication);
            }
        };
        let seat = self.green_seat().expect("a returning owner is seated");
        assert!(seat.parked_root_batch.is_none(), "one batch per owner");
        seat.parked_root_batch = Some(prepared);
        Ok(())
    }

    pub(crate) fn finish_retained_parked_roots(&mut self, key: ThreadKey) -> Result<(), Flow> {
        let (has_own_key, has_retained) = self.with_green_runtime(|runtime| {
            let parked = runtime.parked.borrow();
            (parked.contains_key(&key), parked.has_retained())
        });
        if !has_own_key && !has_retained {
            return Ok(());
        }
        if has_own_key {
            self.prepare_parked_root_retirement()?;
            if let Some(publication) =
                self.with_green_runtime(|runtime| runtime.parked.borrow_mut().remove(&key))
            {
                self.finish_restored_publication(key, publication)?;
            }
        }
        // Drain stale duplicate leases in `WorldRootRegistry::retained`
        // best-effort: never force `drain_to_quiescent` for an unrelated
        // owner's stuck lease and never turn a still-live reader into a Lisp
        // error (over-rooting is safe, matching `try_retire_world_seat_parked_roots`).
        if has_retained && (has_own_key || self.world_seat_retirement_phase_is_safe()) {
            self.with_green_runtime(|runtime| {
                runtime.parked.borrow_mut().drain_retained_with(|lease| {
                    super::root_registry::finish(lease).map_err(|failure| failure.into_parts().0)
                });
            });
        }
        Ok(())
    }

    pub(crate) fn finish_restored_publication(
        &mut self,
        key: ThreadKey,
        publication: neovm_value::tagged::transport::RootBatchLease,
    ) -> Result<(), Flow> {
        match super::root_registry::finish(publication) {
            Ok(()) => Ok(()),
            Err(failure) => {
                let (publication, error) = failure.into_parts();
                self.with_green_runtime(|runtime| {
                    runtime.parked.borrow_mut().insert(key, publication);
                });
                Err(signal("error", vec![Value::string(error.to_string())]))
            }
        }
    }

    /// A restored owner may retire its duplicate publication during a pending
    /// sweep: marking has finished, and sweep never reads root tables. Keep
    /// every other collector state on the complete drain and validation path.
    pub(crate) fn prepare_parked_root_retirement(&mut self) -> Result<(), Flow> {
        // switch_refusal prevents any owner from parking a synchronous driver
        // extent. Defend that proof here too, before retirement or a drain.
        if self.gc_driver_active {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Cannot retire parked roots while a collection driver is active",
                )],
            ));
        }
        let sweep_only = match self.tagged_heap.mark_phase() {
            MarkPhase::Sweeping => true,
            MarkPhase::Idle
            | MarkPhase::ConcurrentRunning
            | MarkPhase::StoppedMarking
            | MarkPhase::PublishingSweep => false,
        };
        if sweep_only && !self.tagged_heap.gc_locks_poisoned() {
            // The Baton still excludes concurrent launch; no callback,
            // safepoint or handoff runs before the transport table closes the
            // publication and checks its explicit readers. The restored owner
            // supplies its roots to every later collection in the usual way.
            return Ok(());
        }
        let heap: *mut neovm_value::tagged::gc::TaggedHeap = self.tagged_heap.heap_root().as_ptr();
        // SAFETY: the Baton holder has restored its owner; no Lisp, callback
        // or handoff runs until retirement. Reseeding uses the full registered
        // census, including every still-published owner and symbol cell.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                self.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .map_err(|error| signal("error", vec![Value::string(error.to_string())]))?;
        drop(collector);
        Ok(())
    }

    /// Retain census scratch without retaining any of its raw Lisp words.
    pub(crate) fn return_parked_root_buffer(&mut self, mut roots: Vec<Value>) {
        roots.clear();
        let seat = self.green_seat().expect("a publishing owner is seated");
        debug_assert!(seat.parked_roots_buffer.is_empty());
        seat.parked_roots_buffer = roots;
    }

    /// Whether this seated owner's heap is in a safe non-marking phase for
    /// retiring a duplicate parked `RootBatchLease` under a live `WorldTurn`
    /// without calling `drain_to_quiescent` (L6.7b b4 Q2 ruling).
    fn world_seat_retirement_phase_is_safe(&self) -> bool {
        !self.gc_driver_active
            && matches!(
                self.tagged_heap.mark_phase(),
                MarkPhase::Idle | MarkPhase::Sweeping
            )
            && !self.tagged_heap.gc_locks_poisoned()
    }

    /// Best-effort fail-closed retirement of this owner's parked lease (and
    /// any previously deferred leases) under the Q2 non-marking phase guard.
    /// Over-rooting is safe: if the guard fails or a reader is held, the lease
    /// stays live in `GreenRuntime::parked` and retires at a later safe point.
    #[cold]
    #[inline(never)]
    pub(crate) fn try_retire_world_seat_parked_roots(&mut self, key: ThreadKey) {
        if !self.world_seat_retirement_phase_is_safe() {
            return;
        }
        let publication =
            self.with_green_runtime(|runtime| runtime.parked.borrow_mut().remove(&key));
        if let Some(publication) = publication {
            match publication.try_recycle() {
                Ok(prepared) => {
                    let seat = self.green_seat().expect("a returning owner is seated");
                    if seat.parked_root_batch.is_none() {
                        seat.parked_root_batch = Some(prepared);
                    }
                }
                Err(failure) => {
                    let (publication, _) = failure.into_parts();
                    if let Err(failure) = super::root_registry::finish(publication) {
                        let (publication, _) = failure.into_parts();
                        self.with_green_runtime(|runtime| {
                            if let Some(existing) =
                                runtime.parked.borrow_mut().insert(key, publication)
                            {
                                runtime.parked.borrow_mut().push_retained(existing);
                            }
                        });
                    }
                }
            }
        }
        self.with_green_runtime(|runtime| {
            runtime.parked.borrow_mut().drain_retained_with(|lease| {
                super::root_registry::finish(lease).map_err(|failure| failure.into_parts().0)
            });
        });
    }

    /// Park a seated Green `Context`'s private roots and JIT pins into the
    /// World's `GreenRuntime::parked` table before stowing World carriers.
    ///
    /// Keeps `world_seat.rs` `#![forbid(unsafe_code)]` while sharing Green's
    /// exact census + `RootBatchLease` publication contract.
    #[cold]
    #[inline(never)]
    pub(crate) fn park_world_seat_with_roots(
        &mut self,
        retention: Option<&neovm_value::tagged::gc::FacadeEpochRetention>,
    ) -> Result<
        (
            WorldSeatParkedRoots,
            neovm_value::tagged::gc::FacadeEpochRetention,
        ),
        WorldSeatRootError,
    > {
        let (key, seat_retention) = {
            let Some(seat) = self.green_seat() else {
                return Err(WorldSeatRootError::Unseated);
            };
            let key = seat.key;
            let retained = match retention {
                Some(existing) => existing
                    .retain()
                    .map_err(|_| WorldSeatRootError::Retention)?,
                None => match seat.baton.as_ref() {
                    Some(baton) => baton
                        .parallel_epoch_retention()
                        .map_err(|_| WorldSeatRootError::Retention)?,
                    None => return Err(WorldSeatRootError::Retention),
                },
            };
            (key, retained)
        };
        // Opportunistically retire any previously deferred lease for this key
        // before publishing a fresh census. If retirement still refuses (e.g.
        // a held reader), preserve the old lease in `retained` below so zero
        // live leases are dropped.
        self.try_retire_world_seat_parked_roots(key);
        // Close any open bump-allocation regions before the World turn ends so
        // stopped collections never trip `ImageError::RegionsOpen`.
        self.tagged_heap.close_alloc_regions();
        #[cfg(feature = "jit")]
        let parked_jit = {
            let mut scratch =
                std::mem::take(&mut self.green_seat().expect("seated above").parked_jit_scratch);
            let prepared =
                crate::emacs_core::jit::green_owner::ParkedJit::prepare_reusing(self, &mut scratch);
            self.green_seat().expect("seated above").parked_jit_scratch = scratch;
            prepared.map_err(|_| WorldSeatRootError::JitPrepare)?
        };
        let parked = self.park_owner();
        #[allow(unused_mut)]
        let mut roots = self.collect_parked_roots();
        #[cfg(feature = "jit")]
        roots.extend_from_slice(parked_jit.roots());
        macro_rules! rollback_park {
            ($err:expr) => {{
                self.return_parked_root_buffer(roots);
                #[cfg(feature = "jit")]
                parked_jit.return_scratch(
                    &mut self.green_seat().expect("seated above").parked_jit_scratch,
                );
                self.resume_owner(parked);
                return Err($err);
            }};
        }
        let prepared = self
            .green_seat()
            .expect("seated above")
            .parked_root_batch
            .take();
        let prepared = match prepared {
            Some(mut prepared) => match prepared.ensure_capacity(roots.len()) {
                Ok(()) => Ok(prepared),
                Err(_) => {
                    self.green_seat().expect("seated above").parked_root_batch = Some(prepared);
                    Err(WorldSeatRootError::BatchPrepare)
                }
            },
            None => match seat_retention.retain() {
                Ok(batch_epoch) => {
                    neovm_value::tagged::transport::PreparedRootBatch::reserve_retained(
                        &self.tagged_heap,
                        batch_epoch,
                        roots.len(),
                    )
                    .map_err(|_| WorldSeatRootError::BatchPrepare)
                }
                Err(_) => Err(WorldSeatRootError::Retention),
            },
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(err) => rollback_park!(err),
        };
        // SAFETY: this seated owner holds exclusive World access after
        // `park_owner` completed. The census was copied from this heap's live
        // roots; no GC, Lisp or handoff occurs between collection,
        // finalization and publication.
        let finalized = match unsafe { prepared.finalize(&roots) } {
            Ok(finalized) => finalized,
            Err(failure) => {
                let (prepared, _) = failure.into_parts();
                self.green_seat().expect("seated above").parked_root_batch = Some(prepared);
                rollback_park!(WorldSeatRootError::BatchFinalize);
            }
        };
        let publication = match finalized.publish_finalized_root_batch() {
            Ok(publication) => publication,
            Err(failure) => {
                let (finalized, _) = failure.into_parts();
                drop(finalized);
                rollback_park!(WorldSeatRootError::BatchPublish);
            }
        };
        self.return_parked_root_buffer(roots);
        self.with_green_runtime(|runtime| {
            let mut parked_map = runtime.parked.borrow_mut();
            if let Some(previous) = parked_map.insert(key, publication) {
                parked_map.push_retained(previous);
            }
        });
        Ok((
            WorldSeatParkedRoots {
                parked,
                #[cfg(feature = "jit")]
                jit: parked_jit,
                key,
            },
            seat_retention,
        ))
    }

    /// Restore a seated Green `Context`'s JIT pins and host bindings FIRST,
    /// then retire its parked `RootBatchLease` under the Q2 non-marking phase
    /// guard without calling `drain_to_quiescent`.
    #[cold]
    #[inline(never)]
    pub(crate) fn resume_world_seat_with_roots(&mut self, parked_roots: WorldSeatParkedRoots) {
        self.obarray.refresh_admission();
        #[cfg(feature = "jit")]
        {
            let mut scratch = std::mem::take(
                &mut self
                    .green_seat()
                    .expect("returning owner is seated")
                    .parked_jit_scratch,
            );
            let resumed = parked_roots.jit.resume_reusing(self, &mut scratch);
            self.green_seat()
                .expect("returning owner is seated")
                .parked_jit_scratch = scratch;
            resumed.expect("the original seated owner resumes in its own heap");
        }
        self.resume_owner(parked_roots.parked);
        self.try_retire_world_seat_parked_roots(parked_roots.key);
    }

    /// True when this Context is seated in a Green runtime and currently holds
    /// an open parallel-era turn right (`TurnRight::Era`, QD1).
    ///
    /// Cold-path check only (never called from `gc_safe_point_exact_should_collect`).
    #[inline]
    pub(crate) fn green_seat_holds_era_turn(&self) -> bool {
        self.owner
            .slab()
            .and_then(|slab| slab.green_seat())
            .and_then(|seat| seat.baton.as_ref())
            .is_some_and(super::super::world::TurnRight::is_era)
    }

    #[cfg(test)]
    pub(crate) fn green_era_turn_epoch_for_test(&self) -> Option<std::num::NonZeroU64> {
        self.owner
            .slab()
            .and_then(|slab| slab.green_seat())
            .and_then(|seat| seat.baton.as_ref())
            .and_then(super::super::world::TurnRight::era_turn_epoch_for_test)
    }

    /// L6.7b b4.4: run a synchronous collection from a seated Baton thread
    /// inside an open parallel era via `ActiveWorldSeat::exit` ->
    /// `Control::collect_era_interval` -> `ParkedWorldSeat::enter`.
    ///
    /// Called strictly BEFORE `gc_driver_active = true` (Q2: the keeper is the
    /// GC driver, so `ParkedWorldSeat::enter` can recycle its parked
    /// `RootBatchLease` immediately on resume). Never falls through to direct
    /// `TaggedHeap` collection on refusal.
    #[cold]
    #[inline(never)]
    pub(crate) fn gc_collect_era_interval(&mut self, _force_complete: bool) -> Result<(), Flow> {
        use super::super::parallel_entry::world_seat::ActiveWorldSeat;
        use super::super::world::{EraTurnRight, TurnRight};

        debug_assert!(
            !self.gc_driver_active,
            "b4.4 Q2: gc_driver_active must remain false across era collection"
        );
        // 1. Pre-stop World prep while this Baton thread holds the World and
        //    heap TLS (GNU `src/alloc.c:5796-5800`).
        neovm_edit::emacs_core::undo::compact_buffers_for_gc(self);
        let start = std::time::Instant::now();
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();

        // 2. Take the active `TurnRight::Era` and `Control` from `GreenSeat`.
        let (control, era_right) = {
            let seat = self
                .green_seat()
                .expect("green_seat_holds_era_turn verified an active GreenSeat");
            let control = Arc::clone(&seat.control);
            let right = seat
                .baton
                .take()
                .expect("green_seat_holds_era_turn verified TurnRight::Era");
            let era_right = match right.into_era() {
                Ok(era) => era,
                Err(solo) => {
                    seat.baton = Some(solo);
                    unreachable!("green_seat_holds_era_turn verified TurnRight::Era");
                }
            };
            (control, era_right)
        };

        // 3. Decompose `EraTurnRight` (`into_parts` restores `World::baton_holder`
        //    to the keeper thread, HIGH-1) and adopt into `ActiveWorldSeat` so
        //    `ActiveWorldSeat::exit` mints `StowedForReturn` strictly via
        //    `world_seat` (Nit 6).
        let (turn, lend, world_handle) = era_right.into_parts();
        let seat_retention = match world_handle.parallel_epoch_retention() {
            Ok(retention) => retention,
            Err(error) => {
                let restored = TurnRight::Era(Box::new(
                    EraTurnRight::from_parts(turn, lend, world_handle)
                        .expect("turn and world_handle came from the same EraTurnRight"),
                ));
                restored.enter();
                self.green_seat().expect("seated").baton = Some(restored);
                return Err(signal(
                    LispCondition::Error,
                    vec![Value::string(format!(
                        "era collection metadata retention refused: {error:?}"
                    ))],
                ));
            }
        };
        let active = match ActiveWorldSeat::from_active_green_seat(self, turn, lend, seat_retention)
        {
            Ok(active) => active,
            Err((turn, lend, retention, refusal)) => {
                drop(retention);
                let restored = TurnRight::Era(Box::new(
                    EraTurnRight::from_parts(turn, lend, world_handle)
                        .expect("turn and world_handle came from the same EraTurnRight"),
                ));
                restored.enter();
                self.green_seat().expect("seated").baton = Some(restored);
                return Err(signal(
                    LispCondition::Error,
                    vec![Value::string(format!(
                        "era collection seat adoption refused: {refusal:?}"
                    ))],
                ));
            }
        };

        // 4. Exit the Baton-group interval: closes open alloc regions, parks
        //    host bindings + JIT pins, publishes parked roots into
        //    `GreenRuntime::parked`, stows World carriers to
        //    `CarriedHolder::Stowed`, parks carrier TLS, and mints
        //    `WorldReturned` via `WorldReturned::from_stowed_seat`.
        // SAFETY: the active seat is the caller-owned `self` turn, its World carriers
        // are stowed by `exit` itself under the same contract as `switch_out`, and the
        // returned lend is consumed before this frame yields.
        let (parked, lend, returned) = unsafe { active.exit(self) };

        // 5. Ask the keeper to collect first in its next stop and re-lend the
        //    World to this Baton group at ring HEAD. `Control::collect_era_interval`
        //    holds `GroupTurn::Collecting` unlocked across `GroupPort::collect`
        //    so sibling wakes join FIFO without emitting a duplicate request.
        //
        //    Fail-closed policy (LOW-2, matching b4.3's poisoned-era abort in
        //    `Control::await_era_world_with_port` / `EraTransitionAbortOnPanic`):
        //    once `WorldReturn` is in flight (`GroupPort::collect` cleared
        //    `lend.armed`), carrier TLS is parked and `parked` holds a retained
        //    `RootBatchLease` in `GreenRuntime::parked`. Unwinding a Baton
        //    thread without a `WorldTurn` would leak that lease and leave a
        //    half-parked seat, so a refusal or closed report channel aborts the
        //    process cleanly rather than panicking or falling through to direct
        //    `TaggedHeap` collection.
        let (report_tx, report_rx) = mpsc::sync_channel(1);
        let (turn2, _epoch2, lend2) = match control.collect_era_interval(
            lend,
            returned,
            Some(report_tx),
        ) {
            Ok(resumed) => resumed,
            Err(refusal) => {
                tracing::error!(
                    ?refusal,
                    "era collection refused by keeper driver while seat is parked; aborting per b4.3 poisoned-era policy"
                );
                std::process::abort();
            }
        };
        let report = match report_rx.recv() {
            Ok(Ok(report)) => report,
            Ok(Err(refusal)) => {
                tracing::error!(
                    ?refusal,
                    "era collection report refused after re-grant while seat is parked; aborting per b4.3 poisoned-era policy"
                );
                std::process::abort();
            }
            Err(_) => {
                tracing::error!(
                    "era collection report channel closed after re-grant while seat is parked; aborting per b4.3 poisoned-era policy"
                );
                std::process::abort();
            }
        };

        // 6. Re-enter the seated Baton Context on the re-granted WorldTurn
        //    (restores carrier TLS, unstows World carriers, resumes JIT + host
        //    bindings, and retires parked `RootBatchLease` under Q2).
        let resumed = parked
            .enter(self, turn2, lend2)
            .expect("re-entering seated Baton Context after keeper collection must succeed");
        let (turn2, lend2, retention2) = resumed.into_active_green_parts();
        drop(retention2);
        let new_right = TurnRight::Era(Box::new(
            EraTurnRight::from_parts(turn2, lend2, world_handle)
                .expect("keeper re-granted WorldTurn must match this World's heap identity"),
        ));
        new_right.enter();
        self.green_seat().expect("seated Baton Context").baton = Some(new_right);

        // 7. Post-collection bookkeeping on the resumed Baton thread sourced
        //    from the keeper's completed collection (no second local scan).
        self.finish_keeper_collection(&report, start)
    }

    /// Test-only helper: admit a detached reader on this seated owner's parked
    /// `RootBatchLease` to exercise Q2 `ReadersOutstanding` deferral.
    #[cfg(test)]
    pub(crate) fn admit_parked_root_reader_for_test(
        &self,
    ) -> Result<
        neovm_value::tagged::transport::HeldRootBatchReaderForTest,
        neovm_value::tagged::transport::RootBatchError,
    > {
        let key = self
            .owner
            .slab()
            .and_then(|slab| slab.green_seat())
            .expect("seated owner")
            .key;
        self.with_green_runtime(|runtime| {
            let parked = runtime.parked.borrow();
            let lease = parked
                .owner_lease_for_test(&key)
                .expect("parked owner has an active RootBatchLease");
            lease.admit_detached_reader_for_test(&self.tagged_heap)
        })
    }

    /// Test-only helper: finish an already-started stopped mark extent into
    /// `MarkPhase::Sweeping` so `#![forbid(unsafe_code)]` tests can exercise
    /// the Q2 `StoppedMarking -> Sweeping` transition.
    #[cfg(test)]
    pub(crate) fn finish_stopped_mark_into_sweep_for_test(&mut self) {
        let heap: *mut neovm_value::tagged::gc::TaggedHeap = self.tagged_heap.heap_root().as_ptr();
        // SAFETY: sole owner in a single-threaded unit test; no concurrent
        // collector, callback, or handoff runs during this root seed.
        unsafe { self.seed_registered_mutator_roots_world_stopped(heap) };
        self.tagged_heap.incremental_drain_all();
        let bytes = self.tagged_heap.live_bytes();
        self.tagged_heap
            .incremental_finish_for_test(bytes, std::time::Instant::now());
    }

    /// Test-only helper: whether this seated owner holds a recycled
    /// `PreparedRootBatch` cached on its `GreenSeat` (proving retirement took
    /// the `RootBatchLease::try_recycle` fast path rather than the
    /// `root_registry::finish` fallback).
    #[cfg(test)]
    pub(crate) fn parked_root_batch_is_cached_for_test(&mut self) -> bool {
        self.green_seat()
            .is_some_and(|seat| seat.parked_root_batch.is_some())
    }

    /// Test-only helper: drop any cached recycled `PreparedRootBatch` on this
    /// seated owner's `GreenSeat` before teardown.
    #[cfg(test)]
    pub(crate) fn clear_cached_parked_root_batch_for_test(&mut self) {
        if let Some(seat) = self.green_seat() {
            seat.parked_root_batch = None;
        }
    }

    /// GNU `make-thread` (ARGS: FUNCTION, NAME, BUFFER-DISPOSITION) in a
    /// Green World: a Lisp thread on its own OS thread, starting in this
    /// thread's current buffer and running in its turn. This thread keeps
    /// running (GNU does not yield here). Starts the Green runtime on first
    /// use. Returns the thread object.
    #[cold]
    #[inline(never)]
    pub(crate) fn green_spawn(&mut self, args: &[Value]) -> Result<Value, Flow> {
        if self.green_seat().is_none() && !self.in_active_serial_turn() {
            self.start_green_threads()
                .map_err(|error| signal("error", vec![Value::string(error.to_string())]))?;
        }
        let current = self.edit.buffers.current_buffer_id();
        let (id, function) =
            crate::emacs_core::threads::prepare_make_thread(&mut self.threads, args)?;
        self.threads.set_thread_current_buffer(id, current);
        let handle = self.threads.thread_handle(id).unwrap_or(Value::NIL);
        let key = ThreadKey::from_id(id);
        let control = self.green_control();
        let receiver = control.register_worker(key);
        self.with_green_runtime(|runtime| {
            runtime.pending_starts.borrow_mut().insert(key, function);
        });
        let job = CarrierJob {
            key,
            receiver,
            control: Arc::clone(&control),
            // Set by the pool when an idle carrier is handed this job.
            reused: false,
        };
        // Reuse an idle carrier when one is parked, otherwise spawn one. The
        // Lisp thread object above is already fresh; only the OS executor is
        // recycled.
        if let Err(refusal) = self.with_green_runtime(|runtime| runtime.carriers.dispatch(job)) {
            // GNU `make-thread` (src/thread.c:933-938) restores the previous
            // situation (the never-started thread never appears in
            // `all-threads`) and then signals; the same resource failure must
            // not abort the editor. Undo every Lisp-side and scheduler-side
            // registration before the signal.
            let refused = refusal.job.key;
            self.with_green_runtime(|runtime| {
                runtime.pending_starts.borrow_mut().remove(&refused);
            });
            control.unregister_worker(refused);
            self.threads.discard_unstarted_thread(id);
            drop(refusal);
            return Err(signal(
                "error",
                vec![Value::string("Could not start a new thread")],
            ));
        }
        Ok(handle)
    }

    /// Start the Green runtime and seat this owner as its main thread.
    #[cold]
    #[inline(never)]
    pub(crate) fn start_green_threads(&mut self) -> Result<(), super::GreenThreadStartError> {
        self.green_admission_readiness()
            .map_err(super::GreenThreadStartError::Admission)?;
        let baton = self.start_green_runtime()?;
        // The main carrier runs Lisp on this OS thread; reserve its dense
        // SymId/NameId caches as worker owners do on creation.
        neovm_value::intern::prepare_thread_symbol_caches();
        let control = self.with_green_runtime(|runtime| Arc::clone(&runtime.control));
        control.install_select(crate::emacs_core::eval::process_hooks::select_handle(self));
        let receiver = control.register_main();
        self.take_green_seat(ThreadKey::Main, control, receiver, baton.into());
        // A host attached before Green start marks the World hosted; seat
        // main's adapter now that the pump exists. Without a host this is a
        // no-op and every host request settles closed.
        if self.in_hosted_green_session() && !self.install_main_host_effects() {
            tracing::warn!(
                "hosted Green session started without a native host adapter; host requests will settle closed"
            );
        }
        Ok(())
    }

    /// Run OP on this World's Green runtime (which has started).
    pub(crate) fn with_green_runtime<R>(&self, op: impl FnOnce(&super::GreenRuntime) -> R) -> R {
        let runtime = self.green_runtime_ref();
        op(runtime.as_ref().expect("the Green runtime has started"))
    }
}

/// Native service runs on evaluator-host main before ANY owner restoration.
/// Returning from it resumes main's original channel wait, not its Lisp Wait.
fn receive_owner_turn(
    key: ThreadKey,
    mut wake: Wake,
    control: &Control,
    receiver: &mut OwnerReceive,
    host: &mut Option<&mut (dyn crate::emacs_core::eval::protocol::DisplayHost + 'static)>,
) -> RestoringTurn {
    let _abort_on_panic = AbortOnPanic;
    loop {
        wake = control.intercept_acquire(key, wake, receiver.as_channel());
        let result = match wake {
            Wake::Resume(baton) => {
                control.debug_assert_turn_authority(&baton);
                baton.enter();
                return RestoringTurn::ordinary(baton);
            }
            Wake::OwnerStep { key: parent, baton } => {
                control.debug_assert_turn_authority(&baton);
                return restoring_owner_step(key, parent, baton);
            }
            Wake::ContinuationTurn { key: parent, baton } => {
                if key != ThreadKey::Main {
                    tracing::error!(?key, "native continuation addressed a non-main owner");
                    std::process::abort();
                }
                receiver.continue_owner_step(
                    parent,
                    baton,
                    control,
                    host.as_mut().map(|host| &mut **host),
                )
            }
            Wake::ServiceTurn {
                key: request,
                baton,
            } => {
                if key != ThreadKey::Main {
                    tracing::error!(?key, "native service addressed a non-main owner");
                    std::process::abort();
                }
                receiver.serve(
                    request,
                    baton,
                    control,
                    host.as_mut().map(|host| &mut **host),
                )
            }
            Wake::NativePump { baton } => {
                if key != ThreadKey::Main {
                    tracing::error!(?key, "native pump addressed a non-main owner");
                    std::process::abort();
                }
                receiver.drive(baton, control, host.as_mut().map(|host| &mut **host))
            }
        };
        match result {
            // Already qualified by the native scheduler. Re-intercepting would
            // steal a ready original main turn merely because A remains pending.
            ServiceReturn::ResumeMain(baton) => {
                control.debug_assert_turn_authority(&baton);
                baton.enter();
                return RestoringTurn::ordinary(baton);
            }
            ServiceReturn::ResumeOwnerStep { key: parent, baton } => {
                control.debug_assert_turn_authority(&baton);
                return restoring_owner_step(key, parent, baton);
            }
            ServiceReturn::Sent => {
                wake = match receiver.as_channel().recv() {
                    Ok(wake) => wake,
                    Err(_) => std::process::abort(),
                };
            }
        }
    }
}

/// An OwnerStep wake carries no execution authority. Resolve the durable
/// expected step under the Baton; heap materialization happens only after the
/// original owner's JIT/carried/binding restoration has completed.
fn restoring_owner_step(
    owner: ThreadKey,
    parent: HostRequestKey,
    baton: TurnRight,
) -> RestoringTurn {
    baton.enter();
    let step = {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            std::process::abort();
        };
        let mut parked = runtime.parked.borrow_mut();
        match parked.hosts().awaiting_owner_step(parent) {
            Ok(step) if step.owner() == owner => step,
            Ok(_) | Err(_) => {
                tracing::error!(
                    ?owner,
                    ?parent,
                    "owner-step wake lost its durable requester correlation"
                );
                std::process::abort();
            }
        }
    };
    RestoringTurn {
        baton,
        disposition: OwnerRestore::Intermediate(step),
    }
}

/// One carrier job: wait for the Baton, build the Lisp thread's owner, run
/// its function, record the outcome and hand the Baton on. The carrier loop
/// parks the OS thread afterwards; the job's heap-pointer TLS is nulled at
/// that park, and the remaining cache resets run here at the start of a
/// reused job, before the new owner's prologue.
pub(super) fn run_carrier_job(
    job: CarrierJob,
    pool: &super::carrier::PoolShared,
    carrier: super::carrier::CarrierId,
) -> super::carrier::ParkResult {
    let CarrierJob {
        key,
        receiver,
        control,
        reused,
    } = job;
    // A panic here would drop the Baton with this thread's owner, and every
    // parked thread would wait for it forever: end the process instead.
    let _abort_on_panic = AbortOnPanic;
    let mut receiver = OwnerReceive::new(key, receiver);
    let Ok(wake) = receiver.as_channel().recv() else {
        // The runtime went away before this thread ever ran; the pool is
        // draining and never reuses this slot.
        return super::carrier::ParkResult::Exit;
    };
    // A reused carrier went idle with its heap pointers park-nulled; prove it
    // did not acquire one while idle (debug builds), then run the deferred
    // cache resets. This is the first action of a reused job, before
    // `baton.enter` and before any owner or World state is installed. A fresh
    // carrier captures its pristine TLS reference instead.
    if reused {
        super::carrier_tls::carrier_tls_idle_state_and_check();
        super::carrier_tls::reset_carrier_tls();
        super::carrier_tls::carrier_tls_reset_and_check();
    } else {
        super::carrier_tls::carrier_tls_job_started();
    }
    #[cfg(test)]
    CARRIER_JOB_THREADS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push((key.id(), std::thread::current().id()));
    // A reused carrier's park left the heap slot empty; a fresh carrier
    // starts empty. `new_green_owner` installs the World's heap below.
    debug_assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "carrier job starts with a stale TAGGED_HEAP TLS pointer"
    );
    let mut no_host: Option<&mut (dyn crate::emacs_core::eval::protocol::DisplayHost + 'static)> =
        None;
    let mut baton =
        receive_owner_turn(key, wake, &control, &mut receiver, &mut no_host).into_ordinary_baton();
    baton.clear_stowed_for_return();
    baton.enter();
    set_job_thread_name(key);
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::green_owner::begin_owner_turn();
    unstow_carried_state(&baton);
    let mut owner = Context::new_green_owner_with_turn(&baton);
    unstow_owner_state(&mut owner, &baton);
    let completion_id = match owner.threads.get_thread(key.id()) {
        Some(thread) => thread.id(),
        None => std::process::abort(),
    };
    if control.lifecycle() == Lifecycle::Closing {
        // Main ended before this thread ever ran.
        let completion = match owner.threads.prepare_thread_completion(
            completion_id,
            crate::emacs_core::threads::CompletionExit::ClosingBeforeStart,
        ) {
            Ok(completion) => completion,
            Err(error) => error.fail_stop(),
        };
        if let Some(runtime) = baton.green_runtime().as_ref() {
            runtime.pending_starts.borrow_mut().remove(&key);
        }
        if baton.is_era() {
            owner.tagged_heap.close_alloc_regions();
        }
        stow_owner_state(&mut owner, &baton);
        owner.retire_green_owner();
        stow_carried_state(&baton);
        // Park-time heap-pointer nulls (see the prologue): the carrier must
        // name no heap or World object before it may be registered idle.
        // Park before publishing the exit: no other Lisp thread holds the
        // Baton until `finish` hands it on, so the idle registration is
        // visible to any dispatcher that runs after `exited` is observed.
        // SAFETY: `owner` is stowed and retired, carried World state is stowed,
        // no TLS heap loan is live, and the carrier prologue runs
        // `reset_carrier_tls()` before reusing this OS thread.
        unsafe { super::carrier_tls::park_carrier_tls() };
        if baton.is_era() {
            let proof =
                super::super::parallel_entry::world_seat::prove_retired_worker_stowed_for_era(
                    &baton,
                );
            baton.set_stowed_for_return(proof);
        }
        let parked = pool.park(carrier);
        let retired = baton.retire_thread_completion(completion);
        control.finish(retired, baton);
        return parked;
    }
    let outgoing_error = baton
        .green_runtime()
        .as_ref()
        .and_then(|runtime| runtime.outgoing_constant_error.take())
        .filter(|(previous, _)| *previous != key)
        .map(|(_, symbol)| symbol);
    // Enter the thread before its buffer: a buffer switch records itself as
    // the current thread's buffer.
    owner.threads.enter_thread(key.id());
    if let Some(buffer) = owner.threads.thread_current_buffer(key.id()) {
        owner.restore_current_buffer_if_live(buffer);
    }
    owner.threads.start_thread(key.id());
    // An error signalled into this thread before it ran stays pending: GNU's
    // first acquire comes before the thread has handlers, so
    // `post_acquire_global_lock` (src/thread.c:146-154) does not raise it
    // there, but leaves it in `error_symbol`. Nothing raises before the
    // function runs. Inside it, every wait sees the pending error and every
    // delivery point raises it.
    let function = baton
        .green_runtime()
        .as_ref()
        .and_then(|runtime| runtime.pending_starts.borrow_mut().remove(&key));
    owner.take_green_seat(
        key,
        Arc::clone(&control),
        receiver.into_worker_channel(),
        baton,
    );
    #[cfg(test)]
    if PANIC_NEXT_WORKER.swap(false, std::sync::atomic::Ordering::Relaxed) {
        panic!("test hook: a Green worker panics");
    }
    // GNU's analogous failure before a newborn thread has handlers crashes
    // the process. Keep the internal error as this thread's last-error;
    // established threads take the exact catchable path in green_park.
    let outcome = match (outgoing_error, function) {
        (Some(symbol), _) => Err(signal(
            neovm_value::error::LispCondition::SettingConstant,
            vec![Value::from_sym_id(symbol)],
        )),
        (None, Some(function)) => {
            owner.push_vm_root_frame();
            owner.push_vm_frame_root(function);
            let outcome = owner.apply(function, Vec::new());
            owner.pop_vm_root_frame();
            outcome
        }
        (None, None) => Ok(Value::NIL),
    };
    let exit = outcome.as_ref().err().and_then(Flow::shutdown_request);
    // A kill-emacs is the process's, not this thread's: it is not recorded.
    let _ = crate::emacs_core::threads::finish_make_thread_result(
        &mut owner.threads,
        key.id(),
        outcome,
    );
    let completion = match owner.threads.prepare_thread_completion(
        completion_id,
        crate::emacs_core::threads::CompletionExit::Executed,
    ) {
        Ok(completion) => completion,
        Err(error) => error.fail_stop(),
    };
    let seat = owner.green_seat().expect("seated above");
    let mut baton = seat
        .baton
        .take()
        .expect("a finishing thread holds the Baton");
    if let Some(runtime) = baton.green_runtime().as_ref() {
        // GNU clears prev_thread when a thread exits: no outgoing unbind.
        runtime.outgoing_constant_error.set(None);
    }
    if let Some(request) = exit
        && control.lifecycle() == Lifecycle::Running
        && let Some(runtime) = baton.green_runtime().as_ref()
    {
        // GNU's `kill-emacs` exits the process from any thread: main runs
        // next, only to leave.
        runtime.exit.set(Some(request));
        control.prioritize(ThreadKey::Main);
    }
    if baton.is_era() {
        owner.tagged_heap.close_alloc_regions();
    }
    stow_owner_state(&mut owner, &baton);
    owner.retire_green_owner();
    stow_carried_state(&baton);
    // Park-time heap-pointer nulls (see the prologue): the carrier must name
    // no heap or World object before it may be registered idle. Idle
    // registration precedes the exit publication so a dispatcher that runs
    // after `exited` is observed always finds the slot parked.
    // SAFETY: `owner` is stowed and retired, carried World state is stowed,
    // no TLS heap loan is live, and the carrier prologue runs
    // `reset_carrier_tls()` before reusing this OS thread.
    unsafe { super::carrier_tls::park_carrier_tls() };
    if baton.is_era() {
        let proof =
            super::super::parallel_entry::world_seat::prove_retired_worker_stowed_for_era(&baton);
        baton.set_stowed_for_return(proof);
    }
    let parked = pool.park(carrier);
    let retired = baton.retire_thread_completion(completion);
    control.finish(retired, baton);
    parked
}

/// The World's thread-local state leaves this OS thread with the Baton
/// (`carried`).
fn stow_carried_state(baton: &TurnRight) {
    if let Some(runtime) = baton.green_runtime().as_ref() {
        runtime.carried.borrow_mut().stow();
        runtime.carried_holder.set(super::CarriedHolder::Stowed);
    }
}

/// The World's thread-local state arrives on this OS thread with the Baton.
fn unstow_carried_state(baton: &TurnRight) {
    if let Some(runtime) = baton.green_runtime().as_ref() {
        assert_eq!(
            runtime.carried_holder.get(),
            super::CarriedHolder::Stowed,
            "A-1: World carried TLS must be stowed before unstowing onto a Green owner"
        );
        runtime.carried.borrow_mut().unstow();
    }
}

/// The warm GNU-global state leaves OWNER with the Baton (`owner_carried`).
fn stow_owner_state(owner: &mut Context, baton: &TurnRight) {
    if let Some(runtime) = baton.green_runtime().as_ref() {
        let mut slot = runtime.owner_carried.borrow_mut();
        assert!(slot.is_none(), "the World's warm state is in two places");
        *slot = Some(super::owner_carried::OwnerCarried::take(owner));
    }
}

/// The warm GNU-global state arrives in OWNER with the Baton.
fn unstow_owner_state(owner: &mut Context, baton: &TurnRight) {
    let owner_slab = super::OwnerSlabId::from_slab(owner.ensure_owner_slab());
    let state = {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            return;
        };
        assert_eq!(
            runtime.carried_holder.get(),
            super::CarriedHolder::Stowed,
            "A-1: World warm carriers must be stowed before unstowing onto a Green owner"
        );
        runtime.carried_holder.set(super::CarriedHolder::Held {
            slab: owner_slab,
            thread: std::thread::current().id(),
        });
        runtime.owner_carried.borrow_mut().take()
    };
    if let Some(state) = state {
        state.put(owner);
    }
}

/// Linux `comm` name for a running job (diagnostics only). `std` fixes the
/// thread name at spawn, so the carrier renames the kernel view per job; other
/// platforms keep the carrier's spawn name.
fn set_job_thread_name(key: ThreadKey) {
    #[cfg(target_os = "linux")]
    {
        let name = format!("neovm-lisp-{}", key.id());
        let mut buf = [0u8; 16];
        let bytes = name.as_bytes();
        let n = bytes.len().min(buf.len() - 1);
        buf[..n].copy_from_slice(&bytes[..n]);
        // SAFETY: PR_SET_NAME copies a NUL-terminated string of at most 16
        // bytes from the buffer for the duration of the call.
        unsafe {
            libc::prctl(
                libc::PR_SET_NAME,
                buf.as_ptr() as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            );
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = key;
}

/// Test hook: `(Lisp thread id, OS thread id)` for every carrier job, in
/// dispatch order.
#[cfg(test)]
pub(crate) static CARRIER_JOB_THREADS: std::sync::Mutex<Vec<(u64, std::thread::ThreadId)>> =
    std::sync::Mutex::new(Vec::new());

/// Recorded `(Lisp thread id, OS thread id)` for every carrier job, in order.
#[cfg(test)]
pub(crate) fn carrier_job_threads_for_test() -> Vec<(u64, std::thread::ThreadId)> {
    CARRIER_JOB_THREADS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Aborts the process when a panic unwinds midway through `green_park_impl`'s
/// era stow/TLS-park or TLS-reset/unstow windows (matching `world_seat.rs`,
/// LOW-2).
struct EraTransitionAbortOnPanic(&'static str);

impl Drop for EraTransitionAbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            tracing::error!(
                step = self.0,
                "a seated Green era transition panicked between stow/TLS steps; aborting"
            );
            std::process::abort();
        }
    }
}

/// Aborts the process when dropped by a panic.
struct AbortOnPanic;

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let thread = std::thread::current();
            tracing::error!(
                thread = thread.name().unwrap_or("?"),
                "a Green Lisp thread panicked; its Baton would be lost, so the process aborts"
            );
            std::process::abort();
        }
    }
}

/// Test hook: the next worker panics as it starts its function.
#[cfg(test)]
pub(crate) static PANIC_NEXT_WORKER: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
