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

use std::collections::HashMap;

use super::error::{make_signal_binding_value, signal, signal_with_data, EvalResult, Flow};
use super::value::{eq_value, Value};

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
    pub name: Option<String>,
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
}

// ---------------------------------------------------------------------------
// Mutex state
// ---------------------------------------------------------------------------

/// A cooperative mutex.  Since the VM is single-threaded, lock/unlock are
/// effectively no-ops, but we still track ownership for diagnostics and to
/// match the Emacs API.
#[derive(Clone, Debug)]
pub struct MutexState {
    pub id: u64,
    pub name: Option<String>,
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
    pub name: Option<String>,
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
                function: Value::Nil,
                status: ThreadStatus::Running,
                result: Value::Nil,
                last_error: None,
                joined: false,
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
    pub fn create_thread(&mut self, function: Value, name: Option<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.threads.insert(
            id,
            ThreadState {
                id,
                name,
                function,
                status: ThreadStatus::Created,
                result: Value::Nil,
                last_error: None,
                joined: false,
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

    /// Mark a thread as Finished with the given result.
    pub fn finish_thread(&mut self, id: u64, result: Value) {
        if let Some(t) = self.threads.get_mut(&id) {
            t.status = ThreadStatus::Finished;
            t.result = result;
        }
    }

    /// Mark a thread as Signaled with an error value.
    pub fn signal_thread(&mut self, id: u64, error: Value) {
        if let Some(t) = self.threads.get_mut(&id) {
            t.status = ThreadStatus::Signaled;
            t.last_error = Some(error.clone());
        }
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

    /// Check if a thread is alive (Created or Running).
    pub fn thread_alive_p(&self, id: u64) -> bool {
        self.threads
            .get(&id)
            .is_some_and(|t| t.status == ThreadStatus::Created || t.status == ThreadStatus::Running)
    }

    /// Get thread name.
    pub fn thread_name(&self, id: u64) -> Option<&str> {
        self.threads.get(&id).and_then(|t| t.name.as_deref())
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
            .iter()
            .filter_map(|(id, thread)| (!thread.joined).then_some(*id))
            .collect()
    }

    /// Return thread result (for join).
    pub fn thread_result(&self, id: u64) -> Value {
        self.threads
            .get(&id)
            .map(|t| t.result.clone())
            .unwrap_or(Value::Nil)
    }

    /// Mark a thread as joined. Returns its terminal error only on first join.
    pub fn join_thread(&mut self, id: u64) -> Option<Value> {
        let thread = self.threads.get_mut(&id)?;
        if thread.joined {
            return None;
        }
        thread.joined = true;
        thread.last_error.clone()
    }

    /// Get and optionally clear the global last-error.
    pub fn last_error(&mut self, cleanup: bool) -> Value {
        let val = self.last_error.clone().unwrap_or(Value::Nil);
        if cleanup {
            self.last_error = None;
        }
        val
    }

    // -- Mutex operations ---------------------------------------------------

    /// Create a new mutex.  Returns the id.
    pub fn create_mutex(&mut self, name: Option<String>) -> u64 {
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
        self.mutex_handles.insert(id, tagged_object_value("mutex", id));
        id
    }

    /// Lock a mutex (on behalf of the current thread).
    /// In single-threaded mode this always succeeds.
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
                Some(_) => {
                    // In a real multi-threaded implementation this would block.
                    // In our single-threaded sim it should not happen, but we
                    // handle it gracefully by allowing the lock anyway.
                    m.owner = Some(current);
                    m.lock_count = 1;
                    true
                }
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
                // Not locked by current thread — still succeed for compatibility
                true
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
    pub fn mutex_name(&self, id: u64) -> Option<&str> {
        self.mutexes.get(&id).and_then(|m| m.name.as_deref())
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
        name: Option<String>,
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
    pub fn condition_variable_name(&self, id: u64) -> Option<&str> {
        self.condition_vars
            .get(&id)
            .and_then(|cv| cv.name.as_deref())
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

// ===========================================================================
// Argument helpers
// ===========================================================================

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn tagged_object_value(tag: &str, id: u64) -> Value {
    Value::cons(Value::symbol(tag), Value::Int(id as i64))
}

fn tagged_object_id(value: &Value, expected_tag: &str) -> Option<u64> {
    let Value::Cons(cell) = value else {
        return None;
    };
    let pair = cell.lock().expect("poisoned");
    if pair.car.as_symbol_name() != Some(expected_tag) {
        return None;
    }
    match pair.cdr {
        Value::Int(n) if n >= 0 => Some(n as u64),
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

/// Extract a thread id from a canonical thread handle object.
fn expect_thread_id(manager: &ThreadManager, value: &Value) -> Result<u64, Flow> {
    match manager.thread_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("threadp"), value.clone()],
        )),
    }
}

/// Extract a mutex id from a canonical mutex handle object.
fn expect_mutex_id(manager: &ThreadManager, value: &Value) -> Result<u64, Flow> {
    match manager.mutex_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("mutexp"), value.clone()],
        )),
    }
}

/// Extract a condition variable id from a canonical handle object.
fn expect_cv_id(manager: &ThreadManager, value: &Value) -> Result<u64, Flow> {
    match manager.condition_variable_id_from_handle(value) {
        Some(id) => Ok(id),
        None => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("condition-variable-p"), value.clone()],
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
pub(crate) fn builtin_make_thread(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("make-thread", &args, 1)?;

    let function = args[0].clone();

    let name = if args.len() > 1 {
        match &args[1] {
            Value::Str(s) => Some((**s).clone()),
            Value::Nil => None,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other.clone()],
                ));
            }
        }
    } else {
        None
    };

    let thread_id = eval.threads.create_thread(function.clone(), name);
    eval.threads.start_thread(thread_id);

    // Run the function immediately (cooperative simulation).
    let saved_current = eval.threads.current_thread;
    eval.threads.current_thread = thread_id;

    let result = eval.apply(function, vec![]);

    eval.threads.current_thread = saved_current;

    match result {
        Ok(val) => {
            eval.threads.finish_thread(thread_id, val);
        }
        Err(Flow::Signal(ref sig)) => {
            let error_val = make_signal_binding_value(sig);
            eval.threads.signal_thread(thread_id, error_val);
        }
        Err(Flow::Throw { ref tag, ref value }) => {
            let error_val =
                Value::list(vec![Value::symbol("no-catch"), tag.clone(), value.clone()]);
            eval.threads.signal_thread(thread_id, error_val);
        }
    }

    Ok(eval
        .threads
        .thread_handle(thread_id)
        .unwrap_or_else(|| tagged_object_value("thread", thread_id)))
}

/// `(thread-join THREAD)` -- wait for thread completion.
///
/// Since all threads run synchronously at creation time, they are already
/// finished by the time anyone can call join.  Returns the thread's result.
pub(crate) fn builtin_thread_join(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-join", &args, 1)?;
    let id = expect_thread_id(&eval.threads, &args[0])?;
    if !eval.threads.is_thread(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("threadp"), args[0].clone()],
        ));
    }
    if id == eval.threads.current_thread_id() {
        return Err(signal(
            "error",
            vec![Value::string("Cannot join current thread")],
        ));
    }
    if let Some(error) = eval.threads.join_thread(id) {
        // Emacs publishes joined-thread terminal errors through thread-last-error.
        eval.threads.record_last_error(error);
    }
    Ok(eval.threads.thread_result(id))
}

/// `(thread-yield)` -- yield the current thread.
///
/// No-op in our single-threaded simulation.
pub(crate) fn builtin_thread_yield(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-yield", &args, 0)?;
    Ok(Value::Nil)
}

/// `(thread-name THREAD)` -- return the thread's name or nil.
pub(crate) fn builtin_thread_name(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-name", &args, 1)?;
    let id = expect_thread_id(&eval.threads, &args[0])?;
    if !eval.threads.is_thread(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("threadp"), args[0].clone()],
        ));
    }
    match eval.threads.thread_name(id) {
        Some(name) => Ok(Value::string(name)),
        None => Ok(Value::Nil),
    }
}

/// `(thread-live-p THREAD)` -- check if the thread is alive.
pub(crate) fn builtin_thread_live_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-live-p", &args, 1)?;
    let id = expect_thread_id(&eval.threads, &args[0])?;
    if !eval.threads.is_thread(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("threadp"), args[0].clone()],
        ));
    }
    Ok(Value::bool(eval.threads.thread_alive_p(id)))
}

/// `(threadp OBJ)` -- type predicate.
///
/// Returns t if OBJ is a known thread object.
pub(crate) fn builtin_threadp(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("threadp", &args, 1)?;
    Ok(Value::bool(
        eval.threads.thread_id_from_handle(&args[0]).is_some(),
    ))
}

/// `(thread-signal THREAD ERROR-SYMBOL DATA)` -- send a signal to a thread.
///
/// In our simulation this simply records the error on the target thread.
pub(crate) fn builtin_thread_signal(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("thread-signal", &args, 3)?;
    let id = expect_thread_id(&eval.threads, &args[0])?;
    if !eval.threads.is_thread(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("threadp"), args[0].clone()],
        ));
    }
    let error_symbol = args[1].clone();
    let Some(error_name) = error_symbol.as_symbol_name() else {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), error_symbol],
        ));
    };
    let data = args[2].clone();
    if id == eval.threads.current_thread_id() {
        return Err(signal_with_data(error_name, data));
    }
    Ok(Value::Nil)
}

/// `(current-thread)` -- return the current thread object.
pub(crate) fn builtin_current_thread(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-thread", &args, 0)?;
    let id = eval.threads.current_thread_id();
    Ok(eval
        .threads
        .thread_handle(id)
        .unwrap_or_else(|| tagged_object_value("thread", id)))
}

/// `(all-threads)` -- return a list of all thread objects.
pub(crate) fn builtin_all_threads(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("all-threads", &args, 0)?;
    let mut ids = eval.threads.all_thread_ids();
    ids.sort_unstable();
    let objects: Vec<Value> = ids
        .into_iter()
        .map(|id| {
            eval.threads
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
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("thread-last-error"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let cleanup = args.first().is_some_and(|v| v.is_truthy());
    Ok(eval.threads.last_error(cleanup))
}

// ===========================================================================
// Mutex builtins
// ===========================================================================

/// `(make-mutex &optional NAME)` -- create a mutex.
pub(crate) fn builtin_make_mutex(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("make-mutex"), Value::Int(args.len() as i64)],
        ));
    }
    let name = if let Some(v) = args.first() {
        match v {
            Value::Str(s) => Some((**s).clone()),
            Value::Nil => None,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other.clone()],
                ));
            }
        }
    } else {
        None
    };
    let id = eval.threads.create_mutex(name);
    Ok(eval
        .threads
        .mutex_handle(id)
        .unwrap_or_else(|| tagged_object_value("mutex", id)))
}

/// `(mutexp OBJ)` -- type predicate for mutexes.
pub(crate) fn builtin_mutexp(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutexp", &args, 1)?;
    Ok(Value::bool(eval.threads.mutex_id_from_handle(&args[0]).is_some()))
}

/// `(mutex-name MUTEX)` -- return the mutex's name or nil.
pub(crate) fn builtin_mutex_name(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutex-name", &args, 1)?;
    let id = expect_mutex_id(&eval.threads, &args[0])?;
    if !eval.threads.is_mutex(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("mutexp"), args[0].clone()],
        ));
    }
    match eval.threads.mutex_name(id) {
        Some(name) => Ok(Value::string(name)),
        None => Ok(Value::Nil),
    }
}

/// `(mutex-lock MUTEX)` -- lock a mutex.
///
/// In single-threaded mode, this always succeeds immediately.
pub(crate) fn builtin_mutex_lock(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutex-lock", &args, 1)?;
    let id = expect_mutex_id(&eval.threads, &args[0])?;
    if !eval.threads.is_mutex(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("mutexp"), args[0].clone()],
        ));
    }
    eval.threads.mutex_lock(id);
    Ok(Value::Nil)
}

/// `(mutex-unlock MUTEX)` -- unlock a mutex.
pub(crate) fn builtin_mutex_unlock(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mutex-unlock", &args, 1)?;
    let id = expect_mutex_id(&eval.threads, &args[0])?;
    if !eval.threads.is_mutex(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("mutexp"), args[0].clone()],
        ));
    }
    eval.threads.mutex_unlock(id);
    Ok(Value::Nil)
}

// ===========================================================================
// Condition variable builtins
// ===========================================================================

/// `(make-condition-variable MUTEX &optional NAME)` -- create a condition variable.
pub(crate) fn builtin_make_condition_variable(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("make-condition-variable", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-condition-variable"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let mutex_id = expect_mutex_id(&eval.threads, &args[0])?;
    if !eval.threads.is_mutex(mutex_id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("mutexp"), args[0].clone()],
        ));
    }
    let name = if args.len() > 1 {
        match &args[1] {
            Value::Str(s) => Some((**s).clone()),
            Value::Nil => None,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other.clone()],
                ));
            }
        }
    } else {
        None
    };
    match eval.threads.create_condition_variable(mutex_id, name) {
        Some(id) => Ok(eval
            .threads
            .condition_variable_handle(id)
            .unwrap_or_else(|| tagged_object_value("condition-variable", id))),
        None => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("mutexp"), args[0].clone()],
        )),
    }
}

/// `(condition-variable-p OBJ)` -- type predicate.
pub(crate) fn builtin_condition_variable_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-variable-p", &args, 1)?;
    Ok(Value::bool(
        eval.threads
            .condition_variable_id_from_handle(&args[0])
            .is_some(),
    ))
}

/// `(condition-wait COND)` -- wait on a condition variable.
///
/// In single-threaded mode this is a no-op.
pub(crate) fn builtin_condition_wait(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("condition-wait", &args, 1)?;
    let id = expect_cv_id(&eval.threads, &args[0])?;
    if !eval.threads.is_condition_variable(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("condition-variable-p"), args[0].clone()],
        ));
    }
    let Some(mutex_id) = eval.threads.condition_variable_mutex(id) else {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("condition-variable-p"), args[0].clone()],
        ));
    };
    if !eval.threads.mutex_owned_by_current_thread(mutex_id) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Condition variable’s mutex is not held by current thread",
            )],
        ));
    }
    // No-op: in a single-threaded VM, waiting would deadlock.
    // Return nil for compatibility.
    Ok(Value::Nil)
}

/// `(condition-notify COND &optional ALL)` -- notify on a condition variable.
///
/// No-op in single-threaded mode.
pub(crate) fn builtin_condition_notify(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("condition-notify", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("condition-notify"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let id = expect_cv_id(&eval.threads, &args[0])?;
    if !eval.threads.is_condition_variable(id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("condition-variable-p"), args[0].clone()],
        ));
    }
    let Some(mutex_id) = eval.threads.condition_variable_mutex(id) else {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("condition-variable-p"), args[0].clone()],
        ));
    };
    if !eval.threads.mutex_owned_by_current_thread(mutex_id) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Condition variable’s mutex is not held by current thread",
            )],
        ));
    }
    // No-op
    Ok(Value::Nil)
}

// ===========================================================================
// Special form: with-mutex
// ===========================================================================

/// `(with-mutex MUTEX BODY...)` -- execute BODY with MUTEX locked.
///
/// This is a special form: MUTEX is evaluated, the lock is acquired, BODY is
/// executed as an implicit progn, and the lock is released on exit
/// (even if BODY signals an error).
pub(crate) fn sf_with_mutex(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> EvalResult {
    if tail.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::cons(Value::Int(1), Value::Int(1)), Value::Int(0)],
        ));
    }
    let mutex_val = eval.eval(&tail[0])?;
    let mutex_id = expect_mutex_id(&eval.threads, &mutex_val)?;
    if !eval.threads.is_mutex(mutex_id) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("mutexp"), mutex_val],
        ));
    }

    eval.threads.mutex_lock(mutex_id);
    let result = eval.sf_progn(&tail[1..]);
    eval.threads.mutex_unlock(mutex_id);
    result
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::super::eval::Evaluator;
    use super::*;

    // -- ThreadManager unit tests -------------------------------------------

    #[test]
    fn thread_manager_new_has_main_thread() {
        let mgr = ThreadManager::new();
        assert!(mgr.is_thread(0));
        assert_eq!(mgr.current_thread_id(), 0);
        assert!(mgr.thread_alive_p(0));
        assert_eq!(mgr.thread_name(0), None);
    }

    #[test]
    fn create_thread_assigns_unique_ids() {
        let mut mgr = ThreadManager::new();
        let id1 = mgr.create_thread(Value::Nil, Some("t1".into()));
        let id2 = mgr.create_thread(Value::Nil, Some("t2".into()));
        assert_ne!(id1, id2);
        assert!(mgr.is_thread(id1));
        assert!(mgr.is_thread(id2));
        assert!(!mgr.is_thread(999));
    }

    #[test]
    fn thread_lifecycle_created_running_finished() {
        let mut mgr = ThreadManager::new();
        let id = mgr.create_thread(Value::Nil, None);
        assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Created);
        assert!(mgr.thread_alive_p(id));

        mgr.start_thread(id);
        assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Running);
        assert!(mgr.thread_alive_p(id));

        mgr.finish_thread(id, Value::Int(42));
        assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Finished);
        assert!(!mgr.thread_alive_p(id));
        assert_eq!(mgr.thread_result(id).as_int(), Some(42));
    }

    #[test]
    fn thread_signal_records_error() {
        let mut mgr = ThreadManager::new();
        let id = mgr.create_thread(Value::Nil, None);
        mgr.start_thread(id);
        mgr.signal_thread(id, Value::symbol("test-error"));
        assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Signaled);
        assert!(!mgr.thread_alive_p(id));
    }

    #[test]
    fn all_thread_ids_includes_main_and_created() {
        let mut mgr = ThreadManager::new();
        let id = mgr.create_thread(Value::Nil, None);
        let ids = mgr.all_thread_ids();
        assert!(ids.len() >= 2);
        assert!(ids.contains(&0));
        assert!(ids.contains(&id));
    }

    #[test]
    fn all_thread_ids_excludes_joined_thread() {
        let mut mgr = ThreadManager::new();
        let id = mgr.create_thread(Value::Nil, None);
        let before_join = mgr.all_thread_ids();
        assert!(before_join.contains(&id));

        let _ = mgr.join_thread(id);
        let after_join = mgr.all_thread_ids();
        assert!(!after_join.contains(&id));
        assert!(after_join.contains(&0));
    }

    #[test]
    fn last_error_get_and_cleanup() {
        let mut mgr = ThreadManager::new();
        mgr.record_last_error(Value::symbol("oops"));

        let err = mgr.last_error(false);
        assert!(err.is_truthy());
        // Still there after no cleanup
        let err2 = mgr.last_error(true);
        assert!(err2.is_truthy());
        // Now gone
        let err3 = mgr.last_error(false);
        assert!(err3.is_nil());
    }

    // -- Mutex unit tests ---------------------------------------------------

    #[test]
    fn mutex_create_and_lookup() {
        let mut mgr = ThreadManager::new();
        let id = mgr.create_mutex(Some("my-lock".into()));
        assert!(mgr.is_mutex(id));
        assert_eq!(mgr.mutex_name(id), Some("my-lock"));
        assert!(!mgr.is_mutex(999));
    }

    #[test]
    fn mutex_lock_unlock_cycle() {
        let mut mgr = ThreadManager::new();
        let id = mgr.create_mutex(None);
        assert!(mgr.mutex_lock(id));
        assert!(mgr.mutex_unlock(id));
        // Unlocking when not locked is fine
        assert!(mgr.mutex_unlock(id));
    }

    #[test]
    fn mutex_recursive_lock() {
        let mut mgr = ThreadManager::new();
        let id = mgr.create_mutex(None);
        assert!(mgr.mutex_lock(id));
        assert!(mgr.mutex_lock(id));
        // lock_count is 2
        assert!(mgr.mutex_unlock(id));
        // Still locked (count=1)
        let m = mgr.mutexes.get(&id).unwrap();
        assert!(m.owner.is_some());
        assert!(mgr.mutex_unlock(id));
        // Now fully unlocked
        let m = mgr.mutexes.get(&id).unwrap();
        assert!(m.owner.is_none());
    }

    // -- Condition variable unit tests --------------------------------------

    #[test]
    fn condition_variable_create() {
        let mut mgr = ThreadManager::new();
        let mx = mgr.create_mutex(None);
        let cv = mgr.create_condition_variable(mx, Some("cv1".into()));
        assert!(cv.is_some());
        let cv_id = cv.unwrap();
        assert!(mgr.is_condition_variable(cv_id));
        assert_eq!(mgr.condition_variable_name(cv_id), Some("cv1"));
        assert_eq!(mgr.condition_variable_mutex(cv_id), Some(mx));
    }

    #[test]
    fn condition_variable_requires_valid_mutex() {
        let mut mgr = ThreadManager::new();
        let cv = mgr.create_condition_variable(999, None);
        assert!(cv.is_none());
    }

    // -- Builtin-level tests -----------------------------------------------

    #[test]
    fn test_builtin_make_thread_runs_function() {
        let mut eval = Evaluator::new();
        // Define a simple function that returns 42
        eval.set_variable("thread-test-result", Value::Nil);
        eval.set_function(
            "thread-test-fn",
            Value::Lambda(std::sync::Arc::new(super::super::value::LambdaData {
                params: super::super::value::LambdaParams::simple(vec![]),
                body: vec![], // empty body → nil
                env: None,
                docstring: None,
            })),
        );

        let result = builtin_make_thread(
            &mut eval,
            vec![Value::symbol("thread-test-fn"), Value::string("worker")],
        );
        assert!(result.is_ok());
        let tid = result.unwrap();
        assert_eq!(tagged_object_id(&tid, "thread"), Some(1));
    }

    #[test]
    fn test_builtin_threadp() {
        let mut eval = Evaluator::new();
        let current = builtin_current_thread(&mut eval, vec![]).unwrap();

        let r = builtin_threadp(&mut eval, vec![current]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_truthy());

        let r = builtin_threadp(&mut eval, vec![Value::Int(0)]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());

        let r = builtin_threadp(&mut eval, vec![Value::string("nope")]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());

        let fake = Value::cons(Value::symbol("thread"), Value::Int(999));
        let r = builtin_threadp(&mut eval, vec![fake]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());

        let forged_main = Value::cons(Value::symbol("thread"), Value::Int(0));
        let r = builtin_threadp(&mut eval, vec![forged_main]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_current_thread() {
        let mut eval = Evaluator::new();
        let result = builtin_current_thread(&mut eval, vec![]);
        assert!(result.is_ok());
        assert_eq!(tagged_object_id(&result.unwrap(), "thread"), Some(0));
    }

    #[test]
    fn test_builtin_current_thread_returns_stable_handle_identity() {
        let mut eval = Evaluator::new();
        let first = builtin_current_thread(&mut eval, vec![]).unwrap();
        let second = builtin_current_thread(&mut eval, vec![]).unwrap();
        assert!(eq_value(&first, &second));
    }

    #[test]
    fn test_builtin_thread_yield() {
        let mut eval = Evaluator::new();
        let result = builtin_thread_yield(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_thread_name_main() {
        let mut eval = Evaluator::new();
        let current = builtin_current_thread(&mut eval, vec![]).unwrap();
        let result = builtin_thread_name(&mut eval, vec![current]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_thread_live_p_main() {
        let mut eval = Evaluator::new();
        let current = builtin_current_thread(&mut eval, vec![]).unwrap();
        let result = builtin_thread_live_p(&mut eval, vec![current]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn test_builtin_all_threads_includes_main() {
        let mut eval = Evaluator::new();
        let result = builtin_all_threads(&mut eval, vec![]);
        assert!(result.is_ok());
        let list = super::super::value::list_to_vec(&result.unwrap()).unwrap();
        assert!(!list.is_empty());
        assert!(
            list.iter()
                .any(|v| tagged_object_id(v, "thread") == Some(0))
        );
    }

    #[test]
    fn test_builtin_thread_join_finished() {
        let mut eval = Evaluator::new();
        // Create and run a thread
        let tid_val = builtin_make_thread(
            &mut eval,
            vec![Value::Lambda(std::sync::Arc::new(
                super::super::value::LambdaData {
                    params: super::super::value::LambdaParams::simple(vec![]),
                    body: vec![],
                    env: None,
                    docstring: None,
                },
            ))],
        )
        .unwrap();

        // Join it
        let result = builtin_thread_join(&mut eval, vec![tid_val]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_builtin_thread_join_current_thread_errors() {
        let mut eval = Evaluator::new();
        let current = builtin_current_thread(&mut eval, vec![]).unwrap();
        let result = builtin_thread_join(&mut eval, vec![current]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(sig.data.len(), 1);
                assert_eq!(sig.data[0].as_str(), Some("Cannot join current thread"));
            }
            other => panic!("expected error signal for self-join, got {other:?}"),
        }
    }

    #[test]
    fn test_builtin_thread_signal_non_current_is_noop() {
        let mut eval = Evaluator::new();
        let tid_val = builtin_make_thread(
            &mut eval,
            vec![Value::Lambda(std::sync::Arc::new(
                super::super::value::LambdaData {
                    params: super::super::value::LambdaParams::simple(vec![]),
                    body: vec![],
                    env: None,
                    docstring: None,
                },
            ))],
        )
        .unwrap();

        let result = builtin_thread_signal(
            &mut eval,
            vec![tid_val, Value::symbol("test-error"), Value::string("oops")],
        );
        assert!(result.is_ok());

        // Signaling an already-finished non-current thread does not set global last-error.
        let err = builtin_thread_last_error(&mut eval, vec![]);
        assert!(err.is_ok());
        assert!(err.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_thread_signal_current_thread_raises() {
        let mut eval = Evaluator::new();
        let current = builtin_current_thread(&mut eval, vec![]).unwrap();
        let result = builtin_thread_signal(
            &mut eval,
            vec![current, Value::symbol("foo"), Value::Int(1)],
        );
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "foo");
                assert_eq!(sig.raw_data, Some(Value::Int(1)));
            }
            other => panic!("expected signal from thread-signal current thread, got {other:?}"),
        }
    }

    #[test]
    fn test_builtin_thread_last_error_cleanup() {
        let mut eval = Evaluator::new();
        eval.threads
            .record_last_error(Value::list(vec![Value::symbol("err"), Value::Int(1)]));

        let e1 = builtin_thread_last_error(&mut eval, vec![Value::Nil]).unwrap();
        assert!(e1.is_truthy());

        // Cleanup
        let e2 = builtin_thread_last_error(&mut eval, vec![Value::True]).unwrap();
        assert!(e2.is_truthy());

        // Should be gone now
        let e3 = builtin_thread_last_error(&mut eval, vec![]).unwrap();
        assert!(e3.is_nil());
    }

    // -- Mutex builtin tests ------------------------------------------------

    #[test]
    fn test_builtin_make_mutex() {
        let mut eval = Evaluator::new();
        let result = builtin_make_mutex(&mut eval, vec![Value::string("my-mutex")]);
        assert!(result.is_ok());
        let mx = result.unwrap();
        assert_eq!(tagged_object_id(&mx, "mutex"), Some(1));
    }

    #[test]
    fn test_builtin_mutexp() {
        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();

        let r = builtin_mutexp(&mut eval, vec![mx]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_truthy());

        let r = builtin_mutexp(&mut eval, vec![Value::Int(1)]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());

        let r = builtin_mutexp(&mut eval, vec![Value::Nil]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());

        let forged = Value::cons(Value::symbol("mutex"), Value::Int(1));
        let r = builtin_mutexp(&mut eval, vec![forged]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_mutex_name() {
        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![Value::string("named-mx")]).unwrap();
        let result = builtin_mutex_name(&mut eval, vec![mx]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), Some("named-mx"));
    }

    #[test]
    fn test_builtin_mutex_lock_unlock() {
        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
        let lock_result = builtin_mutex_lock(&mut eval, vec![mx.clone()]);
        assert!(lock_result.is_ok());
        let unlock_result = builtin_mutex_unlock(&mut eval, vec![mx]);
        assert!(unlock_result.is_ok());
    }

    // -- Condition variable builtin tests -----------------------------------

    #[test]
    fn test_builtin_make_condition_variable() {
        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
        let result = builtin_make_condition_variable(&mut eval, vec![mx, Value::string("my-cv")]);
        assert!(result.is_ok());
        assert!(tagged_object_id(&result.unwrap(), "condition-variable").is_some());
    }

    #[test]
    fn test_builtin_condition_variable_p() {
        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
        let cv = builtin_make_condition_variable(&mut eval, vec![mx]).unwrap();

        let r = builtin_condition_variable_p(&mut eval, vec![cv]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_truthy());

        let r = builtin_condition_variable_p(&mut eval, vec![Value::Int(1)]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());

        let r = builtin_condition_variable_p(&mut eval, vec![Value::Nil]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());

        let forged = Value::cons(Value::symbol("condition-variable"), Value::Int(1));
        let r = builtin_condition_variable_p(&mut eval, vec![forged]);
        assert!(r.is_ok());
        assert!(r.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_condition_wait_noop() {
        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
        let cv = builtin_make_condition_variable(&mut eval, vec![mx.clone()]).unwrap();
        let owner_error = builtin_condition_wait(&mut eval, vec![cv.clone()]);
        assert!(owner_error.is_err());
        let lock = builtin_mutex_lock(&mut eval, vec![mx.clone()]);
        assert!(lock.is_ok());
        let result = builtin_condition_wait(&mut eval, vec![cv]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
        let unlock = builtin_mutex_unlock(&mut eval, vec![mx]);
        assert!(unlock.is_ok());
    }

    #[test]
    fn test_builtin_condition_notify_noop() {
        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
        let cv = builtin_make_condition_variable(&mut eval, vec![mx.clone()]).unwrap();
        let owner_error = builtin_condition_notify(&mut eval, vec![cv.clone()]);
        assert!(owner_error.is_err());
        let lock = builtin_mutex_lock(&mut eval, vec![mx.clone()]);
        assert!(lock.is_ok());
        let result = builtin_condition_notify(&mut eval, vec![cv]);
        assert!(result.is_ok());
        let unlock = builtin_mutex_unlock(&mut eval, vec![mx]);
        assert!(unlock.is_ok());
    }

    // -- with-mutex special form tests --------------------------------------

    #[test]
    fn test_sf_with_mutex_executes_body() {
        use super::super::expr::Expr;

        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
        let mx_id = tagged_object_id(&mx, "mutex").unwrap();

        // Store the mutex id in a variable so the special form can look it up
        eval.set_variable("test-mx", mx);

        // (with-mutex test-mx 42)
        let tail = vec![Expr::Symbol("test-mx".into()), Expr::Int(42)];
        let result = sf_with_mutex(&mut eval, &tail);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_int(), Some(42));

        // Mutex should be unlocked after with-mutex completes
        let m = eval.threads.mutexes.get(&mx_id).unwrap();
        assert!(m.owner.is_none());
    }

    #[test]
    fn test_sf_with_mutex_unlocks_on_error() {
        use super::super::expr::Expr;

        let mut eval = Evaluator::new();
        let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
        let mx_id = tagged_object_id(&mx, "mutex").unwrap();
        eval.set_variable("test-mx2", mx);

        // (with-mutex test-mx2 (/ 1 0))  -- will signal arith-error
        let tail = vec![
            Expr::Symbol("test-mx2".into()),
            Expr::List(vec![Expr::Symbol("/".into()), Expr::Int(1), Expr::Int(0)]),
        ];
        let result = sf_with_mutex(&mut eval, &tail);
        // Should propagate the error
        assert!(result.is_err());

        // But the mutex should still be unlocked
        let m = eval.threads.mutexes.get(&mx_id).unwrap();
        assert!(m.owner.is_none());
    }

    #[test]
    fn test_sf_with_mutex_wrong_args() {
        let mut eval = Evaluator::new();
        // No arguments at all
        let result = sf_with_mutex(&mut eval, &[]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-number-of-arguments");
                assert_eq!(
                    sig.data,
                    vec![Value::cons(Value::Int(1), Value::Int(1)), Value::Int(0)]
                );
            }
            other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
        }
    }

    // -- Arity / type error tests -------------------------------------------

    #[test]
    fn test_thread_yield_wrong_args() {
        let mut eval = Evaluator::new();
        let result = builtin_thread_yield(&mut eval, vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_current_thread_wrong_args() {
        let mut eval = Evaluator::new();
        let result = builtin_current_thread(&mut eval, vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_make_thread_non_callable_returns_thread_object() {
        let mut eval = Evaluator::new();
        let result = builtin_make_thread(&mut eval, vec![Value::Int(42)]).unwrap();
        let is_thread = builtin_threadp(&mut eval, vec![result]).unwrap();
        assert!(is_thread.is_truthy());
    }

    #[test]
    fn test_make_thread_non_callable_last_error_shape() {
        let mut eval = Evaluator::new();
        let thread = builtin_make_thread(&mut eval, vec![Value::Int(1)]).unwrap();
        let _ = builtin_thread_join(&mut eval, vec![thread]).unwrap();
        let err = builtin_thread_last_error(&mut eval, vec![]).unwrap();
        assert_eq!(super::super::print::print_value(&err), "(invalid-function 1)");
    }

    #[test]
    fn test_thread_last_error_is_published_when_joining_signaled_thread() {
        let mut eval = Evaluator::new();
        let _ = builtin_thread_last_error(&mut eval, vec![Value::True]).unwrap();

        let thread = builtin_make_thread(&mut eval, vec![Value::symbol("car")]).unwrap();
        let before_join = builtin_thread_last_error(&mut eval, vec![]).unwrap();
        assert!(before_join.is_nil());

        let _ = builtin_thread_join(&mut eval, vec![thread.clone()]).unwrap();
        let after_join = builtin_thread_last_error(&mut eval, vec![]).unwrap();
        assert_eq!(
            super::super::print::print_value(&after_join),
            "(wrong-number-of-arguments #<subr car> 0)"
        );

        let _ = builtin_thread_last_error(&mut eval, vec![Value::True]).unwrap();
        let _ = builtin_thread_join(&mut eval, vec![thread]).unwrap();
        let republished = builtin_thread_last_error(&mut eval, vec![]).unwrap();
        assert!(republished.is_nil());
    }

    #[test]
    fn test_thread_name_nonexistent() {
        let mut eval = Evaluator::new();
        let fake = Value::cons(Value::symbol("thread"), Value::Int(999));
        let result = builtin_thread_name(&mut eval, vec![fake]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mutex_lock_nonexistent() {
        let mut eval = Evaluator::new();
        let fake = Value::cons(Value::symbol("mutex"), Value::Int(999));
        let result = builtin_mutex_lock(&mut eval, vec![fake]);
        assert!(result.is_err());
    }

    #[test]
    fn test_condition_wait_nonexistent() {
        let mut eval = Evaluator::new();
        let fake = Value::cons(Value::symbol("condition-variable"), Value::Int(999));
        let result = builtin_condition_wait(&mut eval, vec![fake]);
        assert!(result.is_err());
    }
}
