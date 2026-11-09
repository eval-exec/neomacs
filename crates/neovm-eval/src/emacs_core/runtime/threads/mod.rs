//! Emacs-compatible threading primitives for the Elisp VM.
//!
//! Default Elisp threads cooperate through the World's Baton. The Sim harness
//! runs stored functions on its caller; the Green runtime schedules carriers.
//! Both use the same heap-owned logical objects and live registrations.
//!
//! Provided primitives:
//! - Threads: `make-thread`, `thread-join`, `thread-yield`, `thread-name`,
//!   `thread-live-p`, `threadp`, `thread-signal`, `current-thread`,
//!   `all-threads`, `thread-last-error`
//! - Mutexes: `make-mutex`, `mutexp`, `mutex-name`, `mutex-lock`, `mutex-unlock`
//! - Condition variables: `make-condition-variable`, `condition-variable-p`,
//!   `condition-wait`, `condition-notify`
//! - Special form: `with-mutex`

use neovm_value::error::LispCondition;
#[doc(hidden)]
pub use neovm_value::error::{expect_args, expect_args_range, expect_min_args};
use std::{collections::HashMap, time::Duration};

#[doc(hidden)]
pub use neovm_value::error::{
    EvalResult, Flow, FlowKind, FlowResultExt, make_signal_binding_value, signal,
    signal_from_binding_value, signal_with_data_id,
};
use neovm_value::gc_trace::GcTrace;
use neovm_value::heap_types::LispString;
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::{Value, ValueKind, eq_value, list_to_vec};

const CONDITION_CASE_CONTINUATION_MARKER: &str = "neomacs--thread-condition-case-continuation";
const SLEEP_BLOCKER_MARKER: &str = "sleep-for";

pub(crate) fn make_thread_condition_case_continuation(
    var: Value,
    remaining_forms: Value,
    handlers: Value,
    lexenv: Value,
) -> Value {
    Value::list(vec![
        Value::symbol(CONDITION_CASE_CONTINUATION_MARKER),
        var,
        remaining_forms,
        handlers,
        lexenv,
    ])
}

pub(crate) fn thread_condition_case_continuation_parts(
    value: Value,
) -> Option<(Value, Value, Value, Value)> {
    let items = list_to_vec(&value)?;
    if items.len() != 5 || !items[0].is_bare_symbol_named(CONDITION_CASE_CONTINUATION_MARKER) {
        return None;
    }
    Some((items[1], items[2], items[3], items[4]))
}

#[doc(hidden)]
pub fn make_sleep_blocker(seconds: f64) -> Value {
    Value::list(vec![
        Value::symbol(SLEEP_BLOCKER_MARKER),
        Value::make_float(seconds),
    ])
}

fn sleep_duration_from_blocker(value: Value) -> Option<Duration> {
    let items = list_to_vec(&value)?;
    if items.len() != 2 || !items[0].is_bare_symbol_named(SLEEP_BLOCKER_MARKER) {
        return None;
    }
    match neovm_value::system_vocab::time::WaitTimeout::from(items[1].xfloat()) {
        neovm_value::system_vocab::time::WaitTimeout::Poll => None,
        neovm_value::system_vocab::time::WaitTimeout::For(duration) => Some(duration),
        neovm_value::system_vocab::time::WaitTimeout::Forever => Some(Duration::MAX),
    }
}

// ---------------------------------------------------------------------------
// Thread state
// ---------------------------------------------------------------------------

/// A thread's stored NAME object (GNU `struct thread_state.name`).
///
/// GNU validates NAME at `make-thread` (`CHECK_STRING` unless nil,
/// `src/thread.c:902-903`), stores the object itself, and `thread-name`
/// returns that same object (`src/thread.c:941-953`), so
/// `(eq name (thread-name th))` is t.  The newtype keeps the string-or-nil
/// invariant at the one construction site and makes the identity contract
/// explicit: readers hand back the stored object, never a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadName(neovm_value::heap_types::threading::ThreadName);

impl ThreadName {
    /// No NAME was supplied (GNU stores nil).
    pub const UNNAMED: Self = Self(neovm_value::heap_types::threading::ThreadName::UNNAMED);

    /// GNU `make-thread`'s `if (!NILP (name)) CHECK_STRING (name)`.
    pub fn from_lisp(name: Value) -> Result<Self, Flow> {
        neovm_value::heap_types::threading::ThreadName::from_value(name)
            .map(Self)
            .map_err(|_| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), name],
                )
            })
    }

    pub fn is_unnamed(self) -> bool {
        self.0.is_unnamed()
    }

    pub fn stored(self) -> Value {
        self.0.stored()
    }
}

/// Temporary transport roots awaiting admission into a target's ordinary
/// traced children. A successful admission permits these leases to expire;
/// a refusal leaves the caller's event intact.
#[derive(Clone, Debug)]
pub struct InFlightThreadEvent {
    pub sender: SharedRoot,
    pub error_symbol: SharedRoot,
    pub data: SharedRoot,
}

impl InFlightThreadEvent {
    /// Root `(SENDER ERROR-SYMBOL DATA)` for transport to the main mutator.
    ///
    /// This is on `thread-signal`'s Lisp-reachable path, so a heap-identity
    /// failure is a Lisp error, never a panic: `share_values_array` returns
    /// `[SharedRoot; 3]` by construction, and its one fallible step (the
    /// installed-mutator identity check) becomes `(error "…")`.  During Lisp
    /// execution the check cannot fail — the signalling evaluator is, by
    /// construction, the mutator whose heap its thread has installed — but
    /// the runtime still models the failure instead of asserting it.
    ///
    /// # Safety
    /// The caller must be the installed mutator of this evaluator's heap and
    /// keep all three values reachable until this returns; see
    /// [`SharedRoot::new`](neovm_value::tagged::transport::SharedRoot::new).
    ///
    /// # Errors
    /// A GNU-shaped `(error "…")` when the evaluator's heap is not the one
    /// installed on this thread (or when none is installed).
    pub unsafe fn share(
        ctx: &super::eval::Context,
        sender: Value,
        error_symbol: Value,
        data: Value,
    ) -> Result<Self, Flow> {
        // SAFETY: the caller supplies the live same-heap provenance required
        // by `share_values_array`; this Context supplies its heap and checks
        // that it is the one installed on this thread.
        let roots = unsafe { ctx.share_values_array([sender, error_symbol, data]) }
            .map_err(thread_event_transport_error)?;
        let [sender, error_symbol, data] = roots;
        Ok(Self {
            sender,
            error_symbol,
            data,
        })
    }
}

/// The runtime failure mode of [`InFlightThreadEvent::share`], as a Lisp error.
///
/// GNU's `Fthread_signal` has no counterpart (its kbd buffer is process-wide
/// and untyped), so this names the transport invariant instead of inventing a
/// GNU condition: the event must be rooted on the heap the signalling evaluator
/// owns.
fn thread_event_transport_error(error: neovm_value::tagged::transport::SharedRootError) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "thread-signal cannot root its event on this evaluator's heap: {error}"
        ))],
    )
}

mod completion;
mod layout;
mod mailbox;
mod manager;
mod release_point;

use completion::SimCompletion;
pub(crate) use completion::{CompletingThread, CompletionExit, RetiredCompletion};
#[doc(hidden)]
pub use release_point::SimReleasePoint;

pub(crate) use neovm_value::heap_types::threading::{
    BlockedWait, CondVarId, CondVarPayload, HandlerList, JoinObservation, MutexId, MutexOwnership,
    MutexPayload, MutexReference, RecursionDepth, RunState, StoredThreadEvent, ThreadChildren,
    ThreadId, ThreadIds, ThreadNativeState, ThreadOutcome, ThreadPayload, ThreadPhase,
};

/// A validated object identity. This word is not a GC root by itself: Lisp
/// arguments or a published execution record must retain it across a safepoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MutexHandle {
    value: Value,
    id: MutexId,
}

impl MutexHandle {
    #[doc(hidden)]
    pub fn value(self) -> Value {
        self.value
    }
    #[doc(hidden)]
    pub fn native_key(self) -> u64 {
        self.id.native_key()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CondVarHandle {
    value: Value,
    id: CondVarId,
    mutex: MutexHandle,
}

impl CondVarHandle {
    #[doc(hidden)]
    pub fn value(self) -> Value {
        self.value
    }
    #[doc(hidden)]
    pub fn native_key(self) -> u64 {
        self.id.native_key()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MutexAcquisition {
    Acquired,
    Blocked,
    DepthExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MutexRelease {
    StillLocked,
    Released,
    NotOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationScope {
    One,
    All,
}

/// World-owned live registrations. Concrete heap objects own logical state;
/// these reservations preserve the established WorldSlot and hot field offsets.
#[repr(transparent)]
struct CurrentThreadOrdinal(u64);

impl CurrentThreadOrdinal {
    #[doc(hidden)]
    pub const MAIN: Self = Self(0);

    #[inline]
    #[doc(hidden)]
    pub fn native_key(&self) -> u64 {
        self.0
    }
}

#[repr(C)]
pub struct ThreadManager {
    last_error: Option<Value>,
    threads: HashMap<ThreadId, Value>,
    mailbox_reservation: layout::MailboxReservation,
    before_issuer: layout::BeforeIssuerReservation,
    ids: ThreadIds,
    current_thread: CurrentThreadOrdinal,
    tail: layout::TailReservation,
}

static_assertions::const_assert_eq!(size_of::<ThreadManager>(), layout::OLD_SIZE);
static_assertions::const_assert_eq!(align_of::<ThreadManager>(), layout::OLD_ALIGN);
static_assertions::const_assert_eq!(
    std::mem::offset_of!(ThreadManager, last_error),
    layout::OLD_LAST_ERROR
);
static_assertions::const_assert_eq!(
    std::mem::offset_of!(ThreadManager, ids) + ThreadIds::NEXT_THREAD_OFFSET,
    layout::OLD_NEXT
);
static_assertions::const_assert_eq!(
    std::mem::offset_of!(ThreadManager, current_thread),
    layout::OLD_CURRENT
);

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for ThreadManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        let Self {
            last_error,
            threads,
            mailbox_reservation: _,
            before_issuer: _,
            ids: _,
            current_thread: _,
            tail: _,
        } = self;
        roots.extend(threads.values().copied());
        if let Some(error) = last_error {
            roots.push(*error);
        }
        // Stored events are reached through the canonical ThreadObj child schema.
    }
}

/// The concrete pseudovector kind inspected by the native-key test helper.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadingKind {
    Thread,
    Mutex,
    CondVar,
}

#[cfg(test)]
impl ThreadingKind {
    /// The heap tag that makes this object opaque to Lisp.
    #[doc(hidden)]
    pub fn veclike(self) -> neovm_value::tagged::header::VecLikeType {
        match self {
            Self::Thread => neovm_value::tagged::header::VecLikeType::Thread,
            Self::Mutex => neovm_value::tagged::header::VecLikeType::Mutex,
            Self::CondVar => neovm_value::tagged::header::VecLikeType::CondVar,
        }
    }
}

/// The id inside `value`, if it is a handle of exactly `expected`'s kind.
#[cfg(test)]
fn tagged_object_id(value: &Value, expected: ThreadingKind) -> Option<u64> {
    if value.veclike_type() != Some(expected.veclike()) {
        return None;
    }
    value.threading_handle_id()
}

fn missing_live_thread_registration() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string("Live thread registration is missing")],
    )
}

fn split_signal_binding_value(value: Value) -> Option<(Value, Value)> {
    if !value.is_cons() {
        return None;
    };
    let pair_car = value.cons_car();
    let pair_cdr = value.cons_cdr();
    pair_car.as_bare_symbol_id()?;
    Some((pair_car, pair_cdr))
}

/// Extract the scheduler key from a validated owned thread object.
pub(crate) fn expect_thread_id(manager: &ThreadManager, value: &Value) -> Result<u64, Flow> {
    match manager.thread_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("threadp"), *value],
        )),
    }
}

/// Validate a mutex object and retain its typed native identity with its value.
fn expect_mutex_id(manager: &ThreadManager, value: &Value) -> Result<MutexHandle, Flow> {
    match manager.mutex_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), *value],
        )),
    }
}

/// Validate a condition variable and retain its typed identity with its value.
fn expect_cv_id(manager: &ThreadManager, value: &Value) -> Result<CondVarHandle, Flow> {
    match manager.condition_variable_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), *value],
        )),
    }
}

// ===========================================================================
// Thread builtins
// ===========================================================================

/// `(make-thread FUNCTION &optional NAME)` -- create a thread.
///
/// GNU `Fmake_thread` (src/thread.c:885-947) allocates the thread state, links
/// it into `all_threads` (:924-925) and returns while the creating thread
/// still holds the global lock; the worker's `run_thread` (:757+) executes
/// only after it acquires that lock. The simulation keeps the same order: the
/// body runs at the caller's next global-lock release point
/// ([`SimReleasePoint`]), never during construction, so a created worker is
/// live (`thread_live_p`, :76/:1036-1045) and listed by `all-threads`
/// (:1146-1160) before it has run.
#[doc(hidden)]
pub fn builtin_make_thread(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    // A Green World runs threads as owners on their own OS threads; the
    // simulated exchange never runs beside admission.
    if super::eval::green_world(eval) {
        return super::eval::green_make_thread(eval, &args);
    }
    let (thread_id, _function) = prepare_make_thread(&mut eval.threads, &args)?;
    eval.threads
        .set_thread_current_buffer(thread_id, eval.edit.buffers.current_buffer_id());
    eval.threads
        .thread_handle(thread_id)
        .ok_or_else(missing_live_thread_registration)
}

/// Run the body of one created-but-not-yet-run simulated worker.
///
/// A created worker first runs at any GNU global-lock release point
/// (`thread_select` src/thread.c:613-643, `Fthread_yield` :725-740, a
/// contended `lisp_mutex_lock_for_thread` :212-246, `Fcondition_wait`
/// :446-501 or `Fthread_join` :1095-1144). The single scheduling pass lives
/// in [`Context::sim_release_point`]; this helper runs one such worker and is
/// idempotent because the body's completion moves the phase out of
/// `New`/`Active(Running)`.
pub(crate) fn run_pending_sim_thread(
    eval: &mut super::eval::Context,
    thread_id: u64,
) -> EvalResult {
    if !eval.threads.thread_is_pending_sim_run(thread_id) {
        return Ok(Value::NIL);
    }
    // Keep the existing two-word callback ABI: the registered object is still
    // a strong root when this callback begins, so its typed identity can be
    // acquired here before runtime entry rather than captured.
    let completion_id = eval
        .threads
        .get_thread(thread_id)
        .ok_or_else(missing_live_thread_registration)?
        .id();
    let function = eval
        .threads
        .thread_function(thread_id)
        .ok_or_else(missing_live_thread_registration)?;
    eval.with_sim_creation_local_bindings(|eval| {
        let runtime_state = match enter_thread_runtime(eval, thread_id) {
            Ok(state) => state,
            Err(flow) => {
                eval.threads.discard_unstarted_thread(thread_id);
                return Err(flow);
            }
        };
        // GNU installs `handlerlist_sentinel` after the first acquire
        // (src/thread.c:805 then :810-815) and does not raise there.
        eval.threads.mark_handlerlist_installed(thread_id);
        let result = eval.apply(function, vec![]);
        let result = preserve_blocked_sim_worker(eval, thread_id, result);
        let exit_result = exit_thread_runtime(eval, thread_id, runtime_state);
        let thread_result = finish_make_thread_result(&mut eval.threads, thread_id, result);
        finish_sim_thread_registration(&mut eval.threads, completion_id)?;
        exit_result?;
        thread_result
    })
}

pub(crate) fn prepare_make_thread(
    threads: &mut ThreadManager,
    args: &[Value],
) -> Result<(u64, Value), Flow> {
    expect_args_range("make-thread", args, 1, 3)?;

    let function = args[0];
    // GNU stores the NAME object itself and `thread-name` returns it, so the
    // constructor validates (string or nil) without copying.
    let name = ThreadName::from_lisp(args.get(1).copied().unwrap_or(Value::NIL))?;
    let buffer_disposition = args.get(2).copied().unwrap_or(Value::NIL);

    let thread_id = threads.try_create_thread(function, name)?.native_key();
    threads.set_thread_buffer_disposition(thread_id, buffer_disposition);
    threads.start_thread(thread_id);
    Ok((thread_id, function))
}

#[derive(Debug)]
pub(crate) struct ThreadRuntimeState {
    previous_thread_id: u64,
    previous_buffer_id: Option<neovm_edit::buffer::BufferId>,
    previous_dynamic_bindings: super::eval::ThreadDynamicBindingToken,
}

pub(crate) fn enter_thread_runtime(
    eval: &mut super::eval::Context,
    thread_id: u64,
) -> Result<ThreadRuntimeState, Flow> {
    let previous_dynamic_bindings = eval.suspend_dynamic_bindings_for_thread_switch()?;
    let previous_thread_id = eval.threads.enter_thread(thread_id);
    let previous_buffer_id = eval
        .threads
        .thread_current_buffer(previous_thread_id)
        .or_else(|| eval.edit.buffers.current_buffer_id());
    if let Some(thread_buffer_id) = eval.threads.thread_current_buffer(thread_id) {
        if eval.edit.buffers.current_buffer_id() != Some(thread_buffer_id) {
            if let Err(err) = eval.switch_current_buffer(thread_buffer_id) {
                eval.threads.restore_thread(previous_thread_id);
                return match eval
                    .resume_dynamic_bindings_for_thread_switch(previous_dynamic_bindings)
                {
                    Ok(()) => Err(err),
                    Err(resume_flow) => Err(resume_flow),
                };
            }
        } else {
            eval.sync_current_thread_buffer_state();
        }
    } else {
        eval.sync_current_thread_buffer_state();
    }
    // The body is about to run on this OS stack: a release point from here
    // must not treat this worker as a created-but-unrun one.
    eval.threads.set_thread_executing(thread_id, true);
    Ok(ThreadRuntimeState {
        previous_thread_id,
        previous_buffer_id,
        previous_dynamic_bindings,
    })
}

/// Keep a blocking worker's specpdl for the resume that continues it.
///
/// An `Ok` or a real signal has already unwound. `ThreadBlocked` has not:
/// [`Context::unbind_to_with_result_slow`] leaves those frames in place.
fn preserve_blocked_sim_worker(
    eval: &mut super::eval::Context,
    thread_id: u64,
    result: EvalResult,
) -> EvalResult {
    if !matches!(result.as_ref(), Err(flow) if flow.is_thread_blocked()) {
        return result;
    }
    match eval.park_sim_worker_bindings(thread_id) {
        Ok(()) => result,
        Err(flow) => eval.unbind_to_with_result(0, Err(flow)),
    }
}

/// After a resumed continuation, run parked `unwind-protect` and `let`
/// cleanups. A fresh `ThreadBlocked` parks those frames again instead.
fn settle_resumed_sim_worker(
    ctx: &mut crate::emacs_core::eval::Context,
    thread_id: u64,
    result: EvalResult,
) -> EvalResult {
    if matches!(result.as_ref(), Err(flow) if flow.is_thread_blocked()) {
        return match ctx.park_sim_worker_bindings(thread_id) {
            Ok(()) => result,
            Err(flow) => ctx.unbind_to_with_result(0, Err(flow)),
        };
    }
    ctx.unbind_to_with_result(0, result)
}

pub(crate) fn exit_thread_runtime(
    eval: &mut super::eval::Context,
    thread_id: u64,
    runtime_state: ThreadRuntimeState,
) -> Result<(), Flow> {
    eval.threads
        .set_thread_current_buffer(thread_id, eval.edit.buffers.current_buffer_id());
    eval.threads.set_thread_executing(thread_id, false);
    eval.threads
        .restore_thread(runtime_state.previous_thread_id);
    let resume_result =
        eval.resume_dynamic_bindings_for_thread_switch(runtime_state.previous_dynamic_bindings);
    if let Some(previous_buffer_id) = runtime_state.previous_buffer_id {
        eval.restore_current_buffer_if_live(previous_buffer_id);
    }
    eval.sync_current_thread_buffer_state();
    resume_result
}

pub(crate) fn finish_make_thread_result(
    threads: &mut ThreadManager,
    thread_id: u64,
    result: EvalResult,
) -> EvalResult {
    // Capture the identity while its live registration still roots it. The
    // completion transition can unlink that registration; neither completion
    // nor returning this value reaches a collector safe point.
    let thread = threads
        .thread_handle(thread_id)
        .ok_or_else(missing_live_thread_registration)?;
    let result = result.kinded();
    match result {
        Ok(val) => {
            threads.finish_thread(thread_id, val);
        }
        Err(FlowKind::Signal(ref sig)) => {
            let error_val = make_signal_binding_value(sig);
            // Internal error: recorded for thread-last-error and marks the
            // thread dead, but NOT re-raised by a later thread-join (GNU
            // record_thread_error, thread.c:753-758).
            threads.record_thread_internal_error(thread_id, error_val);
            // GNU publishes thread-last-error when the thread dies, not when
            // another thread joins it later.
            threads.record_last_error(error_val);
        }
        Err(FlowKind::Throw(ref thrown)) => {
            let error_val = Value::list(vec![Value::symbol("no-catch"), thrown.tag, thrown.value]);
            threads.record_thread_internal_error(thread_id, error_val);
            threads.record_last_error(error_val);
        }
        Err(FlowKind::ThreadBlocked(blocked)) => {
            threads.block_thread(thread_id, blocked.blocker, blocked.remaining_forms);
            // A select-class wait publishes its deadline here, after the
            // worker has parked. `thread-signal` does not wake it.
            if let Some(duration) = sleep_duration_from_blocker(blocked.blocker) {
                let deadline = std::time::Instant::now() + duration;
                threads.set_blocked_wait(thread_id, Some(BlockedWait::Select { deadline }));
            }
        }
        // kill-emacs is process-wide in GNU (Fkill_emacs exits, whichever
        // thread runs it), so it is not recorded as this thread's error — it
        // unwinds to the caller and ends the process.
        Err(flow @ FlowKind::Shutdown(_)) => return Err(Flow::from_kind(flow)),
    }

    Ok(thread)
}

/// Sim has no carrier to retire. Once the previous runtime is restored and
/// the result is published, no native worker access follows this boundary.
fn finish_sim_thread_registration(
    threads: &mut ThreadManager,
    thread_id: ThreadId,
) -> Result<(), Flow> {
    let completion = threads
        .prepare_sim_completion(thread_id)
        .map_err(|error| match error {
            completion::CompletionError::MissingRegistration
            | completion::CompletionError::MainThread
            | completion::CompletionError::AlreadyPrepared => missing_live_thread_registration(),
        })?;
    match completion {
        SimCompletion::Blocked => {}
        SimCompletion::Completed(completion) => {
            let _retired = threads.retire_thread_completion(completion);
        }
    }
    Ok(())
}

/// `(thread-join THREAD)` -- wait for thread completion.
///
/// Returns the thread's result, resuming a simulated blocked thread when needed,
/// or re-signals a `thread-signal` error snapshot the GNU way.
#[doc(hidden)]
pub fn builtin_thread_join(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if ctx.green_threads_running() {
        return ctx.green_thread_join(&args);
    }
    expect_args("thread-join", &args, 1)?;
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    if id == ctx.threads.current_thread_id() {
        return Err(signal(
            "error",
            vec![Value::string("Cannot join current thread")],
        ));
    }
    // GNU snapshots the target thread's injected error before waiting, then
    // re-signals that snapshot even if the target catches and clears it while
    // join is waiting.
    let pending_join_signal = ctx.threads.pending_thread_signal_for_value(args[0]);
    // GNU `Fthread_join` (src/thread.c:1095-1144) waits on the target's exit
    // condition and thereby releases the global lock; created workers run and
    // suspended waiters resume at that release point. A signal stored before
    // the target's first acquire stays pending: `run_thread` acquires at
    // :805 before `handlerlist_sentinel` (:810-815), and
    // `post_acquire_global_lock` (:141-154) raises only once that handler
    // exists. The snapshot above is what join re-raises.
    ctx.sim_release_point(SimReleasePoint::Join)?;
    if ctx.threads.thread_ready_to_resume(id) {
        resume_blocked_thread(ctx, id)?;
    } else if let Some(duration) = ctx.threads.blocked_sleep_duration(id) {
        std::thread::sleep(duration);
        ctx.threads.wake_thread(id);
        resume_blocked_thread(ctx, id)?;
    }
    // GNU `Fthread_join` (thread.c:1118-1144) re-raises only an error injected
    // into this thread by `thread-signal` (recorded in `error_symbol`); an
    // error signalled *inside* the thread function is surfaced via
    // `thread-last-error`, and join just returns the thread's result.
    let _ = ctx.threads.join_thread_for_value(args[0]);
    if let Some(flow) = pending_join_signal {
        return Err(flow);
    }
    Ok(ctx.threads.thread_result_for_value(args[0]))
}

pub(crate) fn resume_blocked_thread(
    ctx: &mut crate::emacs_core::eval::Context,
    thread_id: u64,
) -> EvalResult {
    let Some(remaining_forms) = ctx.threads.blocked_remaining_forms_if_ready(thread_id) else {
        return Ok(Value::NIL);
    };
    let completion_id = ctx
        .threads
        .get_thread(thread_id)
        .ok_or_else(missing_live_thread_registration)?
        .id();
    let wait = ctx.threads.blocked_wait(thread_id);
    let signaled = ctx.threads.has_pending_thread_signal(thread_id);
    let mut runtime_state = enter_thread_runtime(ctx, thread_id)?;

    // GNU re-runs part of the blocked operation before the saved
    // continuation: `lisp_mutex_lock_for_thread` re-checks the mutex
    // (src/thread.c:212-246), and `condition_wait_callback` reacquires the
    // mutex with its saved count (:446-476). An injected signal ends a
    // plain mutex-lock wait without taking the mutex; a condition wait still
    // reacquires before `post_acquire_global_lock` raises (:105-155).
    let blocker = ctx.threads.thread_blocker(thread_id).unwrap_or(Value::NIL);
    let retry: Result<bool, Flow> = match wait {
        Some(BlockedWait::MutexLock) if !signaled => {
            match ctx.threads.mutex_id_from_handle(&blocker) {
                Some(mutex) => match ctx.threads.mutex_lock(mutex) {
                    MutexAcquisition::Acquired => Ok(true),
                    MutexAcquisition::Blocked => Ok(false),
                    MutexAcquisition::DepthExhausted => Err(signal(
                        LispCondition::Error,
                        vec![Value::string("Mutex recursion depth limit exceeded")],
                    )),
                },
                // The wait object is rooted by `event_object`; losing its typed
                // identity means the mutex object was replaced or corrupted.
                None => Ok(true),
            }
        }
        Some(BlockedWait::ConditionWait(depth)) => {
            let mutex = match ctx.threads.condition_variable_id_from_handle(&blocker) {
                Some(condition) => Some(ctx.threads.condition_variable_mutex(condition)),
                // A notified waiter that lost the mutex race re-waits on the
                // mutex itself (`lisp_mutex_lock_for_thread` :234-241).
                None => ctx.threads.mutex_id_from_handle(&blocker),
            };
            match mutex {
                Some(mutex) => match ctx.threads.mutex_relock_after_wait(mutex, depth) {
                    MutexAcquisition::Acquired => Ok(true),
                    MutexAcquisition::Blocked => Ok(false),
                    MutexAcquisition::DepthExhausted => Err(signal(
                        LispCondition::Error,
                        vec![Value::string("Mutex recursion depth limit exceeded")],
                    )),
                },
                None => Ok(true),
            }
        }
        // A pending signal skips the mutex retry (`lisp_mutex_lock_for_thread`
        // returns before taking the lock when `error_symbol` is set,
        // src/thread.c:232-238). A select deadline has already elapsed: the
        // joiner slept until then, so raise or continue and do not re-enter
        // the select (src/thread.c:974-982).
        Some(BlockedWait::Select { .. } | BlockedWait::MutexLock) | None => Ok(true),
    };
    let retry_error = match retry {
        Ok(true) => None,
        Ok(false) => {
            // Another woken waiter took the mutex first. Keep the typed wait
            // and re-wait on the mutex condition (GNU `sys_cond_wait`).
            if let Some(condition) = ctx.threads.condition_variable_id_from_handle(&blocker) {
                let mutex_value = ctx
                    .threads
                    .mutex_handle(ctx.threads.condition_variable_mutex(condition));
                ctx.threads.set_thread_blocker(thread_id, mutex_value);
            }
            ctx.threads.clear_thread_woken(thread_id);
            let exit_result = exit_thread_runtime(ctx, thread_id, runtime_state);
            exit_result?;
            return Ok(Value::NIL);
        }
        Err(flow) => Some(flow),
    };

    // The wait ended. GNU clears `event_object` before returning to Lisp
    // (`do_unwind_mutex_lock` :343-347, `condition_wait_callback` :465).
    ctx.threads.clear_thread_blocker(thread_id);
    ctx.threads.set_blocked_wait(thread_id, None);
    ctx.threads.clear_thread_woken(thread_id);

    // The wait parked `unwind-protect` and dynamic `let` frames. Put them
    // back before the continuation, so a cleanup sees the inner binding
    // and a handler flag set by the continuation.
    let restore_error = ctx
        .restore_sim_worker_bindings(thread_id, &mut runtime_state.previous_dynamic_bindings)
        .err();

    let handler = thread_condition_case_continuation_parts(remaining_forms);
    let result = if let Some(flow) = restore_error.or(retry_error) {
        let _ = ctx.threads.take_blocked_remaining_forms(thread_id);
        Err(flow)
    } else if let Some((var, body, handlers, lexenv)) = handler {
        // GNU raises inside the handlers that were active at the wait
        // (`post_acquire_global_lock`, thread.c:141-154). This continuation
        // takes the pending pair itself, which is the same path as `signal`.
        match ctx.threads.take_blocked_remaining_forms(thread_id) {
            Some(taken_forms) => {
                debug_assert!(eq_value(&taken_forms, &remaining_forms));
                ctx.resume_thread_condition_case_continuation(var, body, handlers, lexenv)
            }
            None => Err(missing_live_thread_registration()),
        }
    } else if let Some(flow) = ctx.threads.take_pending_thread_signal(thread_id) {
        // No handler was saved around the wait, so the forms after the
        // blocking call do not run (src/thread.c:238-241). Dispatch so
        // `signal-hook-function` and `debug-on-signal` run in the target,
        // the same path as `signal` (src/eval.c:1966-1976).
        let _ = ctx.threads.take_blocked_remaining_forms(thread_id);
        ctx.dispatch_signal_result_if_needed(Err(flow))
    } else {
        match ctx.threads.take_blocked_remaining_forms(thread_id) {
            Some(taken_forms) => {
                debug_assert!(eq_value(&taken_forms, &remaining_forms));
                if remaining_forms
                    .closure_slot(neovm_value::tagged::header::CLOSURE_ARGLIST)
                    .is_some()
                {
                    ctx.apply(remaining_forms, vec![])
                } else {
                    ctx.eval_lambda_body_value(remaining_forms)
                }
            }
            None => Err(missing_live_thread_registration()),
        }
    };
    let result = settle_resumed_sim_worker(ctx, thread_id, result);
    let exit_result = exit_thread_runtime(ctx, thread_id, runtime_state);
    let thread_result = finish_make_thread_result(&mut ctx.threads, thread_id, result);
    finish_sim_thread_registration(&mut ctx.threads, completion_id)?;
    exit_result?;
    thread_result?;
    Ok(Value::NIL)
}

/// `(thread-yield)` -- yield the current thread.
#[doc(hidden)]
pub fn builtin_thread_yield(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if ctx.green_threads_running() {
        return ctx.green_thread_yield(&args);
    }
    expect_args("thread-yield", &args, 0)?;
    // GNU `Fthread_yield` (src/thread.c:725-740, `yield_callback`) releases
    // the global lock, yields the CPU, and reacquires. The typed release
    // point names that operation; a worker continues in place afterwards,
    // exactly as yield_callback reacquires for its caller.
    ctx.sim_release_point(SimReleasePoint::Yield)?;
    // Yield's reacquire is this return. `post_acquire_global_lock`
    // (src/thread.c:141-154) raises a pending signal here, after the
    // top-level handler exists and before the next form.
    ctx.deliver_pending_sim_signal()?;
    Ok(Value::NIL)
}

/// `(thread-name THREAD)` -- return the thread's name or nil.
#[doc(hidden)]
pub fn builtin_thread_name(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-name", &args, 1)?;
    expect_thread_id(&ctx.threads, &args[0])?;
    // GNU returns the stored object itself (identity preserved); a copy
    // would fail `(eq name (thread-name th))`.
    match ctx.threads.thread_name_for_value(args[0]) {
        Some(name) => Ok(name.stored()),
        None => Ok(Value::NIL),
    }
}

/// `(thread-live-p THREAD)` -- check if the thread is alive.
#[doc(hidden)]
pub fn builtin_thread_live_p(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-live-p", &args, 1)?;
    expect_thread_id(&ctx.threads, &args[0])?;
    Ok(Value::bool_val(ctx.threads.thread_alive_for_value(args[0])))
}

/// `(threadp OBJ)` -- type predicate.
///
/// Returns t if OBJ is a known thread object.
#[doc(hidden)]
pub fn builtin_threadp(ctx: &mut crate::emacs_core::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("threadp", &args, 1)?;
    Ok(Value::bool_val(
        ctx.threads.thread_id_from_handle(&args[0]).is_some(),
    ))
}

/// `(thread-signal THREAD ERROR-SYMBOL DATA)` -- send a signal to a thread.
///
/// In our simulation this records the pending error on the target thread.
#[doc(hidden)]
pub fn builtin_thread_signal(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let symbol_mode = ctx.symbol_mode();
    expect_args("thread-signal", &args, 3)?;
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    let error_symbol = args[1];
    let Some(error_symbol_id) = error_symbol.as_symbol_id(symbol_mode) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), error_symbol],
        ));
    };
    let data = args[2];
    if id == ctx.threads.current_thread_id() {
        return Err(signal_with_data_id(error_symbol_id, data));
    }
    // GNU `Fthread_signal` (`src/thread.c:1002-1021`): a signal aimed at the
    // main thread queues `(thread-event SENDER ERROR-SYMBOL DATA)` on the
    // input queue instead of storing an error, in both threading modes.  The
    // main thread's reader delivers it through `special-event-map`
    // (`src/keyboard.c`, THREAD_EVENT; `lisp/thread.el:thread-handle-event`).
    if id == 0 {
        let sender = ctx
            .threads
            .thread_handle(ctx.threads.current_thread_id())
            .ok_or_else(missing_live_thread_registration)?;
        // SAFETY: SENDER is the canonical handle kept in the manager, and
        // ERROR-SYMBOL and DATA are this call's live arguments (rooted on the
        // signalling mutator's stack); the signalling thread is the installed
        // mutator of this evaluator's heap and keeps all three reachable
        // until admission ends.
        let event = unsafe { InFlightThreadEvent::share(ctx, sender, error_symbol, data) }?;
        let target = ctx.threads.ids.main_thread();
        ctx.threads
            .queue_thread_event(target, &event)
            .map_err(mailbox::event_error)?;
        if ctx.green_threads_running() {
            ctx.green_wake_main_for_thread_event();
        }
        return Ok(Value::NIL);
    }
    if ctx.green_threads_running() {
        return ctx.green_thread_signal(args[0], error_symbol, data);
    }
    ctx.threads
        .signal_thread_for_value(args[0], Value::cons(error_symbol, data));
    Ok(Value::NIL)
}

/// `(current-thread)` -- return the current thread object.
#[doc(hidden)]
pub fn builtin_current_thread(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-thread", &args, 0)?;
    let id = ctx.threads.current_thread_id();
    ctx.threads
        .thread_handle(id)
        .ok_or_else(missing_live_thread_registration)
}

/// `(all-threads)` -- return a list of all thread objects.
#[doc(hidden)]
pub fn builtin_all_threads(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("all-threads", &args, 0)?;
    let mut ids = ctx.threads.all_thread_ids();
    ids.sort_unstable();
    let objects: Vec<Value> = ids
        .into_iter()
        .map(|id| {
            ctx.threads
                .thread_handle(id)
                .ok_or_else(missing_live_thread_registration)
        })
        .collect::<Result<_, _>>()?;
    Ok(Value::list(objects))
}

/// `(thread-last-error &optional CLEANUP)` -- return the last error.
///
/// If CLEANUP is non-nil, clear the stored error after returning it.
#[doc(hidden)]
pub fn builtin_thread_last_error(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("thread-last-error"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let cleanup = args.first().is_some_and(|v| v.is_truthy());
    Ok(ctx.threads.last_error(cleanup))
}

/// `(thread--blocker THREAD)` -- return the object THREAD is waiting on.
#[doc(hidden)]
pub fn builtin_thread_blocker(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread--blocker", &args, 1)?;
    expect_thread_id(&ctx.threads, &args[0])?;
    Ok(ctx
        .threads
        .thread_blocker_for_value(args[0])
        .unwrap_or(Value::NIL))
}

/// `(thread-buffer-disposition THREAD)` -- return THREAD's buffer disposition.
#[doc(hidden)]
pub fn builtin_thread_buffer_disposition(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-buffer-disposition", &args, 1)?;
    expect_thread_id(&ctx.threads, &args[0])?;
    Ok(ctx
        .threads
        .thread_buffer_disposition_for_value(args[0])
        .unwrap_or(Value::NIL))
}

/// `(thread-set-buffer-disposition THREAD VALUE)` -- set THREAD's buffer disposition.
#[doc(hidden)]
pub fn builtin_thread_set_buffer_disposition(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-set-buffer-disposition", &args, 2)?;
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    let value = args[1];
    if id == 0 && !value.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("null"), value],
        ));
    }
    ctx.threads
        .set_thread_buffer_disposition_for_value(args[0], value);
    Ok(value)
}

// ===========================================================================
// Mutex builtins
// ===========================================================================

/// `(make-mutex &optional NAME)` -- create a mutex.
#[doc(hidden)]
pub fn builtin_make_mutex(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-mutex"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let name = if let Some(v) = args.first() {
        if v.is_nil() {
            None
        } else {
            Some(ctx.expect_lisp_string(*v)?.clone())
        }
    } else {
        None
    };
    let mutex = ctx.threads.create_mutex(name)?;
    Ok(mutex.value())
}

/// `(mutexp OBJ)` -- type predicate for mutexes.
#[doc(hidden)]
pub fn builtin_mutexp(ctx: &mut crate::emacs_core::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("mutexp", &args, 1)?;
    Ok(Value::bool_val(
        ctx.threads.mutex_id_from_handle(&args[0]).is_some(),
    ))
}

/// `(mutex-name MUTEX)` -- return the mutex's name or nil.
#[doc(hidden)]
pub fn builtin_mutex_name(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutex-name", &args, 1)?;
    let id = expect_mutex_id(&ctx.threads, &args[0])?;
    match ctx.threads.mutex_name(id) {
        Some(name) => Ok(Value::heap_string(name.clone())),
        None => Ok(Value::NIL),
    }
}

/// `(mutex-lock MUTEX)` -- lock a mutex.
///
/// GNU `Fmutex_lock` (`src/thread.c:348-371`) parks the caller on the
/// mutex's condition variable while another thread owns it
/// (`lisp_mutex_lock_for_thread`, `:212-246`); that wait releases the global
/// lock, so other Lisp threads run. The simulation implements the same wait:
/// the main thread waits in place, a worker suspends with the typed retry the
/// release point will re-run.
#[doc(hidden)]
pub fn builtin_mutex_lock(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutex-lock", &args, 1)?;
    let id = expect_mutex_id(&ctx.threads, &args[0])?;
    if ctx.green_threads_running() {
        return ctx.green_mutex_lock(id);
    }
    match ctx.threads.mutex_lock(id) {
        MutexAcquisition::Acquired => Ok(Value::NIL),
        MutexAcquisition::DepthExhausted => Err(mutex_depth_limit()),
        MutexAcquisition::Blocked => sim_mutex_lock_blocked(ctx, args[0], id),
    }
}

fn mutex_depth_limit() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string("Mutex recursion depth limit exceeded")],
    )
}

/// The contended arm of `mutex-lock`: GNU `mutex_lock_callback` waiting in
/// `lisp_mutex_lock_for_thread` (`src/thread.c:212-246`).
fn sim_mutex_lock_blocked(
    ctx: &mut crate::emacs_core::eval::Context,
    mutex_value: Value,
    mutex: MutexHandle,
) -> EvalResult {
    let current = ctx.threads.current_thread_id();
    if current == 0 {
        // The main thread is the simulation's scheduler, so it runs its
        // wait in place: publish the wait object (GNU `event_object`), give
        // every releasable thread the released lock, and retry.
        ctx.threads.set_thread_blocker(current, mutex_value);
        loop {
            let ran = match ctx.sim_release_point(SimReleasePoint::MutexLock) {
                Ok(ran) => ran,
                Err(flow) => {
                    ctx.threads.clear_thread_blocker(current);
                    return Err(flow);
                }
            };
            match ctx.threads.mutex_lock(mutex) {
                MutexAcquisition::Acquired => {
                    ctx.threads.clear_thread_blocker(current);
                    return Ok(Value::NIL);
                }
                MutexAcquisition::DepthExhausted => {
                    ctx.threads.clear_thread_blocker(current);
                    return Err(mutex_depth_limit());
                }
                MutexAcquisition::Blocked if !ran => {
                    // GNU would keep waiting; a simulated main thread none
                    // of whose threads can release the mutex is deadlocked.
                    ctx.threads.clear_thread_blocker(current);
                    return Err(Flow::thread_blocked(mutex_value, Value::NIL));
                }
                MutexAcquisition::Blocked => {}
            }
        }
    }
    // A worker cannot run beside its own suspended frame: release the lock
    // once for the other threads, then suspend with the typed retry GNU
    // keeps on the waiting thread's C stack. Publishing the typed wait
    // before the pass makes a `thread-signal` during it land in this wait
    // (GNU `thread_signal_callback`) instead of killing a running thread.
    ctx.threads
        .set_blocked_wait(current, Some(BlockedWait::MutexLock));
    if let Err(flow) = ctx.sim_release_point(SimReleasePoint::MutexLock) {
        ctx.threads.set_blocked_wait(current, None);
        return Err(flow);
    }
    if let Some(flow) = ctx.threads.take_pending_thread_signal(current) {
        // GNU's wait loop exits on a signal without taking the mutex, and
        // post_acquire_global_lock raises it (`src/thread.c:238-241,105-155`).
        ctx.threads.set_blocked_wait(current, None);
        return Err(flow);
    }
    match ctx.threads.mutex_lock(mutex) {
        MutexAcquisition::Acquired => {
            ctx.threads.set_blocked_wait(current, None);
            Ok(Value::NIL)
        }
        MutexAcquisition::DepthExhausted => {
            ctx.threads.set_blocked_wait(current, None);
            Err(mutex_depth_limit())
        }
        MutexAcquisition::Blocked => Err(Flow::thread_blocked(mutex_value, Value::NIL)),
    }
}

/// `(mutex-unlock MUTEX)` -- unlock a mutex.
#[doc(hidden)]
pub fn builtin_mutex_unlock(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutex-unlock", &args, 1)?;
    let id = expect_mutex_id(&ctx.threads, &args[0])?;
    // GNU `lisp_mutex_unlock` (thread.c:259-263) signals an error when the
    // current thread does not own the mutex (including a never-locked mutex,
    // whose owner is NULL). Match that instead of silently succeeding.
    if !ctx.threads.mutex_owned_by_current_thread(id) {
        return Err(signal(
            "error",
            vec![Value::string("Cannot unlock mutex owned by another thread")],
        ));
    }
    if ctx.green_threads_running() {
        return ctx.green_mutex_unlock(id);
    }
    match ctx.threads.mutex_unlock(id) {
        MutexRelease::Released => {
            ctx.threads.wake_threads_blocked_on(args[0]);
            Ok(Value::NIL)
        }
        MutexRelease::StillLocked => Ok(Value::NIL),
        MutexRelease::NotOwner => Err(signal(
            LispCondition::Error,
            vec![Value::string("Cannot unlock mutex owned by another thread")],
        )),
    }
}

// ===========================================================================
// Condition variable builtins
// ===========================================================================

/// `(make-condition-variable MUTEX &optional NAME)` -- create a condition variable.
#[doc(hidden)]
pub fn builtin_make_condition_variable(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("make-condition-variable", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-condition-variable"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let mutex_id = expect_mutex_id(&ctx.threads, &args[0])?;
    let name = if let Some(value) = args.get(1) {
        if value.is_nil() {
            None
        } else {
            Some(ctx.expect_lisp_string(*value)?.clone())
        }
    } else {
        None
    };
    let condition = ctx.threads.create_condition_variable(mutex_id, name)?;
    Ok(condition.value())
}

/// `(condition-variable-p OBJ)` -- type predicate.
#[doc(hidden)]
pub fn builtin_condition_variable_p(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-variable-p", &args, 1)?;
    Ok(Value::bool_val(
        ctx.threads
            .condition_variable_id_from_handle(&args[0])
            .is_some(),
    ))
}

/// `(condition-name COND)` -- return COND's name string, or nil when unnamed.
#[doc(hidden)]
pub fn builtin_condition_name(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-name", &args, 1)?;
    let id = expect_cv_id(&ctx.threads, &args[0])?;
    match ctx.threads.condition_variable_name(id) {
        Some(name) => Ok(Value::heap_string(name.clone())),
        None => Ok(Value::NIL),
    }
}

/// `(condition-mutex COND)` -- return COND's associated mutex object.
#[doc(hidden)]
pub fn builtin_condition_mutex(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-mutex", &args, 1)?;
    let id = expect_cv_id(&ctx.threads, &args[0])?;
    Ok(ctx.threads.condition_variable_mutex(id).value())
}

/// `(condition-wait COND)` -- wait on a condition variable.
///
/// GNU `Fcondition_wait` (`src/thread.c:478-501`) and
/// `condition_wait_callback` (`:446-476`): release the associated mutex
/// however many times it is held, wait for a notification or an injected
/// signal, reacquire the mutex at the saved count, and only then raise a
/// pending signal. Both waits release the global lock, so every other Lisp
/// thread can run at the release point.
#[doc(hidden)]
pub fn builtin_condition_wait(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-wait", &args, 1)?;
    let id = expect_cv_id(&ctx.threads, &args[0])?;
    let mutex_id = ctx.threads.condition_variable_mutex(id);
    if !ctx.threads.mutex_owned_by_current_thread(mutex_id) {
        return Err(condition_mutex_not_held());
    }
    if ctx.green_threads_running() {
        return ctx.green_condition_wait(id);
    }
    if ctx.threads.current_thread_id() != 0 && !ctx.sim_thread_wait_is_resumable() {
        // A suspended simulated worker resumes as a saved form list; it
        // cannot carry dynamic let-bindings, unwind-protect cleanups,
        // excursions or load state across the switch (the thread-switch
        // path itself requires the specpdl to unwind first, and a
        // `with-mutex` cleanup would then release COND's mutex a second
        // time). GNU parks the whole Lisp stack here. For such nested
        // contexts the simulation keeps its historical degeneration: the
        // wait returns at once and the mutex stays held, so the P11 soak's
        // inline-burn accounting is unchanged. A wait in the worker's
        // top-level body carries only call frames and takes the real
        // handoff path below.
        return Ok(Value::NIL);
    }
    let Some(depth) = ctx.threads.mutex_unlock_for_wait(mutex_id) else {
        return Err(condition_mutex_not_held());
    };
    // GNU `lisp_mutex_unlock_for_wait` (thread.c:276-288) broadcasts the
    // mutex's condition, so mutex waiters may take it while COND waits.
    let mutex_value = ctx.threads.mutex_handle(mutex_id);
    ctx.threads.wake_threads_blocked_on(mutex_value);
    let current = ctx.threads.current_thread_id();
    if current == 0 {
        // The main thread runs its wait in place: publish COND as its
        // `event_object`, let the other threads run at the release point,
        // and recheck whether a notification ended the wait.
        ctx.threads.set_main_wait_object(args[0]);
        let mut notified = false;
        loop {
            let ran = match ctx.sim_release_point(SimReleasePoint::ConditionWait) {
                Ok(ran) => ran,
                Err(flow) => {
                    ctx.threads.clear_main_wait_object();
                    return Err(flow);
                }
            };
            if !ctx.threads.main_waiting_on(&args[0]) {
                notified = true;
                break;
            }
            if !ran {
                break;
            }
        }
        ctx.threads.clear_main_wait_object();
        // Whether or not it was notified, GNU reacquires COND's mutex at
        // the saved count before returning or raising (thread.c:466-475).
        let reacquired =
            ctx.sim_relock_after_wait(mutex_id, depth, SimReleasePoint::ConditionWait)?;
        if !notified || !reacquired {
            // GNU blocks until COND is notified; a simulation with no
            // notifier left cannot make progress, so report the wait instead
            // of hanging.
            return Err(Flow::thread_blocked(args[0], Value::NIL));
        }
        return Ok(Value::NIL);
    }
    // A worker suspends its Lisp stack: publish the typed reacquisition and
    // the wait object, give the other threads their chance, and block.
    ctx.threads
        .set_blocked_wait(current, Some(BlockedWait::ConditionWait(depth)));
    ctx.threads.set_thread_blocker(current, args[0]);
    if let Err(flow) = ctx.sim_release_point(SimReleasePoint::ConditionWait) {
        ctx.threads.set_blocked_wait(current, None);
        ctx.threads.clear_thread_blocker(current);
        return Err(flow);
    }
    if ctx.threads.thread_is_woken(current) {
        // A notification (or an injected signal, which wakes the wait too)
        // arrived while the lock was released. GNU's sys_cond_wait returns;
        // the mutex is reacquired before any pending error is raised.
        ctx.threads.clear_thread_woken(current);
        ctx.threads.clear_thread_blocker(current);
        if !ctx.sim_relock_after_wait(mutex_id, depth, SimReleasePoint::ConditionWait)? {
            // Another thread owns the mutex: GNU waits on the mutex itself
            // and keeps the saved count for the reacquisition.
            ctx.threads.set_thread_blocker(current, mutex_value);
            return Err(Flow::thread_blocked(mutex_value, Value::NIL));
        }
        ctx.threads.set_blocked_wait(current, None);
        if let Some(flow) = ctx.threads.take_pending_thread_signal(current) {
            return Err(flow);
        }
        return Ok(Value::NIL);
    }
    Err(Flow::thread_blocked(args[0], Value::NIL))
}

fn condition_mutex_not_held() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "Condition variable's mutex is not held by current thread",
        )],
    )
}

/// `(condition-notify COND &optional ALL)` -- notify on a condition variable.
///
/// GNU `condition_notify_callback` (`src/thread.c:505-535`) releases COND's
/// mutex, signals the condition, and reacquires the mutex at the saved
/// count. It holds the global lock throughout, so nothing else runs between
/// the release and the reacquisition.
#[doc(hidden)]
pub fn builtin_condition_notify(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("condition-notify", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("condition-notify"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let id = expect_cv_id(&ctx.threads, &args[0])?;
    let mutex_id = ctx.threads.condition_variable_mutex(id);
    if !ctx.threads.mutex_owned_by_current_thread(mutex_id) {
        return Err(condition_mutex_not_held());
    }
    let scope = match args.get(1) {
        Some(all) if all.is_truthy() => NotificationScope::All,
        Some(_) | None => NotificationScope::One,
    };
    if ctx.green_threads_running() {
        return ctx.green_condition_notify(id, scope);
    }
    let Some(depth) = ctx.threads.mutex_unlock_for_wait(mutex_id) else {
        return Err(condition_mutex_not_held());
    };
    let mutex_value = ctx.threads.mutex_handle(mutex_id);
    // GNU lisp_mutex_unlock_for_wait broadcasts the mutex condition.
    ctx.threads.wake_threads_blocked_on(mutex_value);
    match scope {
        NotificationScope::All => {
            ctx.threads.wake_threads_blocked_on(args[0]);
        }
        NotificationScope::One => {
            ctx.threads.wake_one_thread_blocked_on(args[0]);
        }
    }
    // Notifies never observe a stolen mutex: no other Lisp thread runs
    // between `lisp_mutex_unlock_for_wait` and `lisp_mutex_lock` here.
    match ctx.sim_relock_after_wait(mutex_id, depth, SimReleasePoint::ConditionWait) {
        Ok(true) => Ok(Value::NIL),
        Ok(false) => Err(Flow::thread_blocked(mutex_value, Value::NIL)),
        Err(flow) => Err(flow),
    }
}

// ===========================================================================
// Special form: with-mutex
// ===========================================================================

/// `(with-mutex MUTEX BODY...)` -- execute BODY with MUTEX locked.
///
/// This is a special form: MUTEX is evaluated, the lock is acquired, BODY is
/// executed as an implicit progn, and the lock is released on exit
/// (even if BODY signals an error).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn sf_with_mutex(eval: &mut super::eval::Context, tail: &[Value]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::cons(Value::fixnum(1), Value::fixnum(1)),
                Value::fixnum(0),
            ],
        ));
    }
    let mutex_val = eval.eval_sub(tail[0])?;
    let mutex_id = expect_mutex_id(&eval.threads, &mutex_val)?;
    if !eval.threads.is_mutex(mutex_id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), mutex_val],
        ));
    }

    eval.threads.mutex_lock(mutex_id);
    let mut result: EvalResult = Ok(Value::NIL);
    for form in &tail[1..] {
        result = eval.eval_sub(*form);
        if result.is_err() {
            break;
        }
    }
    eval.threads.mutex_unlock(mutex_id);
    result
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/threads_test.rs"]
mod tests;
