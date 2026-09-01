//! Keyboard macro support -- macro metadata, counter state, and Lisp entry points.
//!
//! Provides Emacs-compatible keyboard macro functionality:
//! - `start-kbd-macro` / `end-kbd-macro` -- record key sequences
//! - `call-last-kbd-macro` -- replay the last recorded macro
//! - `execute-kbd-macro` -- execute a macro N times
//! - `name-last-kbd-macro` -- bind a macro to a symbol
//! - `insert-kbd-macro` -- insert macro definition as Lisp text
//! - `kbd-macro-query` -- interactive query during playback
//! - `store-kbd-macro-event` -- add event to the keyboard runtime's current recording
//! - `kmacro-set-counter` / `kmacro-add-counter` / `kmacro-set-format` -- counter ops
//! - `executing-kbd-macro-p` / `defining-kbd-macro-p` -- predicates
//! - `last-kbd-macro` -- retrieve last macro value
//! - `kmacro-p` -- predicate for macro values

use super::error::{EvalResult, Flow, signal};
use super::intern::resolve_sym;
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Argument helpers (local copies, matching builtins.rs convention)
// ---------------------------------------------------------------------------

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

fn prefix_numeric_value(value: &Value) -> i64 {
    crate::emacs_core::prefix::prefix_numeric_value(value)
}

// ---------------------------------------------------------------------------
// KmacroManager
// ---------------------------------------------------------------------------

/// Metadata manager for keyboard macros.
///
/// GNU keeps the live recording/playback state on the keyboard runtime
/// (`current_kboard` plus global execution vars) and layers richer kmacro UI
/// state on top. NeoVM mirrors that split: the keyboard owner handles current
/// recording/playback, while this manager keeps only the higher-level ring and
/// counter metadata.
#[derive(Clone, Debug)]
pub struct KmacroManager {
    /// Ring of previously saved macros (most recent first).
    pub macro_ring: Vec<Vec<Value>>,
    /// Keyboard macro counter (for `kmacro-insert-counter`).
    pub counter: i64,
    /// Format string for the counter (printf-style, default "%d").
    pub counter_format: LispString,
}

impl Default for KmacroManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for KmacroManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for macro_entry in &self.macro_ring {
            for value in macro_entry {
                roots.push(*value);
            }
        }
    }
}

impl KmacroManager {
    /// Create a new manager with default state.
    pub fn new() -> Self {
        Self {
            macro_ring: Vec::new(),
            counter: 0,
            counter_format: LispString::from_utf8("%d"),
        }
    }

    /// Format the counter using the current format string.
    pub fn format_counter(&self) -> String {
        // Support basic %d / %o / %x / %X formats.
        // For anything more complex, fall back to decimal.
        let fmt = crate::emacs_core::emacs_char::to_utf8_lossy(self.counter_format.as_bytes());
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

fn last_kbd_macro_or_array_error(eval: &super::eval::Context) -> Result<Vec<Value>, Flow> {
    eval.command_loop
        .last_kbd_macro()
        .map(|events| events.to_vec())
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("arrayp"), Value::NIL],
            )
        })
}

fn execute_kbd_macro_iteration(eval: &mut super::eval::Context) -> EvalResult {
    eval.execute_kbd_macro_iteration_via_command_loop()
}

fn execute_kbd_macro_events_with_runtime_state(
    eval: &mut super::eval::Context,
    macro_events: &[Value],
    count: i64,
    loopfunc: Value,
) -> EvalResult {
    // GNU restores the invoking command's key sequence after running a macro,
    // so the executed macro events do not leak into `this-command-keys'
    // (`Fexecute_kbd_macro' saves/restores the command-loop key state). Snapshot
    // it here and restore afterwards.
    let saved_command_keys = eval.read_command_keys().to_vec();
    let result = eval.with_executing_kbd_macro_runtime(macro_events.to_vec(), |eval| {
        let mut repeat = count;
        let mut success_count = 0usize;
        loop {
            eval.reset_executing_kbd_macro_runtime_iteration();

            if !loopfunc.is_nil() {
                let cont = eval.apply(loopfunc, vec![])?;
                if !cont.is_truthy() {
                    break;
                }
            }

            execute_kbd_macro_iteration(eval)?;
            success_count += 1;
            eval.note_executing_kbd_macro_iteration(success_count);

            if repeat == 0 {
                continue;
            }
            repeat -= 1;
            if repeat == 0 {
                break;
            }
        }

        Ok(Value::NIL)
    });
    eval.set_read_command_keys(saved_command_keys);
    result
}

fn publish_kbd_macro_status(eval: &mut super::eval::Context, message: &str) -> EvalResult {
    super::builtins::dispatch_builtin(eval, "message", vec![Value::string(message)])
        .expect("`message` must remain a registered builtin")
}

fn start_kbd_macro_impl(
    eval: &mut super::eval::Context,
    append: bool,
    no_exec: bool,
) -> EvalResult {
    if eval.command_loop.keyboard.kboard.defining_kbd_macro {
        return Err(signal(
            "error",
            vec![Value::string("Already defining kbd macro")],
        ));
    }
    let initial_events = if append {
        Some(last_kbd_macro_or_array_error(eval)?)
    } else {
        None
    };

    if let Some(ref initial_events) = initial_events
        && !no_exec
    {
        execute_kbd_macro_events_with_runtime_state(eval, initial_events, 1, Value::NIL)?;
    }

    publish_kbd_macro_status(
        eval,
        if append {
            "Appending to kbd macro..."
        } else {
            "Defining kbd macro..."
        },
    )?;
    eval.start_kbd_macro_runtime(initial_events.as_deref(), append)?;
    Ok(Value::NIL)
}

pub(crate) fn plan_execute_kbd_macro(
    eval: &super::eval::Context,
    args: &[Value],
) -> Result<(Vec<Value>, i64, Value), Flow> {
    expect_min_args("execute-kbd-macro", args, 1)?;
    expect_max_args("execute-kbd-macro", args, 3)?;
    let count = args.get(1).map_or(1, prefix_numeric_value);
    let loopfunc = args.get(2).copied().unwrap_or(Value::NIL);
    Ok((resolve_macro_events(eval, &args[0])?, count, loopfunc))
}

/// (start-kbd-macro &optional APPEND NO-EXEC) -> nil
///
/// Start recording a keyboard macro.  With non-nil APPEND, append to
/// the last macro instead of starting a new one.  Signals an error if
/// already recording.  With APPEND and nil NO-EXEC, replay the previous
/// macro before starting the new appended definition, matching GNU Emacs.
pub(crate) fn builtin_start_kbd_macro(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("start-kbd-macro", &args, 2)?;
    let append = args.first().is_some_and(|v| v.is_truthy());
    let no_exec = args.get(1).is_some_and(|v| v.is_truthy());
    start_kbd_macro_impl(eval, append, no_exec)?;
    Ok(Value::NIL)
}

/// (end-kbd-macro &optional REPEAT LOOPFUNC) -> nil
///
/// Stop recording a keyboard macro and optionally replay it.
pub(crate) fn builtin_end_kbd_macro(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("end-kbd-macro", &args, 2)?;
    let repeat = if let Some(value) = args.first() {
        expect_int(value).unwrap_or(1)
    } else {
        1
    };
    let loopfunc = args.get(1).copied().unwrap_or(Value::NIL);
    let recorded = eval.end_kbd_macro_runtime()?;
    publish_kbd_macro_status(eval, "Keyboard macro defined")?;
    if repeat == 0 {
        execute_kbd_macro_events_with_runtime_state(eval, &recorded, repeat, loopfunc)?;
    } else if repeat > 1 {
        execute_kbd_macro_events_with_runtime_state(eval, &recorded, repeat - 1, loopfunc)?;
    }
    Ok(Value::NIL)
}

/// (call-last-kbd-macro &optional REPEAT LOOPFUNC) -> nil
///
/// Execute the last keyboard macro.
pub(crate) fn builtin_call_last_kbd_macro(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("call-last-kbd-macro", &args, 2)?;
    if eval.command_loop.keyboard.kboard.defining_kbd_macro {
        return Err(signal(
            "error",
            vec![Value::string(
                "Can't execute anonymous macro while defining one",
            )],
        ));
    }

    // GNU `Fcall_last_kbd_macro' (macros.c) executes the Lisp variable
    // `last-kbd-macro' (KVAR(current_kboard, Vlast_kbd_macro)), not an internal
    // field, so `(setq last-kbd-macro ...)' then call-last-kbd-macro works. A
    // nil value signals "No kbd macro has been defined".
    let last = eval.eval_symbol("last-kbd-macro").unwrap_or(Value::NIL);
    if last.is_nil() {
        return Err(signal(
            "error",
            vec![Value::string("No kbd macro has been defined")],
        ));
    }
    let macro_keys = resolve_macro_events(eval, &last)?;
    let repeat = args.first().map_or(1i64, prefix_numeric_value);
    let loopfunc = args.get(1).copied().unwrap_or(Value::NIL);
    let previous_last_command = eval.eval_symbol("last-command").unwrap_or(Value::NIL);
    let macro_value = Value::vector(macro_keys.clone());
    eval.assign("this-command", previous_last_command);
    eval.assign("real-this-command", macro_value);
    let result = execute_kbd_macro_events_with_runtime_state(eval, &macro_keys, repeat, loopfunc);
    if let Ok(last_command) = eval.eval_symbol("last-command") {
        eval.assign("this-command", last_command);
    }
    result
}

/// (execute-kbd-macro MACRO &optional COUNT LOOPFUNC) -> nil
///
/// Execute MACRO (a vector, string, or symbol) COUNT times.
/// If MACRO is a symbol, its function definition is used.
pub(crate) fn builtin_execute_kbd_macro(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (macro_events, count, loopfunc) = plan_execute_kbd_macro(eval, &args)?;
    execute_kbd_macro_events_with_runtime_state(eval, &macro_events, count, loopfunc)
}

/// (name-last-kbd-macro SYMBOL) -> nil
///
/// Bind the last keyboard macro to SYMBOL as its function definition.
/// Signals an error if no macro has been recorded.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn name_last_kbd_macro_impl(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
    call_name: &str,
) -> EvalResult {
    expect_args(call_name, &args, 1)?;

    let name = match args[0].kind() {
        ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
        ValueKind::String => args[0]
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .expect("ValueKind::String must carry LispString payload"),
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };

    let macro_val = match eval.command_loop.last_kbd_macro() {
        Some(keys) => Value::vector(keys.to_vec()),
        None => {
            return Err(signal(
                "error",
                vec![Value::string("No keyboard macro has been defined")],
            ));
        }
    };

    eval.obarray.set_symbol_function(&name, macro_val);
    Ok(Value::NIL)
}

/// (name-last-kbd-macro SYMBOL) -> nil
///
/// Bind the last keyboard macro to SYMBOL as its function definition.
/// Signals an error if no macro has been recorded.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_name_last_kbd_macro(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    name_last_kbd_macro_impl(eval, args, "name-last-kbd-macro")
}

/// (last-kbd-macro) -> last recorded macro vector or nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_last_kbd_macro(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("last-kbd-macro", &args, 0)?;
    match eval.command_loop.last_kbd_macro() {
        Some(keys) => Ok(Value::vector(keys.to_vec())),
        None => Ok(Value::NIL),
    }
}

/// (kmacro-p OBJECT) -> non-nil when OBJECT is a keyboard macro value.
///
/// Compatibility subset: accepts vector and string macro encodings.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_kmacro_p(args: Vec<Value>) -> EvalResult {
    expect_args("kmacro-p", &args, 1)?;
    Ok(Value::bool_val(args[0].is_vector() || args[0].is_string()))
}

/// (store-kbd-macro-event EVENT) -> nil
///
/// Add EVENT to the keyboard macro currently being recorded.
/// If not currently recording, this is a no-op.
pub(crate) fn builtin_store_kbd_macro_event(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("store-kbd-macro-event", &args, 1)?;
    eval.store_kbd_macro_runtime_event(args[0]);
    Ok(Value::NIL)
}

// ===========================================================================
// Internal helpers
// ===========================================================================

fn indirect_macro_function(eval: &super::eval::Context, value: &Value) -> Value {
    let mut current = *value;
    let mut seen = HashSet::new();

    loop {
        let Some(symbol_id) = (match current.kind() {
            ValueKind::Symbol(id) => Some(id),
            _ if current.bits() == Value::T.bits() => Some(super::intern::intern("t")),
            _ => None,
        }) else {
            return current;
        };

        if !seen.insert(symbol_id) {
            return current;
        }

        current = eval
            .obarray()
            .symbol_function_id(symbol_id)
            .unwrap_or(Value::NIL);
    }
}

/// Resolve a macro value the GNU `execute-kbd-macro` way:
/// follow symbol function indirections, then require a final string or vector.
fn resolve_macro_events(eval: &super::eval::Context, value: &Value) -> Result<Vec<Value>, Flow> {
    match indirect_macro_function(eval, value).kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = indirect_macro_function(eval, value)
                .as_vector_data()
                .unwrap()
                .clone();
            Ok(items.clone())
        }
        ValueKind::String => {
            // Each character in the string becomes a Char event.
            let s = indirect_macro_function(eval, value)
                .as_utf8_str()
                .unwrap()
                .to_owned();
            Ok(s.chars().map(Value::char).collect())
        }
        _ => Err(signal(
            "error",
            vec![Value::string("Keyboard macros must be strings or vectors")],
        )),
    }
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
