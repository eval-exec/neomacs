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

mod native_message;
use crate::tagged::SymbolMode;
#[doc(hidden)]
pub use native_message::CErrorMessage;

use crate::error::{Flow, signal_with_data, signal_with_data_id};
use crate::intern::{SymId, T_SYM_ID, intern};
use crate::symbol::Obarray;
use crate::value::ValueKind;
use crate::value::*;

// ---------------------------------------------------------------------------
// Obarray-based error hierarchy helpers
// ---------------------------------------------------------------------------

/// Set `error-conditions` and `error-message` properties on `name` in the
/// obarray.  `conditions` is the full list of condition symbols (including
/// `name` itself, its parents, and their transitive ancestors).
fn put_error_properties<I, S>(obarray: &mut Obarray, name: &str, message: &str, conditions: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // GNU `define-error` interns every condition name it publishes; the
    // port builds the transitive list by name, so publish each canonical id
    // as a member before storing the list.
    let mut values = Vec::new();
    for condition in conditions {
        let id = crate::intern::intern(condition.as_ref());
        obarray.ensure_interned_global_id(id);
        values.push(Value::from_sym_id(id));
    }
    let cond_list = Value::list(values);
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
            for sym in iter_symbol_list(&parent_conds, SymbolMode::Bare) {
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
fn iter_symbol_list(value: &Value, symbol_mode: SymbolMode) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(items) = list_to_vec(value) {
        for item in items {
            if let Some(name) = item.as_symbol_name(symbol_mode) {
                result.push(name.to_string());
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Hierarchical signal matching (for condition-case)
// ---------------------------------------------------------------------------

// Every caught signal reads this property (GNU's `Qerror_conditions`).
fn error_conditions_sym_id() -> SymId {
    static ID: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| intern("error-conditions"))
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
    symbol_mode: SymbolMode,
) -> bool {
    // The signal's own conditions, read by identity (falling back to just the
    // symbol itself, matching `signal_matches_hierarchical`'s exact-match rule).
    let conditions = obarray.get_property_id(signal_sym, error_conditions_sym_id());
    signal_matches_pattern_against_conditions(signal_sym, conditions.as_ref(), pattern, symbol_mode)
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
    symbol_mode: SymbolMode,
) -> bool {
    let conditions = obarray.get_property_id(signal_sym, error_conditions_sym_id());
    signal_matches_symbol_against_conditions(
        signal_sym,
        conditions.as_ref(),
        condition_sym,
        symbol_mode,
    )
}

fn signal_matches_pattern_against_conditions(
    signal_sym: SymId,
    conditions: Option<&Value>,
    pattern: &Value,
    symbol_mode: SymbolMode,
) -> bool {
    match pattern.kind() {
        ValueKind::T => true,
        ValueKind::Nil => false,
        ValueKind::Cons => list_to_vec(pattern).is_some_and(|items| {
            items.iter().any(|item| {
                signal_matches_pattern_against_conditions(signal_sym, conditions, item, symbol_mode)
            })
        }),
        _ => {
            let Some(cond_id) = crate::value::symbol_id(pattern, symbol_mode) else {
                return false;
            };
            signal_matches_symbol_against_conditions(signal_sym, conditions, cond_id, symbol_mode)
        }
    }
}

fn signal_matches_symbol_against_conditions(
    signal_sym: SymId,
    conditions: Option<&Value>,
    condition_sym: SymId,
    symbol_mode: SymbolMode,
) -> bool {
    if pattern_is_catch_all(condition_sym) || condition_sym == signal_sym {
        return true;
    }
    let Some(conditions) = conditions else {
        return false;
    };
    let mut tail = *conditions;
    while tail.is_cons() {
        if tail.cons_car().as_symbol_id(symbol_mode) == Some(condition_sym) {
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
    // GNU src/thread.c `syms_of_threads`.
    register_simple(
        obarray,
        "thread-buffer-killed",
        "Thread's current buffer killed",
        &["error"],
    );
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
    // GNU data.c: PUT_ERROR (Qinhibited_interaction, error_tail, ...).
    register_simple(
        obarray,
        "inhibited-interaction",
        "User interaction while inhibited",
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
    // `file-locked` is deliberately absent: GNU defines it in `userlock.el`
    // (`lisp/userlock.el:40`, `(define-error 'file-locked "File is locked"
    // 'file-error)`), which loadup does not preload, so the pinned GNU 31.1
    // answers `(intern-soft "file-locked")` with nil until the first file
    // locking autoloads that file. Eagerly registering it here made the
    // symbol visible at boot and changed `(intern-soft "file-locked")` /
    // `(all-completions "file-" obarray)`.
    register_simple(obarray, "file-missing", "File is missing", &["file-error"]);
    register_simple(
        obarray,
        "file-notify-error",
        "File notification error",
        &["file-error"],
    );
    std::cfg_select! {
        neomacs_have_dbus => register_dbus_error(obarray),
        // Without `HAVE_DBUS`, GNU's `lisp/net/dbus.el:50-51` supplies
        // `dbus-error` at load. Ledger 192.
        _ => {}
    }

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

    // --- dynamic module errors (GNU src/emacs-module.c `syms_of_module`) ---
    register_simple(
        obarray,
        "module-out-of-memory",
        "Module out of memory",
        &["error"],
    );
    register_simple(
        obarray,
        "module-load-failed",
        "Module load failed",
        &["error"],
    );
    register_simple(
        obarray,
        "module-open-failed",
        "Module could not be opened",
        &["module-load-failed"],
    );
    register_simple(
        obarray,
        "module-not-gpl-compatible",
        "Module is not GPL compatible",
        &["module-load-failed"],
    );
    register_simple(
        obarray,
        "missing-module-init-function",
        "Module does not export an initialization function",
        &["module-load-failed"],
    );
    register_simple(
        obarray,
        "module-init-failed",
        "Module initialization failed",
        &["module-load-failed"],
    );
    register_simple(
        obarray,
        "invalid-arity",
        "Invalid function arity",
        &["error"],
    );
    register_simple(
        obarray,
        "memory-buffer-too-small",
        "Memory buffer too small",
        &["error"],
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
#[doc(hidden)]
pub fn register_dbus_error(obarray: &mut Obarray) {
    register_simple(obarray, "dbus-error", "D-Bus error", &["error"]);
}

fn register_simple(obarray: &mut Obarray, name: &str, message: &str, parents: &[&str]) {
    let conditions = build_conditions_from_obarray(obarray, name, parents);
    put_error_properties(obarray, name, message, &conditions);
}

// ---------------------------------------------------------------------------
// Signal flow constructors
// ---------------------------------------------------------------------------

#[doc(hidden)]
pub fn build_signal_flow(symbol_name: &str, data: Value) -> Flow {
    signal_with_data(symbol_name, data)
}

/// Identity-preserving, signal-hook-suppressed variant of `build_signal_flow`.
#[doc(hidden)]
pub fn build_signal_flow_id_suppressed(symbol: SymId, data: Value) -> Flow {
    use crate::error::signal_internal_id;
    let normalized = list_to_vec(&data).unwrap_or_else(|| vec![data]);
    signal_internal_id(symbol, normalized, Some(data), true)
}

/// Identity-preserving variant of `build_signal_flow`.
#[doc(hidden)]
pub fn build_signal_flow_id(symbol: SymId, data: Value) -> Flow {
    signal_with_data_id(symbol, data)
}

// ---------------------------------------------------------------------------
// ErrorRegistry (HashMap-based, standalone — usable without an Obarray)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
