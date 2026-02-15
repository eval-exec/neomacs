//! Keyboard macro support -- recording, playback, and macro management.
//!
//! Provides Emacs-compatible keyboard macro functionality:
//! - `start-kbd-macro` / `end-kbd-macro` -- record key sequences
//! - `call-last-kbd-macro` -- replay the last recorded macro
//! - `execute-kbd-macro` -- execute a macro N times
//! - `name-last-kbd-macro` -- bind a macro to a symbol
//! - `insert-kbd-macro` -- insert macro definition as Lisp text
//! - `kbd-macro-query` -- interactive query during playback
//! - `store-kbd-macro-event` -- add event to current recording
//! - `kmacro-set-counter` / `kmacro-add-counter` / `kmacro-set-format` -- counter ops
//! - `executing-kbd-macro-p` / `defining-kbd-macro-p` -- predicates
//! - `last-kbd-macro` -- retrieve last macro value
//! - `kmacro-p` -- predicate for macro values

use super::error::{signal, EvalResult, Flow};
use super::value::*;

// ---------------------------------------------------------------------------
// Argument helpers (local copies, matching builtins.rs convention)
// ---------------------------------------------------------------------------

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

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

fn expect_string(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// KmacroManager
// ---------------------------------------------------------------------------

/// Central manager for keyboard macro state.
#[derive(Clone, Debug)]
pub struct KmacroManager {
    /// Whether we are currently recording a keyboard macro.
    pub recording: bool,
    /// Key events accumulated during the current recording session.
    pub current_macro: Vec<Value>,
    /// The last completed macro (after `end-kbd-macro`).
    pub last_macro: Option<Vec<Value>>,
    /// Ring of previously saved macros (most recent first).
    pub macro_ring: Vec<Vec<Value>>,
    /// Keyboard macro counter (for `kmacro-insert-counter`).
    pub counter: i64,
    /// Format string for the counter (printf-style, default "%d").
    pub counter_format: String,
}

impl Default for KmacroManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KmacroManager {
    /// Create a new manager with default state.
    pub fn new() -> Self {
        Self {
            recording: false,
            current_macro: Vec::new(),
            last_macro: None,
            macro_ring: Vec::new(),
            counter: 0,
            counter_format: "%d".to_string(),
        }
    }

    /// Start recording a new keyboard macro.
    /// If `append` is true, append to the last macro instead of starting fresh.
    pub fn start_recording(&mut self, append: bool) {
        self.recording = true;
        if append {
            // Begin with a copy of the last macro so new keys are appended.
            self.current_macro = self.last_macro.clone().unwrap_or_default();
        } else {
            self.current_macro.clear();
        }
    }

    /// Stop recording and push the result onto the ring.
    /// Returns the recorded macro, or None if nothing was recorded.
    pub fn stop_recording(&mut self) -> Option<Vec<Value>> {
        self.recording = false;
        if self.current_macro.is_empty() {
            return None;
        }
        let recorded = self.current_macro.clone();
        // Push old last_macro onto the ring before replacing.
        if let Some(prev) = self.last_macro.take() {
            self.macro_ring.push(prev);
        }
        self.last_macro = Some(recorded.clone());
        self.current_macro.clear();
        Some(recorded)
    }

    /// Record a single event into the current macro (no-op if not recording).
    pub fn store_event(&mut self, event: Value) {
        if self.recording {
            self.current_macro.push(event);
        }
    }

    /// Format the counter using the current format string.
    pub fn format_counter(&self) -> String {
        // Support basic %d / %o / %x / %X formats.
        // For anything more complex, fall back to decimal.
        let fmt = &self.counter_format;
        if fmt.contains("%d") {
            fmt.replace("%d", &self.counter.to_string())
        } else if fmt.contains("%o") {
            fmt.replace("%o", &format!("{:o}", self.counter))
        } else if fmt.contains("%x") {
            fmt.replace("%x", &format!("{:x}", self.counter))
        } else if fmt.contains("%X") {
            fmt.replace("%X", &format!("{:X}", self.counter))
        } else {
            // Fallback: just print the number.
            self.counter.to_string()
        }
    }
}

// ===========================================================================
// Builtins (evaluator-dependent)
// ===========================================================================

/// (start-kbd-macro &optional APPEND) -> nil
///
/// Start recording a keyboard macro.  With non-nil APPEND, append to
/// the last macro instead of starting a new one.  Signals an error if
/// already recording.
pub(crate) fn builtin_start_kbd_macro(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if eval.kmacro.recording {
        return Err(signal(
            "error",
            vec![Value::string("Already defining a keyboard macro")],
        ));
    }
    let append = args.first().map_or(false, |v| v.is_truthy());
    eval.kmacro.start_recording(append);
    Ok(Value::Nil)
}

/// (end-kbd-macro &optional REPEAT LOOPFUNC) -> nil
///
/// Stop recording a keyboard macro.  Signals an error if not currently
/// recording.  The optional REPEAT argument is accepted for compatibility
/// but ignored in this implementation.
pub(crate) fn builtin_end_kbd_macro(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args; // REPEAT and LOOPFUNC accepted but unused
    if !eval.kmacro.recording {
        return Err(signal(
            "error",
            vec![Value::string("Not defining a keyboard macro")],
        ));
    }
    eval.kmacro.stop_recording();
    Ok(Value::Nil)
}

/// (call-last-kbd-macro &optional REPEAT LOOPFUNC) -> nil
///
/// Execute the last keyboard macro.  Each recorded event is passed to
/// the evaluator via `funcall` on `execute-kbd-macro-event` if defined,
/// otherwise events are evaluated directly.
pub(crate) fn builtin_call_last_kbd_macro(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let repeat = if args.is_empty() {
        1i64
    } else {
        expect_int(&args[0]).unwrap_or(1)
    };

    let macro_keys = match &eval.kmacro.last_macro {
        Some(keys) => keys.clone(),
        None => {
            return Err(signal(
                "error",
                vec![Value::string("No keyboard macro has been defined")],
            ));
        }
    };

    for _ in 0..repeat {
        for event in &macro_keys {
            // Try to call the event as a command.  If it is a symbol,
            // funcall it; otherwise treat it as a self-insert character.
            match event {
                Value::Symbol(name) => {
                    let func = Value::symbol(name.clone());
                    eval.apply(func, vec![])?;
                }
                _ => {
                    // For character events, attempt self-insert-command
                    // by looking it up; if unavailable, just ignore.
                    if let Some(func) = eval.obarray.symbol_function("self-insert-command").cloned()
                    {
                        eval.apply(func, vec![Value::Int(1)])?;
                    }
                }
            }
        }
    }
    Ok(Value::Nil)
}

/// (execute-kbd-macro MACRO &optional COUNT LOOPFUNC) -> nil
///
/// Execute MACRO (a vector, string, or symbol) COUNT times.
/// If MACRO is a symbol, its function definition is used.
pub(crate) fn builtin_execute_kbd_macro(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("execute-kbd-macro", &args, 1)?;

    let count = if args.len() >= 2 {
        expect_int(&args[1]).unwrap_or(1)
    } else {
        1
    };

    // Resolve the macro value.
    let macro_events = resolve_macro_events(&args[0])?;

    for _ in 0..count {
        for event in &macro_events {
            match event {
                Value::Symbol(name) => {
                    let func = Value::symbol(name.clone());
                    eval.apply(func, vec![])?;
                }
                _ => {
                    if let Some(func) = eval.obarray.symbol_function("self-insert-command").cloned()
                    {
                        eval.apply(func, vec![Value::Int(1)])?;
                    }
                }
            }
        }
    }
    Ok(Value::Nil)
}

/// (name-last-kbd-macro SYMBOL) -> nil
///
/// Bind the last keyboard macro to SYMBOL as its function definition.
/// Signals an error if no macro has been recorded.
pub(crate) fn builtin_name_last_kbd_macro(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("name-last-kbd-macro", &args, 1)?;

    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), other.clone()],
            ));
        }
    };

    let macro_val = match &eval.kmacro.last_macro {
        Some(keys) => Value::vector(keys.clone()),
        None => {
            return Err(signal(
                "error",
                vec![Value::string("No keyboard macro has been defined")],
            ));
        }
    };

    eval.obarray.set_symbol_function(&name, macro_val);
    Ok(Value::Nil)
}

/// (insert-kbd-macro MACRONAME &optional KEYS) -> nil
///
/// Insert the definition of keyboard macro MACRONAME as Lisp code.
/// Returns the text representation as a string (since we don't have
/// direct buffer insertion in the VM).
pub(crate) fn builtin_insert_kbd_macro(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("insert-kbd-macro", &args, 1)?;

    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), other.clone()],
            ));
        }
    };

    // Look up the function definition.
    let func = eval.obarray.symbol_function(&name).cloned();
    let macro_events = match func {
        Some(Value::Vector(v)) => {
            let items = v.lock().expect("poisoned");
            items.clone()
        }
        Some(other) => {
            // Try to resolve as a list.
            match list_to_vec(&other) {
                Some(v) => v,
                None => {
                    return Err(signal(
                        "error",
                        vec![Value::string(format!("{} is not a keyboard macro", name))],
                    ));
                }
            }
        }
        None => {
            // Fall back to last_macro if the symbol is not bound.
            match &eval.kmacro.last_macro {
                Some(keys) => keys.clone(),
                None => {
                    return Err(signal(
                        "error",
                        vec![Value::string(format!(
                            "{} is not defined as a keyboard macro",
                            name
                        ))],
                    ));
                }
            }
        }
    };

    // Build a textual representation.
    let mut parts = Vec::new();
    for ev in &macro_events {
        match ev {
            Value::Char(c) => parts.push(format!("?{}", c)),
            Value::Int(n) => parts.push(n.to_string()),
            Value::Symbol(s) => parts.push(s.clone()),
            Value::Str(s) => parts.push(format!("\"{}\"", s)),
            other => parts.push(format!("{:?}", other)),
        }
    }
    let definition = format!("(fset '{}  (vector {}))\n", name, parts.join(" "));
    Ok(Value::string(definition))
}

/// (kbd-macro-query FLAG) -> nil
///
/// Compatibility subset:
/// - arity is exactly 1
/// - when neither recording nor executing a keyboard macro, signal
///   `(user-error "Not defining or executing kbd macro")`
/// - otherwise return nil (interactive query flow is not implemented yet)
pub(crate) fn builtin_kbd_macro_query(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kbd-macro-query", &args, 1)?;
    if !eval.kmacro.recording {
        return Err(signal(
            "user-error",
            vec![Value::string("Not defining or executing kbd macro")],
        ));
    }
    Ok(Value::Nil)
}

/// (store-kbd-macro-event EVENT) -> nil
///
/// Add EVENT to the keyboard macro currently being recorded.
/// If not currently recording, this is a no-op.
pub(crate) fn builtin_store_kbd_macro_event(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("store-kbd-macro-event", &args, 1)?;
    eval.kmacro.store_event(args[0].clone());
    Ok(Value::Nil)
}

/// (kmacro-set-counter VALUE) -> nil
///
/// Set the keyboard macro counter to VALUE (an integer).
pub(crate) fn builtin_kmacro_set_counter(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kmacro-set-counter", &args, 1)?;
    let val = expect_int(&args[0])?;
    eval.kmacro.counter = val;
    Ok(Value::Nil)
}

/// (kmacro-add-counter INCREMENT) -> nil
///
/// Add INCREMENT to the keyboard macro counter.
pub(crate) fn builtin_kmacro_add_counter(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kmacro-add-counter", &args, 1)?;
    let inc = expect_int(&args[0])?;
    eval.kmacro.counter = eval.kmacro.counter.wrapping_add(inc);
    Ok(Value::Nil)
}

/// (kmacro-set-format FORMAT-STRING) -> nil
///
/// Set the keyboard macro counter format string.
pub(crate) fn builtin_kmacro_set_format(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kmacro-set-format", &args, 1)?;
    let fmt = expect_string(&args[0])?;
    eval.kmacro.counter_format = fmt;
    Ok(Value::Nil)
}

// ===========================================================================
// Pure builtins (no evaluator needed)
// ===========================================================================

/// (executing-kbd-macro-p) -> nil
///
/// Return non-nil if a keyboard macro is currently being executed.
/// Stub implementation -- always returns nil since the VM does not
/// track execution state separately from `call-last-kbd-macro`.
pub(crate) fn builtin_executing_kbd_macro_p(args: Vec<Value>) -> EvalResult {
    let _ = args;
    // In a full implementation, this would check an `executing` flag
    // on the KmacroManager.  For now, return nil.
    Ok(Value::Nil)
}

/// (defining-kbd-macro-p) -> t or nil
///
/// Return non-nil if a keyboard macro is currently being defined (recorded).
/// This is a pure builtin that peeks at the evaluator state indirectly:
/// since pure builtins can't access the evaluator, this checks a global
/// static.  As a simplified stub, it always returns nil.  The eval-dependent
/// version below is used when dispatch has evaluator access.
pub(crate) fn builtin_defining_kbd_macro_p(args: Vec<Value>) -> EvalResult {
    let _ = args;
    // Without evaluator access, we cannot check the recording flag.
    // This pure stub exists for bytecode VM dispatch compatibility.
    Ok(Value::Nil)
}

/// (kmacro-p OBJECT) -> t or nil
///
/// Return non-nil if OBJECT looks like a keyboard macro.
/// A keyboard macro is a vector, a string, or a symbol with a vector
/// function definition.
pub(crate) fn builtin_kmacro_p(args: Vec<Value>) -> EvalResult {
    expect_args("kmacro-p", &args, 1)?;
    let is_macro = match &args[0] {
        Value::Vector(_) => true,
        Value::Str(_) => true,
        _ => false,
    };
    Ok(Value::bool(is_macro))
}

// ===========================================================================
// Eval-dependent versions of predicates (richer than pure stubs)
// ===========================================================================

/// (defining-kbd-macro-p) -> t or nil  [eval-dependent version]
///
/// Returns non-nil when the evaluator is currently recording a macro.
pub(crate) fn builtin_defining_kbd_macro_p_eval(
    eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    Ok(Value::bool(eval.kmacro.recording))
}

/// (last-kbd-macro) -> vector or nil  [eval-dependent version]
///
/// Return the last keyboard macro as a vector of events, or nil.
pub(crate) fn builtin_last_kbd_macro_eval(
    eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    match &eval.kmacro.last_macro {
        Some(keys) => Ok(Value::vector(keys.clone())),
        None => Ok(Value::Nil),
    }
}

// ===========================================================================
// Internal helpers
// ===========================================================================

/// Resolve a macro value (vector, string, list, or symbol) into a Vec of events.
fn resolve_macro_events(value: &Value) -> Result<Vec<Value>, Flow> {
    match value {
        Value::Vector(v) => {
            let items = v.lock().expect("poisoned");
            Ok(items.clone())
        }
        Value::Str(s) => {
            // Each character in the string becomes a Char event.
            Ok(s.chars().map(Value::Char).collect())
        }
        Value::Cons(_) | Value::Nil => {
            // Try to interpret as a proper list.
            match list_to_vec(value) {
                Some(v) => Ok(v),
                None => Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("arrayp"), value.clone()],
                )),
            }
        }
        _ => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("arrayp"), value.clone()],
        )),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // KmacroManager unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn new_manager_defaults() {
        let mgr = KmacroManager::new();
        assert!(!mgr.recording);
        assert!(mgr.current_macro.is_empty());
        assert!(mgr.last_macro.is_none());
        assert!(mgr.macro_ring.is_empty());
        assert_eq!(mgr.counter, 0);
        assert_eq!(mgr.counter_format, "%d");
    }

    #[test]
    fn start_stop_recording() {
        let mut mgr = KmacroManager::new();

        // Start recording
        mgr.start_recording(false);
        assert!(mgr.recording);

        // Store some events
        mgr.store_event(Value::Char('a'));
        mgr.store_event(Value::Char('b'));
        mgr.store_event(Value::Char('c'));
        assert_eq!(mgr.current_macro.len(), 3);

        // Stop recording
        let result = mgr.stop_recording();
        assert!(!mgr.recording);
        assert!(result.is_some());
        let recorded = result.unwrap();
        assert_eq!(recorded.len(), 3);

        // last_macro should be set
        assert!(mgr.last_macro.is_some());
        assert_eq!(mgr.last_macro.as_ref().unwrap().len(), 3);

        // current_macro should be cleared
        assert!(mgr.current_macro.is_empty());
    }

    #[test]
    fn stop_recording_empty() {
        let mut mgr = KmacroManager::new();
        mgr.start_recording(false);
        // Don't store any events
        let result = mgr.stop_recording();
        assert!(result.is_none());
        assert!(mgr.last_macro.is_none());
    }

    #[test]
    fn append_recording() {
        let mut mgr = KmacroManager::new();

        // Record first macro
        mgr.start_recording(false);
        mgr.store_event(Value::Char('x'));
        mgr.store_event(Value::Char('y'));
        mgr.stop_recording();

        // Record second macro with append
        mgr.start_recording(true);
        assert_eq!(mgr.current_macro.len(), 2); // starts with previous
        mgr.store_event(Value::Char('z'));
        assert_eq!(mgr.current_macro.len(), 3);
        mgr.stop_recording();

        assert_eq!(mgr.last_macro.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn macro_ring_push() {
        let mut mgr = KmacroManager::new();

        // Record first macro
        mgr.start_recording(false);
        mgr.store_event(Value::Char('a'));
        mgr.stop_recording();
        assert!(mgr.macro_ring.is_empty()); // first macro, nothing to push

        // Record second macro (pushes first onto ring)
        mgr.start_recording(false);
        mgr.store_event(Value::Char('b'));
        mgr.stop_recording();
        assert_eq!(mgr.macro_ring.len(), 1);
        assert_eq!(mgr.macro_ring[0].len(), 1); // the 'a' macro

        // Record third macro (pushes second onto ring)
        mgr.start_recording(false);
        mgr.store_event(Value::Char('c'));
        mgr.stop_recording();
        assert_eq!(mgr.macro_ring.len(), 2);
    }

    #[test]
    fn store_event_not_recording() {
        let mut mgr = KmacroManager::new();
        // Not recording -- store_event should be a no-op
        mgr.store_event(Value::Char('a'));
        assert!(mgr.current_macro.is_empty());
    }

    #[test]
    fn format_counter_decimal() {
        let mgr = KmacroManager {
            counter: 42,
            counter_format: "%d".to_string(),
            ..KmacroManager::new()
        };
        assert_eq!(mgr.format_counter(), "42");
    }

    #[test]
    fn format_counter_hex() {
        let mgr = KmacroManager {
            counter: 255,
            counter_format: "%x".to_string(),
            ..KmacroManager::new()
        };
        assert_eq!(mgr.format_counter(), "ff");
    }

    #[test]
    fn format_counter_octal() {
        let mgr = KmacroManager {
            counter: 8,
            counter_format: "%o".to_string(),
            ..KmacroManager::new()
        };
        assert_eq!(mgr.format_counter(), "10");
    }

    #[test]
    fn format_counter_with_prefix() {
        let mgr = KmacroManager {
            counter: 7,
            counter_format: "item-%d".to_string(),
            ..KmacroManager::new()
        };
        assert_eq!(mgr.format_counter(), "item-7");
    }

    #[test]
    fn format_counter_unknown_format() {
        let mgr = KmacroManager {
            counter: 99,
            counter_format: "???".to_string(),
            ..KmacroManager::new()
        };
        // Fallback to plain decimal
        assert_eq!(mgr.format_counter(), "99");
    }

    // -----------------------------------------------------------------------
    // Builtin-level tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_start_and_end_macro() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // Start recording
        let result = builtin_start_kbd_macro(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(eval.kmacro.recording);

        // Double-start should error
        let result = builtin_start_kbd_macro(&mut eval, vec![]);
        assert!(result.is_err());

        // Store some events
        let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::Char('h')]);
        let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::Char('i')]);

        // End recording
        let result = builtin_end_kbd_macro(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(!eval.kmacro.recording);
        assert!(eval.kmacro.last_macro.is_some());
        assert_eq!(eval.kmacro.last_macro.as_ref().unwrap().len(), 2);

        // Double-end should error
        let result = builtin_end_kbd_macro(&mut eval, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_start_with_append() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // Record a macro
        let _ = builtin_start_kbd_macro(&mut eval, vec![]);
        let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::Char('a')]);
        let _ = builtin_end_kbd_macro(&mut eval, vec![]);

        // Append to it
        let _ = builtin_start_kbd_macro(&mut eval, vec![Value::True]);
        assert_eq!(eval.kmacro.current_macro.len(), 1); // 'a' carried over
        let _ = builtin_store_kbd_macro_event(&mut eval, vec![Value::Char('b')]);
        let _ = builtin_end_kbd_macro(&mut eval, vec![]);

        assert_eq!(eval.kmacro.last_macro.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_call_last_macro_no_macro() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // No macro defined -- should error
        let result = builtin_call_last_kbd_macro(&mut eval, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_store_event_wrong_args() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // Wrong arg count
        let result = builtin_store_kbd_macro_event(&mut eval, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_kmacro_counter_operations() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // Initial counter is 0
        assert_eq!(eval.kmacro.counter, 0);

        // Set counter to 10
        let result = builtin_kmacro_set_counter(&mut eval, vec![Value::Int(10)]);
        assert!(result.is_ok());
        assert_eq!(eval.kmacro.counter, 10);

        // Add 5 to counter
        let result = builtin_kmacro_add_counter(&mut eval, vec![Value::Int(5)]);
        assert!(result.is_ok());
        assert_eq!(eval.kmacro.counter, 15);

        // Add negative
        let result = builtin_kmacro_add_counter(&mut eval, vec![Value::Int(-3)]);
        assert!(result.is_ok());
        assert_eq!(eval.kmacro.counter, 12);
    }

    #[test]
    fn test_kmacro_set_format() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        assert_eq!(eval.kmacro.counter_format, "%d");

        let result = builtin_kmacro_set_format(&mut eval, vec![Value::string("%03d")]);
        assert!(result.is_ok());
        assert_eq!(eval.kmacro.counter_format, "%03d");
    }

    #[test]
    fn test_kmacro_set_counter_wrong_type() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        let result = builtin_kmacro_set_counter(&mut eval, vec![Value::string("not a number")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_defining_kbd_macro_p_eval() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // Not recording
        let result = builtin_defining_kbd_macro_p_eval(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());

        // Start recording
        eval.kmacro.start_recording(false);
        let result = builtin_defining_kbd_macro_p_eval(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());

        // Stop recording
        eval.kmacro.store_event(Value::Char('x'));
        eval.kmacro.stop_recording();
        let result = builtin_defining_kbd_macro_p_eval(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_last_kbd_macro_eval() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // No macro yet
        let result = builtin_last_kbd_macro_eval(&mut eval, vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());

        // Record a macro
        eval.kmacro.start_recording(false);
        eval.kmacro.store_event(Value::Char('a'));
        eval.kmacro.store_event(Value::Char('b'));
        eval.kmacro.stop_recording();

        let result = builtin_last_kbd_macro_eval(&mut eval, vec![]);
        assert!(result.is_ok());
        match result.unwrap() {
            Value::Vector(v) => {
                let items = v.lock().unwrap();
                assert_eq!(items.len(), 2);
            }
            other => panic!("Expected Vector, got {:?}", other),
        }
    }

    #[test]
    fn test_name_last_kbd_macro() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // No macro -- should error
        let result = builtin_name_last_kbd_macro(&mut eval, vec![Value::symbol("my-macro")]);
        assert!(result.is_err());

        // Record a macro
        eval.kmacro.start_recording(false);
        eval.kmacro
            .store_event(Value::Symbol("forward-char".to_string()));
        eval.kmacro.stop_recording();

        // Name it
        let result = builtin_name_last_kbd_macro(&mut eval, vec![Value::symbol("my-macro")]);
        assert!(result.is_ok());

        // Check that the symbol has a function binding
        let func = eval.obarray.symbol_function("my-macro");
        assert!(func.is_some());
        match func.unwrap() {
            Value::Vector(v) => {
                let items = v.lock().unwrap();
                assert_eq!(items.len(), 1);
            }
            other => panic!("Expected Vector, got {:?}", other),
        }
    }

    #[test]
    fn test_name_last_kbd_macro_wrong_type() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        let result = builtin_name_last_kbd_macro(&mut eval, vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_kmacro_p() {
        // Vector is a macro
        let result = builtin_kmacro_p(vec![Value::vector(vec![Value::Char('a')])]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());

        // String is a macro
        let result = builtin_kmacro_p(vec![Value::string("abc")]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());

        // Integer is not a macro
        let result = builtin_kmacro_p(vec![Value::Int(42)]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());

        // Nil is not a macro
        let result = builtin_kmacro_p(vec![Value::Nil]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());

        // Wrong arg count
        let result = builtin_kmacro_p(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_executing_kbd_macro_p() {
        // Stub: always nil
        let result = builtin_executing_kbd_macro_p(vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_defining_kbd_macro_p_pure() {
        // Pure stub: always nil
        let result = builtin_defining_kbd_macro_p(vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_kbd_macro_query_subset() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        let wrong_arity = builtin_kbd_macro_query(&mut eval, vec![]).unwrap_err();
        match wrong_arity {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-number-of-arguments");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("kbd-macro-query"), Value::Int(0)]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }

        let not_recording = builtin_kbd_macro_query(&mut eval, vec![Value::True]).unwrap_err();
        match not_recording {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "user-error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Not defining or executing kbd macro")]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }

        eval.kmacro.start_recording(false);
        let ok = builtin_kbd_macro_query(&mut eval, vec![Value::Nil]).unwrap();
        assert!(ok.is_nil());
    }

    #[test]
    fn test_resolve_macro_events_vector() {
        let v = Value::vector(vec![Value::Char('a'), Value::Char('b')]);
        let events = resolve_macro_events(&v).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_resolve_macro_events_string() {
        let s = Value::string("hello");
        let events = resolve_macro_events(&s).unwrap();
        assert_eq!(events.len(), 5);
        match &events[0] {
            Value::Char('h') => {}
            other => panic!("Expected Char('h'), got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_macro_events_list() {
        let list = Value::list(vec![Value::Char('x'), Value::Char('y')]);
        let events = resolve_macro_events(&list).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_resolve_macro_events_wrong_type() {
        let result = resolve_macro_events(&Value::Int(42));
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_kbd_macro() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();

        // Record and name a macro
        eval.kmacro.start_recording(false);
        eval.kmacro.store_event(Value::Char('a'));
        eval.kmacro.store_event(Value::Char('b'));
        eval.kmacro.stop_recording();
        let _ = builtin_name_last_kbd_macro(&mut eval, vec![Value::symbol("test-macro")]);

        // Insert it
        let result = builtin_insert_kbd_macro(&mut eval, vec![Value::symbol("test-macro")]);
        assert!(result.is_ok());
        let text = result.unwrap();
        let s = text.as_str().unwrap();
        assert!(s.contains("fset"));
        assert!(s.contains("test-macro"));
    }
}
