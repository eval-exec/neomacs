//! Thread switches (plan-3d §3): a Lisp thread gives the Baton up at a
//! switch point and runs again when it comes back.
//!
//! Parking publishes the owner's private roots into the World (§4): a
//! collection run by another owner traces them there, and never reads the
//! parked Context, which stays exclusively its own thread's.

use super::super::world::Baton;
use super::super::{Context, Flow, Value};
use super::host_service::{self, NativeSettlement, PreparedHostCall, RestoredHostOwner};
use super::scheduler::{Control, Handoff, Lifecycle, Peers, ServiceReturn, ThreadKey, Wait, Wake};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::signal;
use crate::emacs_core::intern::SymId;
use crate::host_effect::contracts::OwnedNativeData;
use crate::tagged::gc::MarkPhase;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// How a `thread_select` wait went ([`Context::green_select_switch`]).
pub(crate) enum SelectTurn {
    /// This thread gave the Baton up until input, a scheduler event, or its deadline.
    Switched,
    /// This thread blocks itself for this long.
    Block(Duration),
}

/// Waits that gave the Baton up (tests read it).
#[cfg(test)]
pub(crate) static SELECT_PARKS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// A failed GNU binding exchange, kept as allocation-free metadata until delivery.
#[derive(Clone, Copy, Debug)]
enum BindingResumeError {
    Constant(SymId),
    Forward(crate::emacs_core::forward::ForwardStoreError, Value),
}

/// Ordinary waits and host service share the exact owner publication/restore path.
#[allow(
    dead_code,
    reason = "routed host calls remain dormant until hosted activation"
)]
enum ParkRequest {
    Ordinary(Wait),
    Host {
        call: PreparedHostCall,
        native: OwnedNativeData,
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
    Host {
        owner: &'owner RestoredHostOwner<'owner>,
        settlement: &'owner NativeSettlement,
    },
}

/// A Lisp thread's place in its Green runtime, kept in its owner slab.
pub(crate) struct GreenSeat {
    pub(crate) key: ThreadKey,
    pub(super) control: Arc<Control>,
    receiver: Option<mpsc::Receiver<Wake>>,
    /// The World's Baton, while this thread runs.
    baton: Option<Baton>,
    /// A constant or native forwarder refused a rebind. Deliver this after
    /// condition-wait retakes its mutex. The owner slab traces a forwarded
    /// error's offending Lisp value, including while this owner is running.
    binding_resume_error: Option<BindingResumeError>,
    /// Empty census scratch retained on this owner. Publication copies its
    /// words into the typed root batch before this buffer is cleared.
    pub(super) parked_roots_buffer: Vec<Value>,
    /// Reusable empty typed storage while running. Its live lease belongs to
    /// the World's parked map until this owner has completely resumed.
    pub(super) parked_root_batch: Option<crate::tagged::transport::PreparedRootBatch>,
    #[cfg(feature = "jit")]
    pub(super) parked_jit_scratch: crate::emacs_core::jit::green_owner::ParkedJitScratch,
}

// Wake envelopes change only the cold channel payload, never the owner slot.
static_assertions::const_assert_eq!(
    std::mem::size_of::<mpsc::Receiver<Baton>>(),
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
fn switch_refusal(ctx: &Context) -> Option<&'static str> {
    if ctx.gc_inhibit_depth != 0 {
        Some("Cannot switch threads while garbage collection is inhibited")
    } else if ctx.gc_driver_active {
        Some("Cannot switch threads while a collection driver is active")
    } else if super::super::super::dynamic_module::module_call_active() {
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

    /// Seat this owner, which holds BATON, in the runtime CONTROL as KEY.
    pub(super) fn take_green_seat(
        &mut self,
        key: ThreadKey,
        control: Arc<Control>,
        receiver: mpsc::Receiver<Wake>,
        baton: Baton,
    ) {
        self.ensure_owner_slab()
            .install_green_seat(Box::new(GreenSeat {
                key,
                control,
                receiver: Some(receiver),
                baton: Some(baton),
                binding_resume_error: None,
                parked_roots_buffer: Vec::new(),
                parked_root_batch: None,
                #[cfg(feature = "jit")]
                parked_jit_scratch: Default::default(),
            }));
    }

    /// Copy every root only this owner holds: its private execution state
    /// and its thread-local roots, without the World's storage.
    pub(in crate::emacs_core::eval) fn collect_parked_roots(&mut self) -> Vec<Value> {
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
        let heap = self.tagged_heap.identity();
        let epoch = self.tagged_heap.gc_collections();
        let scan = crate::tagged::gc::CacheRootScan::Parked;
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
            ParkCompletion::Host { .. } => std::process::abort(),
        })
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
    pub(super) unsafe fn green_prepare_host_call(
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
    pub(super) fn green_host_call<R>(
        &mut self,
        call: PreparedHostCall,
        native: OwnedNativeData,
        consume: impl FnOnce(&RestoredHostOwner<'_>, &NativeSettlement) -> Result<R, Flow>,
    ) -> Result<R, Flow> {
        self.green_park_impl(
            ParkRequest::Host { call, native },
            |completion| match completion {
                ParkCompletion::Host { owner, settlement } => consume(owner, settlement),
                ParkCompletion::Ordinary => std::process::abort(),
            },
        )
    }

    fn green_park_impl<R>(
        &mut self,
        request: ParkRequest,
        consume: impl FnOnce(ParkCompletion<'_>) -> Result<R, Flow>,
    ) -> Result<R, Flow> {
        let Some(seat) = self.green_seat() else {
            return match request {
                ParkRequest::Ordinary(_) => consume(ParkCompletion::Ordinary),
                ParkRequest::Host { .. } => Err(signal(
                    LispCondition::Error,
                    vec![Value::string("Host request requires a Green owner")],
                )),
            };
        };
        let key = seat.key;
        let control = Arc::clone(&seat.control);
        if let Some(refusal) = switch_refusal(self) {
            return Err(signal("error", vec![Value::string(refusal)]));
        }
        // A failed retirement kept the restored owner's lease in the World.
        // Finish that obligation before publishing another census for this key.
        self.finish_retained_parked_roots(key)?;
        // Capture exact sources before any binding exchange. Operand roots
        // are copied below from trace_roots' jit_window on this owner only.
        #[cfg(feature = "jit")]
        let parked_jit = {
            let mut scratch =
                std::mem::take(&mut self.green_seat().expect("seated above").parked_jit_scratch);
            let prepared =
                crate::emacs_core::jit::green_owner::ParkedJit::prepare_reusing(self, &mut scratch);
            self.green_seat().expect("seated above").parked_jit_scratch = scratch;
            prepared.map_err(|error| signal("error", vec![Value::string(error.to_string())]))?
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
        let publication = match finalized.publish() {
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
        let baton = seat
            .baton
            .take()
            .expect("the running thread holds the Baton");
        let receiver = seat.receiver.take().expect("a seat keeps its channel");
        // The thread record keeps what another thread may read or change
        // while this one is parked: its current buffer and its errors.
        let current = self.buffers.current_buffer_id();
        self.threads.set_thread_current_buffer(key.id(), current);
        self.restore_deferred_error(&baton, key);
        if let Some(runtime) = baton.green_runtime().as_ref() {
            runtime.outgoing_constant_error.set(
                self.failed_constant_owner_binding(true)
                    .map(|symbol| (key, symbol)),
            );
        }
        stow_owner_state(self, &baton);
        stow_carried_state(&baton);
        self.publish_parked_roots(&baton, key, publication);
        // All World/registry borrows have ended before the unique Baton moves.
        let (outcome, host_resume) = match request {
            ParkRequest::Ordinary(wait) => (control.transfer(key, wait, baton, &receiver), None),
            ParkRequest::Host { call, native } => {
                let (wake, resume) =
                    host_service::dispatch_host(call, native, baton, &control, &receiver);
                (Ok(wake), Some(resume))
            }
        };
        let (wake, deadlocked) = match outcome {
            Ok(wake) => (wake, false),
            Err(Handoff::NoRunnable(baton)) => (Wake::Resume(baton), true),
        };
        // A service turn must not restore main's Lisp continuation, JIT pins,
        // bindings or TLS, or consume its original semantic Wait.
        let baton = receive_owner_turn(key, wake, &control, &receiver);
        // Only the returning continuation refreshes its private dispatch
        // header from canonical cells. A peer's function write updates its own
        // header, so comparing our stale header would miss that redefinition.
        self.obarray.refresh_admission();
        // Invalidate shared leaf slots before dispatch or owner mirror work,
        // and restore old-source pins while the immutable batch still roots them.
        #[cfg(feature = "jit")]
        {
            let mut scratch =
                std::mem::take(&mut self.green_seat().expect("seated above").parked_jit_scratch);
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
            .filter(|buffer| current != Some(*buffer) && self.buffers.get(*buffer).is_some());
        if let Some(buffer) = moved {
            let _ = self.buffers.switch_current_unrecorded(buffer);
        }
        let case_table_changed =
            case_table.map(Value::bits) != self.cached_standard_case_table.map(Value::bits);
        self.cached_standard_case_table = case_table;
        self.resume_owner(parked);
        if case_table_changed {
            crate::emacs_core::casetab::activate_casetab_thread_locals(case_table);
        }
        if moved.is_some() {
            let _ = self.sync_current_buffer_runtime_state();
        }
        // Another thread may have edited strings in place or changed the
        // syntax tables these caches answer from.
        crate::emacs_core::string_pos_cache::reset_string_pos_cache();
        crate::emacs_core::regex::reset_regex_thread_locals();
        // Registered results/errors are decoded, acknowledged and retired on
        // this restored owner BEFORE its ordinary parked publication retires.
        let completion = match host_resume {
            Some(resume) => {
                host_service::finish_owner_reply(self, &baton, resume, |owner, settlement| {
                    consume(ParkCompletion::Host { owner, settlement })
                })
            }
            None => consume(ParkCompletion::Ordinary),
        };
        self.green_seat().expect("restored owner is seated").baton = Some(baton);
        let retirement = self.retire_parked_roots(key);
        if let Some(request) = exit {
            // The process exits: like GNU's `exit`, no cleanup form of this
            // thread runs on the way out.
            self.shutdown_request = Some(request);
            self.command_loop.running = false;
            return Err(Flow::shutdown(request));
        }
        retirement?;
        if deadlocked {
            return Err(signal(
                "error",
                vec![Value::string(
                    "All threads are blocked: this one would wait forever \
                     (GNU Emacs hangs here; neomacs signals)",
                )],
            ));
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
        error: crate::emacs_core::forward::ForwardStoreError,
        value: Value,
    ) {
        if let Some(seat) = self.green_seat() {
            seat.binding_resume_error
                .get_or_insert(BindingResumeError::Forward(error, value));
        }
    }

    /// Raise a failed rebind or an error another thread signalled into KEY.
    pub(super) fn green_raise_pending_signal(&mut self, key: ThreadKey) -> Result<(), Flow> {
        if let Some(error) = self.green_seat().and_then(|seat| seat.binding_resume_error) {
            // Keep a forwarded offending value rooted in the seat while
            // constructing signal data, which may allocate a Lisp string.
            let flow = match error {
                BindingResumeError::Constant(symbol) => signal(
                    crate::emacs_core::error::LispCondition::SettingConstant,
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
    /// With a ready peer this owner parks until input, a scheduler event,
    /// or WAIT_TIME, with no slices. With no ready peer it uses the existing
    /// wait backend, clipped to the earliest peer's absolute deadline.
    /// Outside Green, at zero timeout, or where switching is refused, the
    /// existing wait backend remains responsible for the block.
    pub(crate) fn green_select_switch(&mut self, wait_time: Duration) -> Result<SelectTurn, Flow> {
        if self.green_seat().is_none() {
            return Ok(SelectTurn::Block(wait_time));
        }
        self.green_select_turn(wait_time)
    }

    #[cold]
    #[inline(never)]
    fn green_select_turn(&mut self, wait_time: Duration) -> Result<SelectTurn, Flow> {
        let seat = self.green_seat().expect("seated");
        let key = seat.key;
        let control = Arc::clone(&seat.control);
        if wait_time.is_zero() || switch_refusal(self).is_some() {
            return Ok(SelectTurn::Block(wait_time));
        }
        let now = Instant::now();
        match control.peers(key, now) {
            Peers::Ready => {
                #[cfg(test)]
                control
                    .prepare_select_park_for_test(key, &self.processes)
                    .map_err(|error| {
                        signal(LispCondition::Error, vec![Value::string(error.to_string())])
                    })?;
                #[cfg(test)]
                SELECT_PARKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.green_wait(Wait::Select {
                    wait_end: now + wait_time,
                })?;
                Ok(SelectTurn::Switched)
            }
            Peers::Sleeping(deadline) => Ok(SelectTurn::Block(
                wait_time.min(deadline.saturating_duration_since(now)),
            )),
            Peers::Blocked => Ok(SelectTurn::Block(wait_time)),
        }
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
        // A newborn's signal was deferred until it installed handlers.
        // Its first yield delivers it even when every peer is blocked.
        let deferred =
            self.with_green_runtime(|runtime| runtime.deferred_errors.borrow_mut().remove(&key));
        if let Some((symbol, data)) = deferred {
            self.threads.restore_thread_error(key.id(), symbol, data);
        }
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
        control.close();
        while control.others_could_run(ThreadKey::Main) {
            // Each hand-off ends a thread; main's own exit flow, if any, has
            // nowhere left to go.
            let _ = self.green_park(Wait::Runnable);
        }
        for (_, os_thread) in control.take_join_handles() {
            let _ = os_thread.join();
        }
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

    /// KEY's error held since its start is pending again
    /// (`GreenRuntime::deferred_errors`).
    fn restore_deferred_error(&mut self, baton: &Baton, key: ThreadKey) {
        let deferred = baton
            .green_runtime()
            .as_ref()
            .and_then(|runtime| runtime.deferred_errors.borrow_mut().remove(&key));
        if let Some((symbol, data)) = deferred {
            self.threads.restore_thread_error(key.id(), symbol, data);
        }
    }

    /// Keep ROOTS traced while KEY is parked.
    fn publish_parked_roots(
        &self,
        baton: &Baton,
        key: ThreadKey,
        publication: crate::tagged::transport::RootBatchLease,
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
    fn retire_parked_roots(&mut self, key: ThreadKey) -> Result<(), Flow> {
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

    fn finish_retained_parked_roots(&mut self, key: ThreadKey) -> Result<(), Flow> {
        if !self.with_green_runtime(|runtime| runtime.parked.borrow().contains_key(&key)) {
            return Ok(());
        }
        self.prepare_parked_root_retirement()?;
        let retained = self.with_green_runtime(|runtime| runtime.parked.borrow_mut().remove(&key));
        match retained {
            Some(publication) => self.finish_restored_publication(key, publication),
            None => Ok(()),
        }
    }

    fn finish_restored_publication(
        &mut self,
        key: ThreadKey,
        publication: crate::tagged::transport::RootBatchLease,
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
    pub(super) fn prepare_parked_root_retirement(&mut self) -> Result<(), Flow> {
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
        let heap: *mut crate::tagged::gc::TaggedHeap = self.tagged_heap.heap_root().as_ptr();
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
    fn return_parked_root_buffer(&mut self, mut roots: Vec<Value>) {
        roots.clear();
        let seat = self.green_seat().expect("a publishing owner is seated");
        debug_assert!(seat.parked_roots_buffer.is_empty());
        seat.parked_roots_buffer = roots;
    }

    /// GNU `make-thread` (ARGS: FUNCTION, NAME, BUFFER-DISPOSITION) in a
    /// Green World: a Lisp thread on its own OS thread, starting in this
    /// thread's current buffer and running in its turn. This thread keeps
    /// running (GNU does not yield here). Starts the Green runtime on first
    /// use. Returns the thread object.
    #[cold]
    #[inline(never)]
    pub(crate) fn green_spawn(&mut self, args: &[Value]) -> Result<Value, Flow> {
        if self.green_seat().is_none() {
            self.start_green_threads()
                .map_err(|error| signal("error", vec![Value::string(error.to_string())]))?;
        }
        let current = self.buffers.current_buffer_id();
        let (id, function) =
            crate::emacs_core::threads::prepare_make_thread(&mut self.threads, args)?;
        self.threads.set_thread_current_buffer(id, current);
        let handle = self.threads.thread_handle(id).unwrap_or(Value::NIL);
        let key = ThreadKey::from_id(id);
        let control = Arc::clone(&self.green_seat().expect("seated").control);
        let receiver = control.register_worker(key);
        self.with_green_runtime(|runtime| {
            runtime.pending_starts.borrow_mut().insert(key, function);
        });
        let worker_control = Arc::clone(&control);
        let os_thread = std::thread::Builder::new()
            .name(format!("neovm-lisp-{id}"))
            .spawn(move || run_worker(key, receiver, worker_control))
            .expect("spawning a Lisp thread's OS thread");
        control.note_join_handle(key, os_thread);
        Ok(handle)
    }

    /// Start the Green runtime and seat this owner as its main thread.
    #[cold]
    #[inline(never)]
    pub(crate) fn start_green_threads(&mut self) -> Result<(), super::GreenStartError> {
        let baton = self.start_green_runtime()?;
        // The main carrier runs Lisp on this OS thread; reserve its dense
        // SymId/NameId caches as worker owners do on creation.
        crate::emacs_core::intern::prepare_thread_symbol_caches();
        let control = self.with_green_runtime(|runtime| Arc::clone(&runtime.control));
        control.install_select(self.processes.green_select_handle());
        let receiver = control.register_main();
        self.take_green_seat(ThreadKey::Main, control, receiver, baton);
        Ok(())
    }

    /// Run OP on this World's Green runtime (which has started).
    pub(super) fn with_green_runtime<R>(&self, op: impl FnOnce(&super::GreenRuntime) -> R) -> R {
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
    receiver: &mpsc::Receiver<Wake>,
) -> Baton {
    let _abort_on_panic = AbortOnPanic;
    loop {
        wake = control.intercept_acquire(key, wake, receiver);
        match wake {
            Wake::Resume(baton) => {
                baton.enter();
                return baton;
            }
            Wake::ServiceTurn {
                key: request,
                baton,
            } => {
                if key != ThreadKey::Main {
                    tracing::error!(?key, "native service addressed a non-main owner");
                    std::process::abort();
                }
                match host_service::serve_main_wake(request, baton, control, receiver) {
                    ServiceReturn::ResumeMain(baton) => {
                        baton.enter();
                        return baton;
                    }
                    ServiceReturn::Sent => {
                        wake = match receiver.recv() {
                            Ok(wake) => wake,
                            Err(_) => std::process::abort(),
                        };
                    }
                }
            }
        }
    }
}

/// A Lisp thread's OS thread: wait for the Baton, build the thread's
/// owner, run its function, record the outcome and hand the Baton on.
fn run_worker(key: ThreadKey, receiver: mpsc::Receiver<Wake>, control: Arc<Control>) {
    // A panic here would drop the Baton with this thread's owner, and every
    // parked thread would wait for it forever: end the process instead.
    let _abort_on_panic = AbortOnPanic;
    let Ok(wake) = receiver.recv() else {
        // The runtime went away before this thread ever ran.
        return;
    };
    let baton = receive_owner_turn(key, wake, &control, &receiver);
    baton.enter();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::green_owner::begin_owner_turn();
    unstow_carried_state(&baton);
    let mut owner = Context::new_green_owner(&baton);
    unstow_owner_state(&mut owner, &baton);
    if control.lifecycle() == Lifecycle::Closing {
        // Main ended before this thread ever ran.
        owner.threads.mark_thread_exited(key.id());
        if let Some(runtime) = baton.green_runtime().as_ref() {
            runtime.pending_starts.borrow_mut().remove(&key);
        }
        stow_owner_state(&mut owner, &baton);
        owner.retire_green_owner();
        stow_carried_state(&baton);
        control.finish(key, baton);
        return;
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
    // GNU raises an error signalled into a thread that has not run yet at
    // its first switch, once it has handlers.
    let deferred = owner.threads.take_thread_error(key.id());
    let function = baton.green_runtime().as_ref().and_then(|runtime| {
        if let Some(error) = deferred {
            runtime.deferred_errors.borrow_mut().insert(key, error);
        }
        runtime.pending_starts.borrow_mut().remove(&key)
    });
    owner.take_green_seat(key, Arc::clone(&control), receiver, baton);
    #[cfg(test)]
    if PANIC_NEXT_WORKER.swap(false, std::sync::atomic::Ordering::Relaxed) {
        panic!("test hook: a Green worker panics");
    }
    // GNU's analogous failure before a newborn thread has handlers crashes
    // the process. Keep the internal error as this thread's last-error;
    // established threads take the exact catchable path in green_park.
    let outcome = match (outgoing_error, function) {
        (Some(symbol), _) => Err(signal(
            crate::emacs_core::error::LispCondition::SettingConstant,
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
    owner.threads.mark_thread_exited(key.id());
    let seat = owner.green_seat().expect("seated above");
    let baton = seat
        .baton
        .take()
        .expect("a finishing thread holds the Baton");
    owner.restore_deferred_error(&baton, key);
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
    stow_owner_state(&mut owner, &baton);
    owner.retire_green_owner();
    stow_carried_state(&baton);
    control.finish(key, baton);
}

/// The World's thread-local state leaves this OS thread with the Baton
/// (`carried`).
fn stow_carried_state(baton: &Baton) {
    if let Some(runtime) = baton.green_runtime().as_ref() {
        runtime.carried.borrow_mut().stow();
    }
}

/// The World's thread-local state arrives on this OS thread with the Baton.
fn unstow_carried_state(baton: &Baton) {
    if let Some(runtime) = baton.green_runtime().as_ref() {
        runtime.carried.borrow_mut().unstow();
    }
}

/// The warm GNU-global state leaves OWNER with the Baton (`owner_carried`).
fn stow_owner_state(owner: &mut Context, baton: &Baton) {
    if let Some(runtime) = baton.green_runtime().as_ref() {
        let mut slot = runtime.owner_carried.borrow_mut();
        assert!(slot.is_none(), "the World's warm state is in two places");
        *slot = Some(super::owner_carried::OwnerCarried::take(owner));
    }
}

/// The warm GNU-global state arrives in OWNER with the Baton.
fn unstow_owner_state(owner: &mut Context, baton: &Baton) {
    if let Some(runtime) = baton.green_runtime().as_ref()
        && let Some(state) = runtime.owner_carried.borrow_mut().take()
    {
        state.put(owner);
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
