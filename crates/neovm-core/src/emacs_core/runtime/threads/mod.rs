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

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_min_args};
use std::{collections::HashMap, time::Duration};

use super::error::{
    EvalResult, Flow, FlowKind, FlowResultExt, make_signal_binding_value, signal,
    signal_from_binding_value, signal_with_data_id,
};
use super::value::{Value, ValueKind, eq_value, list_to_vec};
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;
use crate::tagged::transport::SharedRoot;

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

pub(crate) fn make_sleep_blocker(seconds: f64) -> Value {
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
    match crate::emacs_core::timer::WaitTimeout::from(items[1].xfloat()) {
        crate::emacs_core::timer::WaitTimeout::Poll => None,
        crate::emacs_core::timer::WaitTimeout::For(duration) => Some(duration),
        crate::emacs_core::timer::WaitTimeout::Forever => Some(Duration::MAX),
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
pub struct ThreadName(crate::heap_types::threading::ThreadName);

impl ThreadName {
    /// No NAME was supplied (GNU stores nil).
    pub const UNNAMED: Self = Self(crate::heap_types::threading::ThreadName::UNNAMED);

    /// GNU `make-thread`'s `if (!NILP (name)) CHECK_STRING (name)`.
    pub fn from_lisp(name: Value) -> Result<Self, Flow> {
        crate::heap_types::threading::ThreadName::from_value(name)
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

/// One GNU `THREAD_EVENT` waiting for its target thread's reader.
///
/// GNU's `thread-signal` targeted at the main thread does not store an error
/// there: it queues an input event whose Lisp form is
/// `(thread-event SENDER ERROR-SYMBOL DATA)` (`src/thread.c:1007-1013`), and
/// the main thread's reader delivers it through `special-event-map`
/// (`src/keyboard.c`, THREAD_EVENT; `lisp/thread.el:thread-handle-event`).
/// The native transport mailbox belongs to the target's live registration
/// (thread 0 is main). It is separate from the heap object's ordinary traced
/// children, so durable transport roots cannot create an object self-cycle.
/// The main Context drains this mailbox into its keyboard queue in
/// `keyboard.rs` (`stage_pending_thread_events`).
///
/// The fields are [`SharedRoot`]s, not raw values: a Green worker queues the
/// event on its own mutator thread and the main mutator consumes it later, so
/// the three Lisp objects must stay alive and cross the mutator boundary in
/// the heap's designed transport (`tagged::transport`).  The keyboard queue
/// holds the same handles (`InputEvent::ThreadEvent`), which keeps that
/// queue `Send` like every other input event.
#[derive(Clone, Debug)]
pub struct PendingThreadEvent {
    pub sender: SharedRoot,
    pub error_symbol: SharedRoot,
    pub data: SharedRoot,
}

impl PendingThreadEvent {
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
    /// [`SharedRoot::new`](crate::tagged::transport::SharedRoot::new).
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

/// The runtime failure mode of [`PendingThreadEvent::share`], as a Lisp error.
///
/// GNU's `Fthread_signal` has no counterpart (its kbd buffer is process-wide
/// and untyped), so this names the transport invariant instead of inventing a
/// GNU condition: the event must be rooted on the heap the signalling evaluator
/// owns.
fn thread_event_transport_error(error: crate::tagged::transport::SharedRootError) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "thread-signal cannot root its event on this evaluator's heap: {error}"
        ))],
    )
}

mod layout;
mod manager;

pub(crate) use crate::heap_types::threading::{
    CondVarId, CondVarPayload, JoinObservation, MutexId, MutexOwnership, MutexPayload,
    MutexReference, RecursionDepth, RunState, StoredThreadEvent, ThreadId, ThreadIds,
    ThreadOutcome, ThreadPayload, ThreadPhase,
};

/// A validated object identity. This word is not a GC root by itself: Lisp
/// arguments or a published execution record must retain it across a safepoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MutexHandle {
    value: Value,
    id: MutexId,
}

impl MutexHandle {
    pub(crate) fn value(self) -> Value {
        self.value
    }
    pub(crate) fn native_key(self) -> u64 {
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
    pub(crate) fn value(self) -> Value {
        self.value
    }
    pub(crate) fn native_key(self) -> u64 {
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
    const MAIN: Self = Self(0);

    #[inline]
    fn native_key(&self) -> u64 {
        self.0
    }
}

#[repr(C)]
pub struct ThreadManager {
    last_error: Option<Value>,
    threads: HashMap<ThreadId, Value>,
    transport_mailboxes: HashMap<ThreadId, Vec<PendingThreadEvent>>,
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
            transport_mailboxes: _,
            before_issuer: _,
            ids: _,
            current_thread: _,
            tail: _,
        } = self;
        roots.extend(threads.values().copied());
        if let Some(error) = last_error {
            roots.push(*error);
        }
        // In-flight events remain external transport roots until S2 transfers
        // them to stored object edges. No SharedRoot is inside a heap payload.
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
    fn veclike(self) -> crate::tagged::header::VecLikeType {
        match self {
            Self::Thread => crate::tagged::header::VecLikeType::Thread,
            Self::Mutex => crate::tagged::header::VecLikeType::Mutex,
            Self::CondVar => crate::tagged::header::VecLikeType::CondVar,
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
/// In our single-threaded simulation the function is executed immediately.
/// Returns the opaque heap object that owns the thread's state.
pub(crate) fn builtin_make_thread(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    // A Green World runs threads as owners on their own OS threads; the
    // simulated exchange never runs beside admission.
    if super::eval::green_world(eval) {
        return super::eval::green_make_thread(eval, &args);
    }
    let (thread_id, function) = prepare_make_thread(&mut eval.threads, &args)?;
    eval.threads
        .set_thread_current_buffer(thread_id, eval.buffers.current_buffer_id());
    eval.with_sim_creation_local_bindings(|eval| {
        let runtime_state = match enter_thread_runtime(eval, thread_id) {
            Ok(state) => state,
            Err(flow) => {
                eval.threads.discard_unstarted_thread(thread_id);
                return Err(flow);
            }
        };
        let result = eval.apply(function, vec![]);
        let exit_result = exit_thread_runtime(eval, thread_id, runtime_state);
        let thread_result = finish_make_thread_result(&mut eval.threads, thread_id, result);
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
    previous_buffer_id: Option<crate::buffer::BufferId>,
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
        .or_else(|| eval.buffers.current_buffer_id());
    if let Some(thread_buffer_id) = eval.threads.thread_current_buffer(thread_id) {
        if eval.buffers.current_buffer_id() != Some(thread_buffer_id) {
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
    Ok(ThreadRuntimeState {
        previous_thread_id,
        previous_buffer_id,
        previous_dynamic_bindings,
    })
}

pub(crate) fn exit_thread_runtime(
    eval: &mut super::eval::Context,
    thread_id: u64,
    runtime_state: ThreadRuntimeState,
) -> Result<(), Flow> {
    eval.threads
        .set_thread_current_buffer(thread_id, eval.buffers.current_buffer_id());
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
        }
        // kill-emacs is process-wide in GNU (Fkill_emacs exits, whichever
        // thread runs it), so it is not recorded as this thread's error — it
        // unwinds to the caller and ends the process.
        Err(flow @ FlowKind::Shutdown(_)) => return Err(Flow::from_kind(flow)),
    }

    Ok(thread)
}

/// `(thread-join THREAD)` -- wait for thread completion.
///
/// Returns the thread's result, resuming a simulated blocked thread when needed,
/// or re-signals a `thread-signal` error snapshot the GNU way.
pub(crate) fn builtin_thread_join(
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

fn resume_blocked_thread(ctx: &mut crate::emacs_core::eval::Context, thread_id: u64) -> EvalResult {
    let Some(remaining_forms) = ctx.threads.blocked_remaining_forms_if_ready(thread_id) else {
        return Ok(Value::NIL);
    };
    let runtime_state = enter_thread_runtime(ctx, thread_id)?;
    let taken_forms = ctx
        .threads
        .take_blocked_remaining_forms(thread_id)
        .expect("ready blocked continuation remains installed across runtime entry");
    debug_assert_eq!(taken_forms, remaining_forms);
    let result = if remaining_forms
        .closure_slot(crate::tagged::header::CLOSURE_ARGLIST)
        .is_some()
    {
        ctx.apply(remaining_forms, vec![])
    } else if let Some((var, body, handlers, lexenv)) =
        thread_condition_case_continuation_parts(remaining_forms)
    {
        ctx.resume_thread_condition_case_continuation(var, body, handlers, lexenv)
    } else {
        ctx.eval_lambda_body_value(remaining_forms)
    };
    let exit_result = exit_thread_runtime(ctx, thread_id, runtime_state);
    let thread_result = finish_make_thread_result(&mut ctx.threads, thread_id, result);
    exit_result?;
    thread_result?;
    Ok(Value::NIL)
}

/// `(thread-yield)` -- yield the current thread.
pub(crate) fn builtin_thread_yield(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if ctx.green_threads_running() {
        return ctx.green_thread_yield(&args);
    }
    expect_args("thread-yield", &args, 0)?;
    if ctx.threads.current_thread_id() != 0 {
        return Err(Flow::thread_blocked(
            Value::symbol("thread-yield"),
            Value::NIL,
        ));
    }
    Ok(Value::NIL)
}

/// `(thread-name THREAD)` -- return the thread's name or nil.
pub(crate) fn builtin_thread_name(
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
pub(crate) fn builtin_thread_live_p(
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
pub(crate) fn builtin_threadp(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("threadp", &args, 1)?;
    Ok(Value::bool_val(
        ctx.threads.thread_id_from_handle(&args[0]).is_some(),
    ))
}

/// `(thread-signal THREAD ERROR-SYMBOL DATA)` -- send a signal to a thread.
///
/// In our simulation this records the pending error on the target thread.
pub(crate) fn builtin_thread_signal(
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
        let event = unsafe { PendingThreadEvent::share(ctx, sender, error_symbol, data) }?;
        ctx.threads.queue_thread_event(id, event);
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
pub(crate) fn builtin_current_thread(
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
pub(crate) fn builtin_all_threads(
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
pub(crate) fn builtin_thread_last_error(
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
pub(crate) fn builtin_thread_blocker(
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
pub(crate) fn builtin_thread_buffer_disposition(
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
pub(crate) fn builtin_thread_set_buffer_disposition(
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
pub(crate) fn builtin_make_mutex(
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
pub(crate) fn builtin_mutexp(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutexp", &args, 1)?;
    Ok(Value::bool_val(
        ctx.threads.mutex_id_from_handle(&args[0]).is_some(),
    ))
}

/// `(mutex-name MUTEX)` -- return the mutex's name or nil.
pub(crate) fn builtin_mutex_name(
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
/// In single-threaded mode, this always succeeds immediately.
pub(crate) fn builtin_mutex_lock(
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
        MutexAcquisition::Blocked => Err(Flow::thread_blocked(args[0], Value::NIL)),
        MutexAcquisition::DepthExhausted => Err(signal(
            LispCondition::Error,
            vec![Value::string("Mutex recursion depth limit exceeded")],
        )),
    }
}

/// `(mutex-unlock MUTEX)` -- unlock a mutex.
pub(crate) fn builtin_mutex_unlock(
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
pub(crate) fn builtin_make_condition_variable(
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
pub(crate) fn builtin_condition_variable_p(
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
pub(crate) fn builtin_condition_name(
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
pub(crate) fn builtin_condition_mutex(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-mutex", &args, 1)?;
    let id = expect_cv_id(&ctx.threads, &args[0])?;
    Ok(ctx.threads.condition_variable_mutex(id).value())
}

/// `(condition-wait COND)` -- wait on a condition variable.
///
/// In single-threaded mode this is a no-op.
pub(crate) fn builtin_condition_wait(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-wait", &args, 1)?;
    let id = expect_cv_id(&ctx.threads, &args[0])?;
    let mutex_id = ctx.threads.condition_variable_mutex(id);
    if !ctx.threads.mutex_owned_by_current_thread(mutex_id) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Condition variable's mutex is not held by current thread",
            )],
        ));
    }
    if ctx.green_threads_running() {
        return ctx.green_condition_wait(id, mutex_id);
    }
    Ok(Value::NIL)
}

/// `(condition-notify COND &optional ALL)` -- notify on a condition variable.
///
/// No-op in single-threaded mode.
pub(crate) fn builtin_condition_notify(
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
        return Err(signal(
            "error",
            vec![Value::string(
                "Condition variable's mutex is not held by current thread",
            )],
        ));
    }
    if ctx.green_threads_running() {
        let scope = match args.get(1) {
            Some(all) if all.is_truthy() => NotificationScope::All,
            Some(_) | None => NotificationScope::One,
        };
        return ctx.green_condition_notify(id, scope);
    }
    Ok(Value::NIL)
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
