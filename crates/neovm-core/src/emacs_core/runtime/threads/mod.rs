//! Emacs-compatible threading primitives for the Elisp VM.
//!
//! Emacs threading is cooperative: only one thread runs at a time, yielding at
//! certain well-defined points.  Since our VM is single-threaded, threads are
//! *simulated* — `make-thread` stores the function and runs it immediately.
//! The key goal is API compatibility so that Elisp packages using threads,
//! mutexes, and condition variables continue to work without error.
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
    EvalResult, Flow, make_signal_binding_value, signal, signal_from_binding_value,
    signal_with_data_id,
};
use super::value::{Value, ValueKind, eq_value, list_to_vec};
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;

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
    if items.len() != 5 || !items[0].is_symbol_named(CONDITION_CASE_CONTINUATION_MARKER) {
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
    if items.len() != 2 || !items[0].is_symbol_named(SLEEP_BLOCKER_MARKER) {
        return None;
    }
    let seconds = items[1].xfloat();
    seconds
        .is_finite()
        .then_some(seconds)
        .filter(|seconds| *seconds > 0.0)
        .map(Duration::from_secs_f64)
}

// ---------------------------------------------------------------------------
// Thread state
// ---------------------------------------------------------------------------

/// Status of a simulated thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadStatus {
    /// Thread has been created but not yet run.
    Created,
    /// Thread is currently running (in our model, at most one can be Running).
    Running,
    /// Thread is suspended at a cooperative blocking primitive.
    Blocked,
    /// Thread has finished successfully.
    Finished,
    /// Thread was terminated by an error / signal.
    Signaled,
}

/// Per-thread bookkeeping.
#[derive(Clone, Debug)]
pub struct ThreadState {
    /// Unique thread id.  0 is always the main thread.
    pub id: u64,
    /// Optional human-readable name.
    pub name: Option<LispString>,
    /// The function to invoke (value passed to `make-thread`).
    pub function: Value,
    /// Current status.
    pub status: ThreadStatus,
    /// Return value after the thread finishes (or nil).
    pub result: Value,
    /// Last error that occurred in this thread, if any.
    pub last_error: Option<Value>,
    /// Whether this thread has already been joined at least once.
    pub joined: bool,
    /// What should happen if this thread's current buffer is killed.
    pub buffer_disposition: Value,
    /// Current buffer owned by this thread.
    pub current_buffer: Option<crate::buffer::BufferId>,
    /// Object this thread is currently blocked on.
    pub event_object: Value,
    /// Pending or terminal signal symbol for this thread.
    pub error_symbol: Value,
    /// Pending or terminal signal data for this thread.
    pub error_data: Value,
    /// Resume thunk or remaining top-level forms for a cooperative block.
    pub blocked_remaining_forms: Value,
}

// ---------------------------------------------------------------------------
// Mutex state
// ---------------------------------------------------------------------------

/// A cooperative mutex.
///
/// GNU's Lisp mutex ownership is maintained above the platform thread backend:
/// a contended `mutex-lock` waits without changing the owner, then acquires
/// only after the owning thread unlocks.  The simulated VM does not yet have a
/// resumable wait frame for every blocking primitive, but the ownership state
/// must still follow GNU's invariant.
#[derive(Clone, Debug)]
pub struct MutexState {
    pub id: u64,
    pub name: Option<LispString>,
    /// Id of the thread that currently holds the lock, or `None`.
    pub owner: Option<u64>,
    /// Recursive lock count.
    pub lock_count: u32,
}

// ---------------------------------------------------------------------------
// Condition variable state
// ---------------------------------------------------------------------------

/// A condition variable bound to a particular mutex.
#[derive(Clone, Debug)]
pub struct ConditionVarState {
    pub id: u64,
    pub name: Option<LispString>,
    /// The mutex id this condition variable is associated with.
    pub mutex_id: u64,
}

// ---------------------------------------------------------------------------
// ThreadManager
// ---------------------------------------------------------------------------

/// Central registry for threads, mutexes, and condition variables.
pub struct ThreadManager {
    threads: HashMap<u64, ThreadState>,
    next_id: u64,
    current_thread: u64, // ID of currently running thread (0 = main)
    thread_handles: HashMap<u64, Value>,
    mutexes: HashMap<u64, MutexState>,
    next_mutex_id: u64,
    mutex_handles: HashMap<u64, Value>,
    condition_vars: HashMap<u64, ConditionVarState>,
    next_cv_id: u64,
    condition_var_handles: HashMap<u64, Value>,
    /// Global last-error value (returned by `thread-last-error`).
    last_error: Option<Value>,
}

impl ThreadManager {
    /// Create a new manager with the main thread pre-registered.
    pub fn new() -> Self {
        let mut threads = HashMap::new();
        threads.insert(
            0,
            ThreadState {
                id: 0,
                name: None,
                function: Value::NIL,
                status: ThreadStatus::Running,
                result: Value::NIL,
                last_error: None,
                joined: false,
                buffer_disposition: Value::NIL,
                current_buffer: None,
                event_object: Value::NIL,
                error_symbol: Value::NIL,
                error_data: Value::NIL,
                blocked_remaining_forms: Value::NIL,
            },
        );
        let mut thread_handles = HashMap::new();
        thread_handles.insert(0, tagged_object_value("thread", 0));
        Self {
            threads,
            next_id: 1,
            current_thread: 0,
            thread_handles,
            mutexes: HashMap::new(),
            next_mutex_id: 1,
            mutex_handles: HashMap::new(),
            condition_vars: HashMap::new(),
            next_cv_id: 1,
            condition_var_handles: HashMap::new(),
            last_error: None,
        }
    }

    // -- Thread operations --------------------------------------------------

    /// Create a new thread.  Returns the id.
    pub fn create_thread(&mut self, function: Value, name: Option<LispString>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.threads.insert(
            id,
            ThreadState {
                id,
                name,
                function,
                status: ThreadStatus::Created,
                result: Value::NIL,
                last_error: None,
                joined: false,
                buffer_disposition: Value::NIL,
                current_buffer: None,
                event_object: Value::NIL,
                error_symbol: Value::NIL,
                error_data: Value::NIL,
                blocked_remaining_forms: Value::NIL,
            },
        );
        self.thread_handles
            .insert(id, tagged_object_value("thread", id));
        id
    }

    /// Mark a thread as Running.
    pub fn start_thread(&mut self, id: u64) {
        if let Some(t) = self.threads.get_mut(&id) {
            t.status = ThreadStatus::Running;
        }
    }

    /// Remove a newly created thread whose runtime entry failed before its
    /// handle could be returned to Lisp.
    pub fn discard_unstarted_thread(&mut self, id: u64) {
        debug_assert_ne!(id, self.current_thread);
        self.threads.remove(&id);
        self.thread_handles.remove(&id);
    }

    /// Set the currently running thread and return the previous thread id.
    pub fn enter_thread(&mut self, id: u64) -> u64 {
        let saved = self.current_thread;
        self.current_thread = id;
        saved
    }

    /// Restore the previously running thread id.
    pub fn restore_thread(&mut self, id: u64) {
        self.current_thread = id;
    }

    /// Mark a thread as Finished with the given result.
    pub fn finish_thread(&mut self, id: u64, result: Value) {
        if let Some(t) = self.threads.get_mut(&id) {
            t.status = ThreadStatus::Finished;
            t.result = result;
        }
    }

    /// Mark a thread as Signaled by an *external* `thread-signal`.
    ///
    /// GNU `thread_set_error` (thread.c:984-991) records the error in the
    /// thread's `error_symbol`/`error_data`, which is what a blocked
    /// `thread-join` re-raises. Only `thread-signal` reaches this path.
    pub fn signal_thread(&mut self, id: u64, error: Value) {
        if let Some(t) = self.threads.get_mut(&id) {
            t.last_error = Some(error);
            if let Some((symbol, data)) = split_signal_binding_value(error) {
                t.error_symbol = symbol;
                t.error_data = data;
            } else {
                t.error_symbol = Value::NIL;
                t.error_data = Value::NIL;
            }
            if t.status == ThreadStatus::Blocked {
                t.event_object = Value::NIL;
            } else {
                t.status = ThreadStatus::Signaled;
            }
        }
    }

    /// Record a thread's terminal *internal* error (one signalled inside the
    /// thread function itself).
    ///
    /// GNU `record_thread_error` (thread.c:753-758) stores such an error only
    /// for `thread-last-error`; unlike an external `thread-signal` it leaves
    /// the thread's `error_symbol` nil, so `thread-join` returns the thread's
    /// (nil) result rather than re-raising the error.
    pub fn record_thread_internal_error(&mut self, id: u64, error: Value) {
        if let Some(t) = self.threads.get_mut(&id) {
            t.status = ThreadStatus::Signaled;
            t.last_error = Some(error);
            t.error_symbol = Value::NIL;
            t.error_data = Value::NIL;
        }
    }

    /// The pending re-raise for `thread-join`: GNU re-signals only an error
    /// injected by `thread-signal` (recorded in `error_symbol`), never an
    /// internal one. Returns the `Flow` to re-raise, or `None`.
    pub fn pending_thread_signal(&self, id: u64) -> Option<Flow> {
        let thread = self.threads.get(&id)?;
        if thread.error_symbol.is_nil() {
            return None;
        }
        thread.last_error.and_then(signal_from_binding_value)
    }

    pub fn take_pending_thread_signal(&mut self, id: u64) -> Option<Flow> {
        let thread = self.threads.get_mut(&id)?;
        if thread.error_symbol.is_nil() {
            return None;
        }
        let symbol = thread.error_symbol;
        let data = thread.error_data;
        thread.error_symbol = Value::NIL;
        thread.error_data = Value::NIL;
        let sym_id = symbol.as_symbol_id()?;
        Some(signal_with_data_id(sym_id, data))
    }

    /// Publish an error value for `thread-last-error`.
    pub fn record_last_error(&mut self, error: Value) {
        self.last_error = Some(error);
    }

    /// Get the id of the currently running thread.
    pub fn current_thread_id(&self) -> u64 {
        self.current_thread
    }

    /// Look up a thread by id.
    pub fn get_thread(&self, id: u64) -> Option<&ThreadState> {
        self.threads.get(&id)
    }

    /// Check if a thread is alive from Elisp's point of view.
    pub fn thread_alive_p(&self, id: u64) -> bool {
        self.threads.get(&id).is_some_and(|t| match t.status {
            ThreadStatus::Created | ThreadStatus::Running | ThreadStatus::Blocked => true,
            ThreadStatus::Finished => !t.joined,
            ThreadStatus::Signaled => false,
        })
    }

    /// Get thread name.
    pub fn thread_name(&self, id: u64) -> Option<&LispString> {
        self.threads.get(&id).and_then(|t| t.name.as_ref())
    }

    /// Check if a value represents a known thread id.
    pub fn is_thread(&self, id: u64) -> bool {
        self.threads.contains_key(&id)
    }

    /// Return canonical handle object for thread id.
    pub fn thread_handle(&self, id: u64) -> Option<Value> {
        self.thread_handles.get(&id).cloned()
    }

    /// Return the thread id iff VALUE is the canonical thread handle object.
    pub fn thread_id_from_handle(&self, value: &Value) -> Option<u64> {
        canonical_handle_id(&self.thread_handles, value, "thread")
    }

    /// Return all thread ids.
    pub fn all_thread_ids(&self) -> Vec<u64> {
        self.threads
            .keys()
            .filter_map(|id| self.thread_alive_p(*id).then_some(*id))
            .collect()
    }

    /// Return thread result (for join).
    pub fn thread_result(&self, id: u64) -> Value {
        self.threads
            .get(&id)
            .map(|t| t.result)
            .unwrap_or(Value::NIL)
    }

    /// Mark a thread as joined and return its terminal error, if any.
    pub fn join_thread(&mut self, id: u64) -> Option<Value> {
        let thread = self.threads.get_mut(&id)?;
        thread.joined = true;
        thread.last_error
    }

    pub fn block_thread(&mut self, id: u64, blocker: Value, remaining_forms: Value) {
        if let Some(thread) = self.threads.get_mut(&id) {
            thread.status = ThreadStatus::Blocked;
            thread.event_object = blocker;
            thread.blocked_remaining_forms = remaining_forms;
        }
    }

    pub fn wake_threads_blocked_on(&mut self, blocker: Value) {
        for thread in self.threads.values_mut() {
            if thread.status == ThreadStatus::Blocked && eq_value(&thread.event_object, &blocker) {
                thread.event_object = Value::NIL;
            }
        }
    }

    pub fn thread_ready_to_resume(&self, id: u64) -> bool {
        self.threads.get(&id).is_some_and(|thread| {
            thread.status == ThreadStatus::Blocked && thread.event_object.is_nil()
        })
    }

    pub fn blocked_sleep_duration(&self, id: u64) -> Option<Duration> {
        let thread = self.threads.get(&id)?;
        if thread.status != ThreadStatus::Blocked || thread.event_object.is_nil() {
            return None;
        }
        sleep_duration_from_blocker(thread.event_object)
    }

    pub fn wake_thread(&mut self, id: u64) {
        if let Some(thread) = self.threads.get_mut(&id)
            && thread.status == ThreadStatus::Blocked
        {
            thread.event_object = Value::NIL;
        }
    }

    pub fn take_blocked_remaining_forms(&mut self, id: u64) -> Option<Value> {
        let thread = self.threads.get_mut(&id)?;
        if thread.status != ThreadStatus::Blocked || !thread.event_object.is_nil() {
            return None;
        }
        let remaining = thread.blocked_remaining_forms;
        thread.blocked_remaining_forms = Value::NIL;
        thread.status = ThreadStatus::Running;
        Some(remaining)
    }

    pub fn blocked_remaining_forms_if_ready(&self, id: u64) -> Option<Value> {
        let thread = self.threads.get(&id)?;
        (thread.status == ThreadStatus::Blocked && thread.event_object.is_nil())
            .then_some(thread.blocked_remaining_forms)
    }

    /// Get and optionally clear the global last-error.
    pub fn last_error(&mut self, cleanup: bool) -> Value {
        let val = self.last_error.unwrap_or(Value::NIL);
        if cleanup {
            self.last_error = None;
        }
        val
    }

    pub fn thread_buffer_disposition(&self, id: u64) -> Option<Value> {
        self.threads
            .get(&id)
            .map(|thread| thread.buffer_disposition)
    }

    pub fn set_thread_buffer_disposition(&mut self, id: u64, value: Value) -> bool {
        let Some(thread) = self.threads.get_mut(&id) else {
            return false;
        };
        thread.buffer_disposition = value;
        true
    }

    pub fn thread_current_buffer(&self, id: u64) -> Option<crate::buffer::BufferId> {
        self.threads
            .get(&id)
            .and_then(|thread| thread.current_buffer)
    }

    pub fn set_thread_current_buffer(
        &mut self,
        id: u64,
        buffer_id: Option<crate::buffer::BufferId>,
    ) -> bool {
        let Some(thread) = self.threads.get_mut(&id) else {
            return false;
        };
        thread.current_buffer = buffer_id;
        true
    }

    pub fn thread_blocker(&self, id: u64) -> Option<Value> {
        self.threads.get(&id).map(|thread| thread.event_object)
    }

    pub fn set_thread_blocker(&mut self, id: u64, blocker: Value) -> bool {
        let Some(thread) = self.threads.get_mut(&id) else {
            return false;
        };
        thread.event_object = blocker;
        true
    }

    pub fn clear_thread_blocker(&mut self, id: u64) -> bool {
        self.set_thread_blocker(id, Value::NIL)
    }

    // -- Mutex operations ---------------------------------------------------

    /// Create a new mutex.  Returns the id.
    pub fn create_mutex(&mut self, name: Option<LispString>) -> u64 {
        let id = self.next_mutex_id;
        self.next_mutex_id += 1;
        self.mutexes.insert(
            id,
            MutexState {
                id,
                name,
                owner: None,
                lock_count: 0,
            },
        );
        self.mutex_handles
            .insert(id, tagged_object_value("mutex", id));
        id
    }

    /// Lock a mutex on behalf of the current thread.
    ///
    /// Returns true if the current thread owns the mutex after this call.
    /// Returns false when another thread owns it; callers that can suspend
    /// should wait and retry after unlock, mirroring GNU `thread.c`.
    pub fn mutex_lock(&mut self, mutex_id: u64) -> bool {
        let current = self.current_thread;
        if let Some(m) = self.mutexes.get_mut(&mutex_id) {
            match m.owner {
                None => {
                    m.owner = Some(current);
                    m.lock_count = 1;
                    true
                }
                Some(owner) if owner == current => {
                    // Recursive lock.
                    m.lock_count += 1;
                    true
                }
                Some(_) => false,
            }
        } else {
            false
        }
    }

    /// Unlock a mutex.  Returns false if the mutex doesn't exist or is
    /// not held by the current thread.
    pub fn mutex_unlock(&mut self, mutex_id: u64) -> bool {
        let current = self.current_thread;
        if let Some(m) = self.mutexes.get_mut(&mutex_id) {
            if m.owner == Some(current) {
                m.lock_count = m.lock_count.saturating_sub(1);
                if m.lock_count == 0 {
                    m.owner = None;
                }
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Check if a value represents a known mutex id.
    pub fn is_mutex(&self, id: u64) -> bool {
        self.mutexes.contains_key(&id)
    }

    /// Return canonical handle object for mutex id.
    pub fn mutex_handle(&self, id: u64) -> Option<Value> {
        self.mutex_handles.get(&id).cloned()
    }

    /// Return the mutex id iff VALUE is the canonical mutex handle object.
    pub fn mutex_id_from_handle(&self, value: &Value) -> Option<u64> {
        canonical_handle_id(&self.mutex_handles, value, "mutex")
    }

    /// Get mutex name.
    pub fn mutex_name(&self, id: u64) -> Option<&LispString> {
        self.mutexes.get(&id).and_then(|m| m.name.as_ref())
    }

    /// Return true when MUTEX-ID is owned by the current thread.
    pub fn mutex_owned_by_current_thread(&self, mutex_id: u64) -> bool {
        self.mutexes
            .get(&mutex_id)
            .is_some_and(|m| m.owner == Some(self.current_thread))
    }

    // -- Condition variable operations --------------------------------------

    /// Create a condition variable associated with the given mutex.
    pub fn create_condition_variable(
        &mut self,
        mutex_id: u64,
        name: Option<LispString>,
    ) -> Option<u64> {
        if !self.mutexes.contains_key(&mutex_id) {
            return None;
        }
        let id = self.next_cv_id;
        self.next_cv_id += 1;
        self.condition_vars
            .insert(id, ConditionVarState { id, name, mutex_id });
        self.condition_var_handles
            .insert(id, tagged_object_value("condition-variable", id));
        Some(id)
    }

    /// Check if a value represents a known condition variable id.
    pub fn is_condition_variable(&self, id: u64) -> bool {
        self.condition_vars.contains_key(&id)
    }

    /// Return canonical handle object for condition variable id.
    pub fn condition_variable_handle(&self, id: u64) -> Option<Value> {
        self.condition_var_handles.get(&id).cloned()
    }

    /// Return the condition variable id iff VALUE is the canonical handle.
    pub fn condition_variable_id_from_handle(&self, value: &Value) -> Option<u64> {
        canonical_handle_id(&self.condition_var_handles, value, "condition-variable")
    }

    /// Get condition variable name.
    pub fn condition_variable_name(&self, id: u64) -> Option<&LispString> {
        self.condition_vars.get(&id).and_then(|cv| cv.name.as_ref())
    }

    /// Get the mutex associated with a condition variable.
    pub fn condition_variable_mutex(&self, cv_id: u64) -> Option<u64> {
        self.condition_vars.get(&cv_id).map(|cv| cv.mutex_id)
    }
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for ThreadManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for thread in self.threads.values() {
            roots.push(thread.function);
            roots.push(thread.result);
            roots.push(thread.buffer_disposition);
            roots.push(thread.event_object);
            roots.push(thread.error_symbol);
            roots.push(thread.error_data);
            roots.push(thread.blocked_remaining_forms);
            if let Some(buffer_id) = thread.current_buffer {
                roots.push(Value::make_buffer(buffer_id));
            }
            if let Some(ref err) = thread.last_error {
                roots.push(*err);
            }
        }
        for value in self.thread_handles.values() {
            roots.push(*value);
        }
        for value in self.mutex_handles.values() {
            roots.push(*value);
        }
        for value in self.condition_var_handles.values() {
            roots.push(*value);
        }
        if let Some(ref err) = self.last_error {
            roots.push(*err);
        }
    }
}

// ===========================================================================
// Argument helpers
// ===========================================================================

fn tagged_object_value(tag: &str, id: u64) -> Value {
    Value::cons(Value::symbol(tag), Value::fixnum(id as i64))
}

fn tagged_object_id(value: &Value, expected_tag: &str) -> Option<u64> {
    if !value.is_cons() {
        return None;
    };
    let pair_car = value.cons_car();
    let pair_cdr = value.cons_cdr();
    if pair_car.as_symbol_name() != Some(expected_tag) {
        return None;
    }
    match pair_cdr.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Some(n as u64),
        _ => None,
    }
}

fn canonical_handle_id(handles: &HashMap<u64, Value>, value: &Value, tag: &str) -> Option<u64> {
    let id = tagged_object_id(value, tag)?;
    let canonical = handles.get(&id)?;
    if eq_value(canonical, value) {
        Some(id)
    } else {
        None
    }
}

fn split_signal_binding_value(value: Value) -> Option<(Value, Value)> {
    if !value.is_cons() {
        return None;
    };
    let pair_car = value.cons_car();
    let pair_cdr = value.cons_cdr();
    pair_car.as_symbol_id()?;
    Some((pair_car, pair_cdr))
}

/// Extract a thread id from a canonical thread handle object.
fn expect_thread_id(manager: &ThreadManager, value: &Value) -> Result<u64, Flow> {
    match manager.thread_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("threadp"), *value],
        )),
    }
}

/// Extract a mutex id from a canonical mutex handle object.
fn expect_mutex_id(manager: &ThreadManager, value: &Value) -> Result<u64, Flow> {
    match manager.mutex_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), *value],
        )),
    }
}

/// Extract a condition variable id from a canonical handle object.
fn expect_cv_id(manager: &ThreadManager, value: &Value) -> Result<u64, Flow> {
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
/// Returns a `(thread . ID)` object.
pub(crate) fn builtin_make_thread(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let (thread_id, function) = prepare_make_thread(&mut eval.threads, &args)?;
    eval.threads
        .set_thread_current_buffer(thread_id, eval.buffers.current_buffer_id());
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
}

pub(crate) fn prepare_make_thread(
    threads: &mut ThreadManager,
    args: &[Value],
) -> Result<(u64, Value), Flow> {
    expect_args_range("make-thread", args, 1, 3)?;

    let function = args[0];
    let name = if args.len() > 1 {
        match args[1].kind() {
            ValueKind::String => Some(args[1].as_lisp_string().expect("string").clone()),
            ValueKind::Nil => None,
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), args[1]],
                ));
            }
        }
    } else {
        None
    };
    let buffer_disposition = args.get(2).copied().unwrap_or(Value::NIL);

    let thread_id = threads.create_thread(function, name);
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
    match result {
        Ok(val) => {
            threads.finish_thread(thread_id, val);
        }
        Err(Flow::Signal(ref sig)) => {
            let error_val = make_signal_binding_value(sig);
            // Internal error: recorded for thread-last-error and marks the
            // thread dead, but NOT re-raised by a later thread-join (GNU
            // record_thread_error, thread.c:753-758).
            threads.record_thread_internal_error(thread_id, error_val);
            // GNU publishes thread-last-error when the thread dies, not when
            // another thread joins it later.
            threads.record_last_error(error_val);
        }
        Err(Flow::Throw(ref thrown)) => {
            let error_val = Value::list(vec![Value::symbol("no-catch"), thrown.tag, thrown.value]);
            threads.record_thread_internal_error(thread_id, error_val);
            threads.record_last_error(error_val);
        }
        Err(Flow::ThreadBlocked(blocked)) => {
            threads.block_thread(thread_id, blocked.blocker, blocked.remaining_forms);
        }
        // kill-emacs is process-wide in GNU (Fkill_emacs exits, whichever
        // thread runs it), so it is not recorded as this thread's error — it
        // unwinds to the caller and ends the process.
        Err(flow @ Flow::Shutdown(_)) => return Err(flow),
    }

    Ok(threads
        .thread_handle(thread_id)
        .unwrap_or_else(|| tagged_object_value("thread", thread_id)))
}

/// `(thread-join THREAD)` -- wait for thread completion.
///
/// Returns the thread's result, resuming a simulated blocked thread when needed,
/// or re-signals a `thread-signal` error snapshot the GNU way.
pub(crate) fn builtin_thread_join(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-join", &args, 1)?;
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    if !ctx.threads.is_thread(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("threadp"), args[0]],
        ));
    }
    if id == ctx.threads.current_thread_id() {
        return Err(signal(
            "error",
            vec![Value::string("Cannot join current thread")],
        ));
    }
    // GNU snapshots the target thread's injected error before waiting, then
    // re-signals that snapshot even if the target catches and clears it while
    // join is waiting.
    let pending_join_signal = ctx.threads.pending_thread_signal(id);
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
    ctx.threads.join_thread(id);
    if let Some(flow) = pending_join_signal {
        return Err(flow);
    }
    Ok(ctx.threads.thread_result(id))
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
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    if !ctx.threads.is_thread(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("threadp"), args[0]],
        ));
    }
    match ctx.threads.thread_name(id) {
        Some(name) => Ok(Value::heap_string(name.clone())),
        None => Ok(Value::NIL),
    }
}

/// `(thread-live-p THREAD)` -- check if the thread is alive.
pub(crate) fn builtin_thread_live_p(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-live-p", &args, 1)?;
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    if !ctx.threads.is_thread(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("threadp"), args[0]],
        ));
    }
    Ok(Value::bool_val(ctx.threads.thread_alive_p(id)))
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
    expect_args("thread-signal", &args, 3)?;
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    if !ctx.threads.is_thread(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("threadp"), args[0]],
        ));
    }
    let error_symbol = args[1];
    let Some(error_symbol_id) = error_symbol.as_symbol_id() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), error_symbol],
        ));
    };
    let data = args[2];
    if id == ctx.threads.current_thread_id() {
        return Err(signal_with_data_id(error_symbol_id, data));
    }
    ctx.threads
        .signal_thread(id, Value::cons(error_symbol, data));
    Ok(Value::NIL)
}

/// `(current-thread)` -- return the current thread object.
pub(crate) fn builtin_current_thread(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-thread", &args, 0)?;
    let id = ctx.threads.current_thread_id();
    Ok(ctx
        .threads
        .thread_handle(id)
        .unwrap_or_else(|| tagged_object_value("thread", id)))
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
                .unwrap_or_else(|| tagged_object_value("thread", id))
        })
        .collect();
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
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    Ok(ctx.threads.thread_blocker(id).unwrap_or(Value::NIL))
}

/// `(thread-buffer-disposition THREAD)` -- return THREAD's buffer disposition.
pub(crate) fn builtin_thread_buffer_disposition(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-buffer-disposition", &args, 1)?;
    let id = expect_thread_id(&ctx.threads, &args[0])?;
    Ok(ctx
        .threads
        .thread_buffer_disposition(id)
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
    ctx.threads.set_thread_buffer_disposition(id, value);
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
        match v.kind() {
            ValueKind::String => Some(v.as_lisp_string().expect("string").clone()),
            ValueKind::Nil => None,
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), *v],
                ));
            }
        }
    } else {
        None
    };
    let id = ctx.threads.create_mutex(name);
    Ok(ctx
        .threads
        .mutex_handle(id)
        .unwrap_or_else(|| tagged_object_value("mutex", id)))
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
    if !ctx.threads.is_mutex(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), args[0]],
        ));
    }
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
    if !ctx.threads.is_mutex(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), args[0]],
        ));
    }
    if !ctx.threads.mutex_lock(id) {
        return Err(Flow::thread_blocked(args[0], Value::NIL));
    }
    Ok(Value::NIL)
}

/// `(mutex-unlock MUTEX)` -- unlock a mutex.
pub(crate) fn builtin_mutex_unlock(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutex-unlock", &args, 1)?;
    let id = expect_mutex_id(&ctx.threads, &args[0])?;
    if !ctx.threads.is_mutex(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), args[0]],
        ));
    }
    // GNU `lisp_mutex_unlock` (thread.c:259-263) signals an error when the
    // current thread does not own the mutex (including a never-locked mutex,
    // whose owner is NULL). Match that instead of silently succeeding.
    if !ctx.threads.mutex_owned_by_current_thread(id) {
        return Err(signal(
            "error",
            vec![Value::string("Cannot unlock mutex owned by another thread")],
        ));
    }
    ctx.threads.mutex_unlock(id);
    ctx.threads.wake_threads_blocked_on(args[0]);
    Ok(Value::NIL)
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
    if !ctx.threads.is_mutex(mutex_id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), args[0]],
        ));
    }
    let name = if args.len() > 1 {
        match args[1].kind() {
            ValueKind::String => Some(args[1].as_lisp_string().expect("string").clone()),
            ValueKind::Nil => None,
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), args[1]],
                ));
            }
        }
    } else {
        None
    };
    match ctx.threads.create_condition_variable(mutex_id, name) {
        Some(id) => Ok(ctx
            .threads
            .condition_variable_handle(id)
            .unwrap_or_else(|| tagged_object_value("condition-variable", id))),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("mutexp"), args[0]],
        )),
    }
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
    if !ctx.threads.is_condition_variable(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        ));
    }
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
    if !ctx.threads.is_condition_variable(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        ));
    }
    let Some(mutex_id) = ctx.threads.condition_variable_mutex(id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        ));
    };
    ctx.threads.mutex_handle(mutex_id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        )
    })
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
    if !ctx.threads.is_condition_variable(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        ));
    }
    let Some(mutex_id) = ctx.threads.condition_variable_mutex(id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        ));
    };
    if !ctx.threads.mutex_owned_by_current_thread(mutex_id) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Condition variable's mutex is not held by current thread",
            )],
        ));
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
    if !ctx.threads.is_condition_variable(id) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        ));
    }
    let Some(mutex_id) = ctx.threads.condition_variable_mutex(id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("condition-variable-p"), args[0]],
        ));
    };
    if !ctx.threads.mutex_owned_by_current_thread(mutex_id) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Condition variable's mutex is not held by current thread",
            )],
        ));
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
#[path = "tests/mod.rs"]
mod tests;
