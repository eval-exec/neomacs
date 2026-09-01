//! Emacs error hierarchy system.
//!
//! Implements `define-error`, error condition matching via `error-conditions`
//! and `error-message` symbol properties (matching real Emacs behavior), and
//! provides `init_standard_errors` to pre-populate the standard hierarchy.
//!
//! # How it works
//!
//! Each error symbol has two plist properties:
//! - `error-conditions`: a list of symbols representing the error and all its
//!   ancestors (including itself).  E.g. for `file-missing`:
//!   `(file-missing file-error error)`
//! - `error-message`: a human-readable string describing the error.
//!
//! `condition-case` uses `signal_matches_hierarchical` to check whether a
//! signalled error's `error-conditions` list includes the handler's condition
//! symbol.

use super::error::{
    EvalResult, Flow, signal, signal_suppressed, signal_with_data, signal_with_data_id,
};
use super::intern::{SymId, T_SYM_ID, intern, resolve_sym};
use super::symbol::Obarray;
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::value::ValueKind;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Obarray-based error hierarchy helpers
// ---------------------------------------------------------------------------

/// Set `error-conditions` and `error-message` properties on `name` in the
/// obarray.  `conditions` is the full list of condition symbols (including
/// `name` itself, its parents, and their transitive ancestors).
fn put_error_properties(obarray: &mut Obarray, name: &str, message: &str, conditions: Vec<&str>) {
    let cond_list = Value::list(conditions.iter().map(|s| Value::symbol(*s)).collect());
    obarray
        .put_property(name, "error-conditions", cond_list)
        .expect("error-conditions plist should always be valid during init");
    obarray
        .put_property(name, "error-message", Value::string(message))
        .expect("error-message plist should always be valid during init");
}

/// Collect the full condition list for `name` given its direct `parents`.
/// The result always starts with `name`, then the union of each parent's
/// `error-conditions` list (read from the obarray).  If a parent has no
/// `error-conditions` yet, just the parent symbol itself is included.
fn build_conditions_from_obarray(obarray: &Obarray, name: &str, parents: &[&str]) -> Vec<String> {
    let mut conditions = vec![name.to_string()];
    for &parent in parents {
        // Read the parent's error-conditions list from the obarray.
        if let Some(parent_conds) = obarray.get_property(parent, "error-conditions") {
            for sym in iter_symbol_list(&parent_conds) {
                if !conditions.contains(&sym) {
                    conditions.push(sym);
                }
            }
        } else {
            // Parent not yet registered — include the bare symbol.
            if !conditions.contains(&parent.to_string()) {
                conditions.push(parent.to_string());
            }
        }
    }
    conditions
}

/// Iterate over a Value list, yielding symbol names.
fn iter_symbol_list(value: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(items) = list_to_vec(value) {
        for item in items {
            if let Some(name) = item.as_symbol_name() {
                result.push(name.to_string());
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Hierarchical signal matching (for condition-case)
// ---------------------------------------------------------------------------

/// Check whether `signal_sym` matches `condition_sym` using the error
/// hierarchy stored in the obarray.
///
/// Returns `true` if:
/// - `condition_sym` is `"t"` (catches everything),
/// - `condition_sym == signal_sym`, or
/// - `condition_sym` appears in `signal_sym`'s `error-conditions` plist.
///
/// This is the hierarchical replacement for the flat `signal_matches` in
/// `error.rs`.
pub fn signal_matches_hierarchical(
    obarray: &Obarray,
    signal_sym: &str,
    condition_sym: &str,
) -> bool {
    // `t` catches all signals.
    if condition_sym == "t" {
        return true;
    }
    // Exact match (fast path).
    if signal_sym == condition_sym {
        return true;
    }
    // Check the error-conditions plist on the signal symbol.
    if let Some(conds) = obarray.get_property(signal_sym, "error-conditions") {
        for sym_name in iter_symbol_list(&conds) {
            if sym_name == condition_sym {
                return true;
            }
        }
    }
    false
}

/// Like `signal_matches_hierarchical` but matches a runtime `Value`
/// produced by compiled bytecode condition handlers.
pub fn signal_matches_condition_value(
    obarray: &Obarray,
    signal_sym: &str,
    pattern: &Value,
) -> bool {
    match pattern.kind() {
        ValueKind::T => true,
        ValueKind::Nil => false,
        ValueKind::Cons => list_to_vec(pattern).is_some_and(|items| {
            items
                .iter()
                .any(|item| signal_matches_condition_value(obarray, signal_sym, item))
        }),
        _ => {
            // Use symbol_id to handle both bare symbols and symbol-with-pos wrappers.
            if let Some(id) = super::builtins::symbols::symbol_id(pattern) {
                signal_matches_hierarchical(obarray, signal_sym, resolve_sym(id))
            } else {
                false
            }
        }
    }
}

/// Identity-aware variant of `signal_matches_condition_value`.
///
/// GNU matches a `condition-case' clause by testing whether the clause's
/// condition is a member of the *signalled symbol's* `error-conditions' list
/// (`src/eval.c:wants_debugger`/`find_handler_clause`).  The signal symbol must
/// be looked up by identity: an uninterned error symbol (from `make-symbol' +
/// `define-error') carries its conditions on that specific symbol object, so a
/// name-based lookup would resolve to a different, condition-less interned
/// symbol and the clause would never match.
pub fn signal_matches_condition_value_sym(
    obarray: &Obarray,
    signal_sym: SymId,
    pattern: &Value,
) -> bool {
    // The signal's own conditions, read by identity (falling back to just the
    // symbol itself, matching `signal_matches_hierarchical`'s exact-match rule).
    let conditions = obarray.get_property_id(signal_sym, intern("error-conditions"));
    signal_matches_pattern_against_conditions(signal_sym, conditions.as_ref(), pattern)
}

/// Identity-aware hierarchical match for one signal and one condition.
///
/// This is the object-level counterpart of [`signal_matches_hierarchical`]
/// for runtime paths where GNU compares Lisp symbol objects rather than their
/// potentially raw or mutable names.
pub fn signal_matches_hierarchical_sym(
    obarray: &Obarray,
    signal_sym: SymId,
    condition_sym: SymId,
) -> bool {
    let conditions = obarray.get_property_id(signal_sym, intern("error-conditions"));
    signal_matches_symbol_against_conditions(signal_sym, conditions.as_ref(), condition_sym)
}

fn signal_matches_pattern_against_conditions(
    signal_sym: SymId,
    conditions: Option<&Value>,
    pattern: &Value,
) -> bool {
    match pattern.kind() {
        ValueKind::T => true,
        ValueKind::Nil => false,
        ValueKind::Cons => list_to_vec(pattern).is_some_and(|items| {
            items
                .iter()
                .any(|item| signal_matches_pattern_against_conditions(signal_sym, conditions, item))
        }),
        _ => {
            let Some(cond_id) = super::builtins::symbols::symbol_id(pattern) else {
                return false;
            };
            signal_matches_symbol_against_conditions(signal_sym, conditions, cond_id)
        }
    }
}

fn signal_matches_symbol_against_conditions(
    signal_sym: SymId,
    conditions: Option<&Value>,
    condition_sym: SymId,
) -> bool {
    if pattern_is_catch_all(condition_sym) || condition_sym == signal_sym {
        return true;
    }
    let Some(conditions) = conditions else {
        return false;
    };
    let mut tail = *conditions;
    while tail.is_cons() {
        if tail.cons_car().as_symbol_id() == Some(condition_sym) {
            return true;
        }
        tail = tail.cons_cdr();
    }
    false
}

fn pattern_is_catch_all(cond_id: SymId) -> bool {
    cond_id == T_SYM_ID
}

// ---------------------------------------------------------------------------
// Standard Emacs error hierarchy initialisation
// ---------------------------------------------------------------------------

/// Pre-populate the obarray with the standard Emacs error hierarchy.
///
/// Must be called once during evaluator initialisation (after the obarray is
/// created but before any user code runs).
pub fn init_standard_errors(obarray: &mut Obarray) {
    // Root error.
    put_error_properties(obarray, "error", "error", vec!["error"]);

    // GNU `src/data.c:syms_of_data` registers `quit` outside the `error`
    // hierarchy: (get 'quit 'error-conditions) is just (quit).  This is what
    // lets `(condition-case ... (error ...))` avoid catching quits.
    register_simple(obarray, "quit", "Quit", &[]);
    register_simple(obarray, "minibuffer-quit", "Quit", &["quit"]);

    // --- Direct children of `error` ---

    register_simple(obarray, "user-error", "User error", &["error"]);
    register_simple(
        obarray,
        "args-out-of-range",
        "Args out of range",
        &["error"],
    );
    register_simple(
        obarray,
        "beginning-of-buffer",
        "Beginning of buffer",
        &["error"],
    );
    register_simple(obarray, "end-of-buffer", "End of buffer", &["error"]);
    register_simple(
        obarray,
        "end-of-file",
        "End of file during parsing",
        &["error"],
    );
    register_simple(
        obarray,
        "buffer-read-only",
        "Buffer is read-only",
        &["error"],
    );
    register_simple(
        obarray,
        "coding-system-error",
        "Invalid coding system",
        &["error"],
    );
    register_simple(obarray, "circular-list", "List contains a loop", &["error"]);
    register_simple(
        obarray,
        "cyclic-function-indirection",
        "Symbol's chain of function indirections contains a loop",
        &["error"],
    );
    register_simple(
        obarray,
        "cyclic-variable-indirection",
        "Symbol's chain of variable indirections contains a loop",
        &["error"],
    );
    register_simple(obarray, "invalid-function", "Invalid function", &["error"]);
    register_simple(
        obarray,
        "invalid-read-syntax",
        "Invalid read syntax",
        &["error"],
    );
    register_simple(obarray, "invalid-regexp", "Invalid regexp", &["error"]);
    register_simple(
        obarray,
        "wrong-length-argument",
        "Wrong length argument",
        &["error"],
    );
    register_simple(
        obarray,
        "mark-inactive",
        "The mark is not active now",
        &["error"],
    );
    register_simple(obarray, "no-catch", "No catch for tag", &["error"]);
    register_simple(obarray, "scan-error", "Scan error", &["error"]);
    register_simple(obarray, "search-failed", "Search failed", &["error"]);
    register_simple(
        obarray,
        "malformed-keyword-arg-list",
        "Keyword lacks a corresponding value",
        &["error"],
    );
    register_simple(
        obarray,
        "setting-constant",
        "Attempt to set a constant symbol",
        &["error"],
    );
    // GNU data.c: PUT_ERROR (Qtext_read_only, Fcons (Qbuffer_read_only,
    // error_tail), ...) -- text-read-only is a subcondition of
    // buffer-read-only, so a `(buffer-read-only ...)' handler catches it.
    register_simple(
        obarray,
        "text-read-only",
        "Text is read-only",
        &["buffer-read-only"],
    );
    register_simple(
        obarray,
        "void-function",
        "Symbol\u{2019}s function definition is void",
        &["error"],
    );
    register_simple(
        obarray,
        "void-variable",
        "Symbol\u{2019}s value as variable is void",
        &["error"],
    );
    register_simple(
        obarray,
        "wrong-number-of-arguments",
        "Wrong number of arguments",
        &["error"],
    );
    register_simple(
        obarray,
        "wrong-type-argument",
        "Wrong type argument",
        &["error"],
    );
    register_simple(
        obarray,
        "cl-assertion-failed",
        "Assertion failed",
        &["error"],
    );
    // GNU fns.c — type-mismatch is signaled by value< for incompatible types.
    register_simple(obarray, "type-mismatch", "Type mismatch", &["error"]);
    register_simple(
        obarray,
        "permission-denied",
        "Permission denied",
        &["error"],
    );
    register_simple(
        obarray,
        "recursion-error",
        "Excessive recursive calling error",
        &["error"],
    );

    // --- arith-error family ---
    register_simple(obarray, "arith-error", "Arithmetic error", &["error"]);
    register_simple(
        obarray,
        "overflow-error",
        "Arithmetic overflow error",
        &["arith-error"],
    );
    register_simple(
        obarray,
        "range-error",
        "Arithmetic range error",
        &["arith-error"],
    );
    register_simple(
        obarray,
        "domain-error",
        "Arithmetic domain error",
        &["arith-error"],
    );
    register_simple(
        obarray,
        "underflow-error",
        "Arithmetic underflow error",
        &["arith-error"],
    );

    // --- file-error family ---
    register_simple(obarray, "file-error", "File error", &["error"]);
    register_simple(
        obarray,
        "file-already-exists",
        "File already exists",
        &["file-error"],
    );
    register_simple(
        obarray,
        "file-date-error",
        "Cannot set file date",
        &["file-error"],
    );
    register_simple(obarray, "file-locked", "File is locked", &["file-error"]);
    register_simple(obarray, "file-missing", "File is missing", &["file-error"]);
    register_simple(
        obarray,
        "file-notify-error",
        "File notification error",
        &["file-error"],
    );
    // No `dbus-error'.  GNU puts its `error-conditions' and `error-message' in
    // `syms_of_dbusbind' (src/dbusbind.c:2013-2017), inside `#ifdef HAVE_DBUS',
    // and this build has no D-Bus transport.  GNU's own `lisp/net/dbus.el:50-51'
    // supplies it for exactly this build, with the comment "The following
    // symbols are defined in dbusbind.c.  We need them also when Emacs is
    // compiled without D-Bus support."  Ledger 192.

    // --- sqlite-error family ---
    register_simple(obarray, "sqlite-error", "Database error", &["error"]);
    register_simple(
        obarray,
        "sqlite-locked-error",
        "Database locked",
        &["sqlite-error"],
    );

    // --- json-error family (mirrors GNU src/json.c `syms_of_json`) ---
    // Parents must be registered before children so the transitive
    // `error-conditions` closure can be built.
    register_simple(obarray, "json-error", "generic JSON error", &["error"]);
    register_simple(
        obarray,
        "json-out-of-memory",
        "not enough memory for creating JSON object",
        &["json-error"],
    );
    register_simple(
        obarray,
        "json-parse-error",
        "could not parse JSON stream",
        &["json-error"],
    );
    register_simple(
        obarray,
        "json-end-of-file",
        "end of JSON stream",
        &["json-parse-error"],
    );
    register_simple(
        obarray,
        "json-trailing-content",
        "trailing content after JSON stream",
        &["json-parse-error"],
    );
    register_simple(
        obarray,
        "json-object-too-deep",
        "object cyclic or Lisp evaluation too deep",
        &["json-error"],
    );
    register_simple(
        obarray,
        "json-utf8-decode-error",
        "invalid utf-8 encoding",
        &["json-error"],
    );
    register_simple(
        obarray,
        "json-invalid-surrogate-error",
        "invalid surrogate pair",
        &["json-error"],
    );
    register_simple(
        obarray,
        "json-number-out-of-range-error",
        "number out of range",
        &["json-error"],
    );
    register_simple(
        obarray,
        "json-escape-sequence-error",
        "invalid escape sequence",
        &["json-parse-error"],
    );
    // Neomacs extension: serialization-side failures (GNU signals a plain
    // `error` for these). Kept as a `json-error` subtype so existing
    // `condition-case` handlers on `json-error` still catch them.
    register_simple(
        obarray,
        "json-serialize-error",
        "JSON serialize error",
        &["json-error"],
    );

    // --- treesit-error family (GNU src/treesit.c `syms_of_treesit`) ---
    register_simple(
        obarray,
        "treesit-error",
        "Generic tree-sitter error",
        &["error"],
    );
    register_simple(
        obarray,
        "treesit-query-error",
        "Query pattern is malformed",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-parse-error",
        "Parse failed",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-range-invalid",
        "RANGES are invalid: they have to be ordered and should not overlap",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-buffer-too-large",
        "Buffer too large (> 4GiB)",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-load-language-error",
        "Cannot load language definition",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-node-outdated",
        "This node is outdated, please retrieve a new one",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-buffer_changed",
        "Buffer content changed, please don't edit buffer in predicate function, etc",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-node-buffer-killed",
        "The buffer associated with this node is killed",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-parser-deleted",
        "This parser is deleted and cannot be used",
        &["treesit-error"],
    );
    register_simple(
        obarray,
        "treesit-invalid-predicate",
        "Invalid predicate, see `treesit-thing-settings' for valid forms for a predicate",
        &["treesit-error"],
    );

    // --- remote-file-error (child of file-error) ---
    register_simple(
        obarray,
        "remote-file-error",
        "Remote file error",
        &["file-error"],
    );

    // Also register some common signal names that may be used without a
    // full `define-error` (e.g. excessive-lisp-nesting).
    register_simple(
        obarray,
        "excessive-lisp-nesting",
        "Lisp nesting exceeds `max-lisp-eval-depth'",
        &["recursion-error"],
    );
}

/// Helper: register a single error with explicit parents.
/// The parents must already be registered in the obarray (their
/// `error-conditions` are read to build the transitive closure).
fn register_simple(obarray: &mut Obarray, name: &str, message: &str, parents: &[&str]) {
    let conditions = build_conditions_from_obarray(obarray, name, parents);
    let cond_refs: Vec<&str> = conditions.iter().map(|s| s.as_str()).collect();
    put_error_properties(obarray, name, message, cond_refs);
}

// ---------------------------------------------------------------------------
// Builtins: signal wrapper and error-message-string
// ---------------------------------------------------------------------------

fn build_signal_flow(symbol_name: &str, data: Value) -> Flow {
    signal_with_data(symbol_name, data)
}

fn build_peculiar_signal_flow(eval: &super::eval::Context, error_object: Value) -> Flow {
    if !error_object.is_cons() {
        unreachable!("peculiar signal error object must be a cons");
    };
    let pair_car = error_object.cons_car();
    let pair_cdr = error_object.cons_cdr();
    let error_symbol = pair_car;
    let data = pair_cdr;

    let Some(symbol_id) = error_symbol.as_symbol_id() else {
        return signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), error_symbol],
        );
    };
    // Read `error-conditions' by identity so an uninterned error symbol is
    // honoured (see `builtin_signal`).
    if symbol_id != intern("error")
        && symbol_id != intern("quit")
        && eval
            .obarray
            .get_property_id(symbol_id, intern("error-conditions"))
            .is_none()
    {
        return signal_suppressed("error", vec![Value::string("Invalid error symbol")]);
    }

    build_signal_flow_id_suppressed(symbol_id, data)
}

/// Identity-preserving, signal-hook-suppressed variant of `build_signal_flow`.
fn build_signal_flow_id_suppressed(symbol: SymId, data: Value) -> Flow {
    use super::error::signal_internal_id;
    let normalized = list_to_vec(&data).unwrap_or_else(|| vec![data]);
    signal_internal_id(symbol, normalized, Some(data), true)
}

/// Eval-aware `signal`, including GNU's "peculiar error" handling for
/// `nil` as the public error symbol.
pub(crate) fn builtin_signal(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("signal"), Value::fixnum(args.len() as i64)],
        ));
    }
    // GNU 31.0.90: `signal` is DEFUN(1, 2) — DATA is optional, defaulting to nil.
    let data = args.get(1).copied().unwrap_or(Value::NIL);

    if args[0].is_nil() {
        let flow = if data.is_cons() {
            build_peculiar_signal_flow(eval, data)
        } else {
            build_signal_flow("error", data)
        };
        return dispatch_signal_flow(eval, flow);
    }

    // GNU `signal_or_quit` (src/eval.c:1930):
    //   error = (!SYMBOLP (error_symbol) && NILP (data)) ? error_symbol
    //                                                     : Fcons (error_symbol, data);
    //   real_error_symbol = CONSP (error) ? XCAR (error) : error_symbol;
    // So when the public error symbol is itself a cons and DATA is nil, the
    // cons *is* the whole error object, re-raised as-is: its car becomes the
    // real error symbol and its cdr the data.  This is the re-signal idiom
    // `(signal (list 'scan-error "msg" 2 4))` used by ~91 lisp/ call sites.
    if args[0].is_cons() && data.is_nil() {
        let flow = build_resignal_flow(eval, args[0]);
        return dispatch_signal_flow(eval, flow);
    }

    // Preserve the *identity* of the error symbol (GNU `Fsignal` passes the
    // actual symbol object to `signal_or_quit`).  An uninterned symbol given
    // `error-conditions' by `define-error' must keep its SymId so condition
    // matching and `canonicalize_signal_symbol' read the right plist; building
    // the flow from the symbol's name would re-intern to a different symbol.
    let Some(symbol_id) = args[0].as_symbol_id() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };

    let flow = build_signal_flow_id(symbol_id, data);

    dispatch_signal_flow(eval, flow)
}

/// Identity-preserving variant of `build_signal_flow`.
fn build_signal_flow_id(symbol: SymId, data: Value) -> Flow {
    signal_with_data_id(symbol, data)
}

/// `(signal CONS)` with nil DATA: the cons *is* the whole error object,
/// re-raised as-is.  GNU `signal_or_quit` sets `real_error_symbol = XCAR(error)`
/// and keeps `XCDR(error)` as the data.  Unlike `build_peculiar_signal_flow`
/// (which models OOM and suppresses the signal hook), this is a normal
/// `Fsignal` call, so the signal hook runs.
fn build_resignal_flow(eval: &super::eval::Context, error_object: Value) -> Flow {
    let error_symbol = error_object.cons_car();
    let data = error_object.cons_cdr();

    // `Fget (real_error_symbol, Qerror_conditions)` runs `CHECK_SYMBOL`, so a
    // non-symbol car signals `wrong-type-argument symbolp <car>`.
    let Some(symbol_id) = error_symbol.as_symbol_id() else {
        return signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), error_symbol],
        );
    };
    // Read `error-conditions' by identity so an uninterned error symbol is
    // honoured (see `builtin_signal`).
    if symbol_id != intern("error")
        && symbol_id != intern("quit")
        && eval
            .obarray
            .get_property_id(symbol_id, intern("error-conditions"))
            .is_none()
    {
        return signal(
            "error",
            vec![
                Value::string("Invalid error symbol"),
                Value::from_sym_id(symbol_id),
            ],
        );
    }

    build_signal_flow_id(symbol_id, data)
}

fn dispatch_signal_flow(eval: &mut super::eval::Context, flow: Flow) -> EvalResult {
    match flow {
        Flow::Signal(sig) => match eval.dispatch_signal_if_needed(sig) {
            Ok(dispatched) => Err(Flow::Signal(dispatched)),
            Err(flow) => Err(flow),
        },
        other => Err(other),
    }
}

/// `(error-message-string ERROR-DATA)` — format an error for display.
///
/// ERROR-DATA is `(ERROR-SYMBOL . DATA)` as bound by `condition-case`.
/// Looks up `error-message` on the symbol's plist and appends the data.
pub(crate) fn builtin_error_message_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("error-message-string"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    // GNU's `Ferror_message_string' (print.c:1046-1051) short-circuits an
    // error of exactly the shape (error STRING) and hands back that very
    // string object, text properties and all.  Every other shape is rendered
    // by printing into `prin1-to-string-buffer' one character at a time, which
    // carries no properties, and the result is read back out with
    // `buffer-string'.  So the fast path is the *only* way a property reaches
    // the caller: a `user-error' message, a file-error, or an (error STRING
    // EXTRA) all come back plain even when their payload was propertized.
    //
    // Routing every other answer through one strip keeps that guarantee in a
    // single place rather than leaving it to each of the branches below.
    if let Some(message) = single_error_string_payload(&args[0]) {
        return Ok(message);
    }
    let rendered = error_message_string_rendered(eval, &args[0])?;
    Ok(match eval.lisp_string(rendered) {
        Some(string) => Value::heap_string(string.without_properties()),
        None => rendered,
    })
}

/// The STRING of an error datum of exactly the shape `(error STRING)`, which
/// GNU returns unchanged.
fn single_error_string_payload(error_data: &Value) -> Option<Value> {
    if !error_data.is_cons() || error_data.cons_car() != Value::symbol("error") {
        return None;
    }
    let tail = error_data.cons_cdr();
    if !tail.is_cons() || !tail.cons_cdr().is_nil() {
        return None;
    }
    let message = tail.cons_car();
    message.as_lisp_string().is_some().then_some(message)
}

fn error_message_string_rendered(
    eval: &mut super::eval::Context,
    error_data: &Value,
) -> EvalResult {
    // Emacs expects ERROR-DATA to be a list (or nil).
    let (symbol, data) = match error_data.kind() {
        ValueKind::Cons => {
            let pair_car = error_data.cons_car();
            let pair_cdr = error_data.cons_cdr();
            let symbol = match pair_car.as_symbol_id() {
                Some(symbol) => symbol,
                None => return Ok(Value::string("peculiar error")),
            };
            let rest = error_data_tail_to_vec(pair_cdr);
            (symbol, rest)
        }
        ValueKind::Nil => return Ok(Value::heap_string(lisp_lit("peculiar error"))),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), *error_data],
            ));
        }
    };

    // GNU `print_error_message` treats a signal as printable when its
    // `error-message` property is a string.  This deliberately includes
    // non-error conditions such as `quit` and `minibuffer-quit`, whose
    // `error-conditions` do not include `error`.
    let error_symbol = intern("error");
    let user_error_symbol = intern("user-error");
    let Some(base_message_value) = eval
        .obarray
        .get_property_id(symbol, intern("error-message"))
    else {
        if data.is_empty() {
            return Ok(Value::heap_string(lisp_lit("peculiar error")));
        }
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(
            lisp_lit("peculiar error: ").concat(&detail),
        ));
    };

    // GNU `print_error_message` passes the `error-message` property of every
    // condition other than plain `error` through `substitute-command-keys`.
    // Besides command substitutions, this applies the user's
    // `text-quoting-style`.  The callback is intentionally a safe funcall:
    // error rendering must keep the original message if the Lisp helper
    // itself signals.  A non-nil, non-string callback result is retained and
    // consequently renders as a peculiar error, matching GNU's downstream
    // printer behavior.
    let base_message_value = if symbol != error_symbol
        && eval
            .obarray
            .symbol_function_id(intern("substitute-command-keys"))
            .is_some()
    {
        let substituted = eval.safe_funcall(
            Value::symbol("substitute-command-keys"),
            vec![base_message_value],
        )?;
        if substituted.is_nil() {
            base_message_value
        } else {
            substituted
        }
    } else {
        base_message_value
    };

    let Some(_) = base_message_value.as_lisp_string() else {
        if data.is_empty() {
            return Ok(Value::heap_string(lisp_lit("peculiar error")));
        }
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(
            lisp_lit("peculiar error: ").concat(&detail),
        ));
    };
    let base_message = error_arg_lisp(eval, &base_message_value, ErrorDatumPrintMode::Princ);

    if data.is_empty() {
        if symbol == error_symbol {
            return Ok(Value::heap_string(lisp_lit("peculiar error")));
        }
        if symbol == user_error_symbol {
            return Ok(Value::heap_string(lisp_lit("")));
        }
        return Ok(Value::heap_string(base_message));
    }

    // `user-error` always renders payload data directly.
    if symbol == user_error_symbol {
        if let Some(first) = data.first().filter(|v| v.as_lisp_string().is_some()) {
            let first_str = error_arg_lisp(eval, first, ErrorDatumPrintMode::Princ);
            let rest = &data[1..];
            if rest.is_empty() {
                return Ok(Value::heap_string(first_str));
            }
            let rest_j = join_lisp(
                rest.iter()
                    .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Princ))
                    .collect(),
                ", ",
            );
            return Ok(Value::heap_string(
                first_str.concat(&lisp_lit(", ")).concat(&rest_j),
            ));
        }
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Princ))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(detail));
    }

    let is_file_error_family = signal_matches_condition_value_sym(
        &eval.obarray,
        symbol,
        &Value::from_sym_id(intern("file-error")),
    );
    let is_file_locked = symbol == intern("file-locked");

    // `file-locked` is an oddball in Emacs: it always reports "peculiar error"
    // with all payload elements, even if the first datum is a string.
    if is_file_locked {
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(
            lisp_lit("peculiar error: ").concat(&detail),
        ));
    }

    // `error` and file-error-family conditions use a leading string for
    // user-facing detail.
    if symbol == error_symbol || is_file_error_family {
        if let Some(first) = data.first().filter(|v| v.as_lisp_string().is_some()) {
            let first_str = error_arg_lisp(eval, first, ErrorDatumPrintMode::Princ);
            let rest = &data[1..];
            if rest.is_empty() {
                return Ok(Value::heap_string(first_str));
            }
            let print_mode = if symbol == error_symbol {
                ErrorDatumPrintMode::Prin1
            } else {
                ErrorDatumPrintMode::Princ
            };
            let rest_j = join_lisp(
                rest.iter()
                    .map(|v| error_arg_lisp(eval, v, print_mode))
                    .collect(),
                ", ",
            );
            return Ok(Value::heap_string(
                first_str.concat(&lisp_lit(": ")).concat(&rest_j),
            ));
        }

        // `error` and most file-error-family members render peculiar payload
        // data from the second element onward when no leading message string
        // is present.
        if data.len() > 1 {
            let detail = join_lisp(
                data[1..]
                    .iter()
                    .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                    .collect(),
                ", ",
            );
            return Ok(Value::heap_string(
                lisp_lit("peculiar error: ").concat(&detail),
            ));
        }
        return Ok(Value::heap_string(lisp_lit("peculiar error")));
    }

    let print_mode = if symbol == intern("end-of-file") {
        ErrorDatumPrintMode::Princ
    } else {
        ErrorDatumPrintMode::Prin1
    };
    let detail = join_lisp(
        data.iter()
            .map(|v| error_arg_lisp(eval, v, print_mode))
            .collect(),
        ", ",
    );
    Ok(Value::heap_string(
        base_message.concat(&lisp_lit(": ")).concat(&detail),
    ))
}

fn error_data_tail_to_vec(mut tail: Value) -> Vec<Value> {
    let mut data = Vec::new();
    while tail.is_cons() {
        data.push(tail.cons_car());
        tail = tail.cons_cdr();
    }
    data
}

/// GNU `print_error_message` chooses between `Fprinc` and `Fprin1` for an
/// entire error-data object, not only for string quoting.  Keep that protocol
/// typed so a new condition branch must select the full object printer.
#[derive(Clone, Copy)]
enum ErrorDatumPrintMode {
    Princ,
    Prin1,
}

/// Render an error-data argument as faithful Emacs internal-encoding bytes.
/// Both printer paths are shared with the public Lisp primitives, so buffers,
/// nested strings, opaque handles, and raw unibyte data keep their GNU-visible
/// `princ`/`prin1` distinctions.
fn error_arg_lisp(
    eval: &super::eval::Context,
    value: &Value,
    mode: ErrorDatumPrintMode,
) -> crate::heap_types::LispString {
    let bytes = match mode {
        ErrorDatumPrintMode::Princ => {
            super::builtins::misc_eval::print_value_princ_bytes_to_multibyte_buffer(eval, value)
        }
        ErrorDatumPrintMode::Prin1 => {
            super::error::print_value_bytes_escaped_with_eval(eval, value)
        }
    };
    crate::heap_types::LispString::from_emacs_bytes(bytes)
}

/// A static ASCII message literal as a LispString piece. Built unibyte so it
/// does not force the concatenated result multibyte; `concat` promotes it when a
/// multibyte piece is present.
fn lisp_lit(s: &str) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_unibyte(s.as_bytes().to_vec())
}

/// Concatenate LispString `items` with `sep` between them, using Emacs string
/// concatenation (which unifies unibyte/multibyte pieces correctly).
fn join_lisp(
    items: Vec<crate::heap_types::LispString>,
    sep: &str,
) -> crate::heap_types::LispString {
    let sep_ls = lisp_lit(sep);
    let mut iter = items.into_iter();
    let Some(mut acc) = iter.next() else {
        return crate::heap_types::LispString::from_unibyte(Vec::new());
    };
    for item in iter {
        acc = acc.concat(&sep_ls).concat(&item);
    }
    acc
}

// ---------------------------------------------------------------------------
// ErrorRegistry (HashMap-based, standalone — usable without an Obarray)
// ---------------------------------------------------------------------------

/// A standalone registry that tracks error parent relationships.
///
/// This can be used independently of the obarray (e.g. for testing or
/// embedding).  For the full Emacs-compatible approach, prefer the
/// obarray-based functions above.
pub struct ErrorRegistry {
    /// Map from error symbol name to its parent error symbol names.
    parents: HashMap<SymId, Vec<SymId>>,
}

impl ErrorRegistry {
    /// Create a new registry pre-populated with the standard Emacs error
    /// hierarchy.
    pub fn new() -> Self {
        let mut reg = Self {
            parents: HashMap::new(),
        };
        reg.init_standard_hierarchy();
        reg
    }

    /// Register a new error type using symbol identity.
    pub fn define_error_sym(&mut self, name: SymId, _message: &str, parents: &[SymId]) {
        let parent_list = if parents.is_empty() {
            vec![intern("error")]
        } else {
            parents.to_vec()
        };
        self.parents.insert(name, parent_list);
    }

    /// Register a new error type.
    pub fn define_error(&mut self, name: &str, _message: &str, parents: &[&str]) {
        let name = intern(name);
        let parents: Vec<SymId> = parents.iter().map(|s| intern(s)).collect();
        self.define_error_sym(name, _message, &parents);
    }

    /// Check whether `signal` inherits from `condition` (directly or
    /// transitively).
    pub fn signal_matches_condition_sym(&self, signal_sym: SymId, condition: SymId) -> bool {
        if condition == intern("t") {
            return true;
        }
        if signal_sym == condition {
            return true;
        }
        let mut visited = HashSet::new();
        let mut stack = vec![signal_sym];
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(parents) = self.parents.get(&current) {
                for &parent in parents {
                    if parent == condition {
                        return true;
                    }
                    stack.push(parent);
                }
            }
        }
        false
    }

    pub fn signal_matches_condition(&self, signal_sym: &str, condition: &str) -> bool {
        self.signal_matches_condition_sym(intern(signal_sym), intern(condition))
    }

    /// Collect the full condition list for a signal (self + all ancestors).
    pub fn conditions_for_sym(&self, signal_sym: SymId) -> Vec<SymId> {
        let mut result = vec![signal_sym];
        let mut visited = HashSet::new();
        visited.insert(signal_sym);
        let mut stack = vec![signal_sym];
        while let Some(current) = stack.pop() {
            if let Some(parents) = self.parents.get(&current) {
                for &parent in parents {
                    if visited.insert(parent) {
                        result.push(parent);
                        stack.push(parent);
                    }
                }
            }
        }
        result
    }

    pub fn conditions_for(&self, signal_sym: &str) -> Vec<String> {
        self.conditions_for_sym(intern(signal_sym))
            .into_iter()
            .map(|sym| resolve_sym(sym).to_owned())
            .collect()
    }

    fn init_standard_hierarchy(&mut self) {
        // Root.
        self.parents.insert(intern("error"), vec![]);

        let simple_children_of_error = [
            "quit",
            "user-error",
            "args-out-of-range",
            "beginning-of-buffer",
            "end-of-buffer",
            "buffer-read-only",
            "coding-system-error",
            "invalid-function",
            "invalid-read-syntax",
            "invalid-regexp",
            "mark-inactive",
            "no-catch",
            "scan-error",
            "search-failed",
            "setting-constant",
            "void-function",
            "void-variable",
            "wrong-number-of-arguments",
            "wrong-type-argument",
            "cl-assertion-failed",
            "permission-denied",
            "recursion-error",
        ];
        for name in &simple_children_of_error {
            self.parents.insert(intern(name), vec![intern("error")]);
        }

        // GNU data.c makes text-read-only a subcondition of buffer-read-only
        // (not a direct child of error), so `(buffer-read-only ...)' handlers
        // catch a text-read-only signal.
        self.parents
            .insert(intern("text-read-only"), vec![intern("buffer-read-only")]);

        // arith-error family.
        self.parents
            .insert(intern("arith-error"), vec![intern("error")]);
        for name in &[
            "overflow-error",
            "range-error",
            "domain-error",
            "underflow-error",
        ] {
            self.parents
                .insert(intern(name), vec![intern("arith-error")]);
        }

        // file-error family.
        self.parents
            .insert(intern("file-error"), vec![intern("error")]);
        for name in &[
            "file-already-exists",
            "file-date-error",
            "file-locked",
            "file-missing",
            "file-notify-error",
        ] {
            self.parents
                .insert(intern(name), vec![intern("file-error")]);
        }
        // No `dbus-error' parent: it is `#ifdef HAVE_DBUS' in GNU
        // (src/dbusbind.c:2013-2017) and this build has no D-Bus transport.
        // `lisp/net/dbus.el:51' defines it when dbus.el loads.  Ledger 192.

        // json-error family (mirrors GNU src/json.c `syms_of_json`).
        self.parents
            .insert(intern("json-error"), vec![intern("error")]);
        for name in &[
            "json-out-of-memory",
            "json-parse-error",
            "json-object-too-deep",
            "json-utf8-decode-error",
            "json-invalid-surrogate-error",
            "json-number-out-of-range-error",
            // Neomacs extension (GNU uses a plain `error` here).
            "json-serialize-error",
        ] {
            self.parents
                .insert(intern(name), vec![intern("json-error")]);
        }
        // These are children of json-parse-error in GNU, not json-error.
        for name in &[
            "json-end-of-file",
            "json-trailing-content",
            "json-escape-sequence-error",
        ] {
            self.parents
                .insert(intern(name), vec![intern("json-parse-error")]);
        }

        // remote-file-error is a child of file-error.
        self.parents
            .insert(intern("remote-file-error"), vec![intern("file-error")]);
    }
}

impl Default for ErrorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
