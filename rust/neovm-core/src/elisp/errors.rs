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

use super::error::{signal, EvalResult, Flow};
use super::expr::Expr;
use super::symbol::Obarray;
use super::value::*;

// ---------------------------------------------------------------------------
// Obarray-based error hierarchy helpers
// ---------------------------------------------------------------------------

/// Set `error-conditions` and `error-message` properties on `name` in the
/// obarray.  `conditions` is the full list of condition symbols (including
/// `name` itself, its parents, and their transitive ancestors).
fn put_error_properties(obarray: &mut Obarray, name: &str, message: &str, conditions: Vec<&str>) {
    let cond_list = Value::list(conditions.iter().map(|s| Value::symbol(*s)).collect());
    obarray.put_property(name, "error-conditions", cond_list);
    obarray.put_property(name, "error-message", Value::string(message));
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
            for sym in iter_symbol_list(parent_conds) {
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
        for sym_name in iter_symbol_list(conds) {
            if sym_name == condition_sym {
                return true;
            }
        }
    }
    false
}

/// Like `signal_matches_hierarchical` but accepts a condition pattern that may
/// be a single symbol or a list of symbols (as used by `condition-case`
/// handler heads).
pub fn signal_matches_condition_pattern(
    obarray: &Obarray,
    signal_sym: &str,
    pattern: &Expr,
) -> bool {
    match pattern {
        Expr::Symbol(name) => signal_matches_hierarchical(obarray, signal_sym, name),
        Expr::List(items) => items
            .iter()
            .any(|item| signal_matches_condition_pattern(obarray, signal_sym, item)),
        _ => false,
    }
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

    // --- Direct children of `error` ---

    register_simple(obarray, "quit", "Quit", &["error"]);
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
        "mark-inactive",
        "The mark is not active now",
        &["error"],
    );
    register_simple(obarray, "no-catch", "No catch for tag", &["error"]);
    register_simple(obarray, "scan-error", "Scan error", &["error"]);
    register_simple(obarray, "search-failed", "Search failed", &["error"]);
    register_simple(
        obarray,
        "setting-constant",
        "Attempt to set a constant symbol",
        &["error"],
    );
    register_simple(obarray, "text-read-only", "Text is read-only", &["error"]);
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
    register_simple(
        obarray,
        "permission-denied",
        "Permission denied",
        &["error"],
    );
    register_simple(
        obarray,
        "recursion-error",
        "Excessive recursive calling",
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

    // --- json-error family ---
    register_simple(obarray, "json-error", "JSON error", &["error"]);
    register_simple(
        obarray,
        "json-parse-error",
        "JSON parse error",
        &["json-error"],
    );
    register_simple(
        obarray,
        "json-serialize-error",
        "JSON serialize error",
        &["json-error"],
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
        "Lisp nesting exceeds maximum",
        &["error"],
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
// Special form: (define-error NAME MESSAGE &optional PARENT)
// ---------------------------------------------------------------------------

/// `(define-error NAME MESSAGE &optional PARENT)`
///
/// Register a new error type.  NAME is a symbol (evaluated).  MESSAGE is a
/// string (evaluated).  PARENT defaults to `error`; it may be a single symbol
/// or a list of symbols.
///
/// Sets `error-conditions` and `error-message` on NAME's plist in the obarray.
pub(crate) fn sf_define_error(eval: &mut super::eval::Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() || tail.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("define-error"), Value::Int(tail.len() as i64)],
        ));
    }

    // Evaluate NAME — must be a symbol.
    let name_val = eval.eval(&tail[0])?;
    let name = match &name_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), name_val],
            ));
        }
    };

    // Evaluate MESSAGE — must be a string.
    let message_val = if tail.len() > 1 {
        eval.eval(&tail[1])?
    } else {
        Value::string("")
    };
    let message = match &message_val {
        Value::Str(s) => (**s).clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), message_val],
            ));
        }
    };

    // Evaluate PARENT — defaults to 'error.  May be a symbol or list of symbols.
    let parents: Vec<String> = if tail.len() > 2 {
        let parent_val = eval.eval(&tail[2])?;
        extract_parent_symbols(&parent_val)?
    } else {
        vec!["error".to_string()]
    };

    // Build conditions from the obarray.
    let parent_refs: Vec<&str> = parents.iter().map(|s| s.as_str()).collect();
    let conditions = build_conditions_from_obarray(&eval.obarray, &name, &parent_refs);
    let cond_refs: Vec<&str> = conditions.iter().map(|s| s.as_str()).collect();
    put_error_properties(&mut eval.obarray, &name, &message, cond_refs);

    Ok(name_val)
}

/// Extract parent symbol(s) from the PARENT argument of `define-error`.
/// Accepts either a single symbol or a list of symbols.
fn extract_parent_symbols(value: &Value) -> Result<Vec<String>, Flow> {
    match value {
        Value::Symbol(s) => Ok(vec![s.clone()]),
        Value::Nil => Ok(vec!["error".to_string()]),
        Value::True => Ok(vec!["t".to_string()]),
        Value::Cons(_) => {
            let items = list_to_vec(value).ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), value.clone()],
                )
            })?;
            let mut parents = Vec::with_capacity(items.len());
            for item in &items {
                match item.as_symbol_name() {
                    Some(name) => parents.push(name.to_string()),
                    None => {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("symbolp"), item.clone()],
                        ));
                    }
                }
            }
            if parents.is_empty() {
                Ok(vec!["error".to_string()])
            } else {
                Ok(parents)
            }
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins: signal wrapper and error-message-string
// ---------------------------------------------------------------------------

/// `(signal ERROR-SYMBOL DATA)` — signal an error.
///
/// This is a wrapper that can be registered as a builtin.  It extracts the
/// error symbol name and signals with the provided data.
pub(crate) fn builtin_signal(args: Vec<Value>) -> EvalResult {
    if args.len() != 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("signal"), Value::Int(args.len() as i64)],
        ));
    }

    let sym_name = match args[0].as_symbol_name() {
        Some(name) => name.to_string(),
        None => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), args[0].clone()],
            ));
        }
    };

    // DATA should be a list — extract its elements.
    let data = match &args[1] {
        Value::Nil => vec![],
        Value::Cons(_) => list_to_vec(&args[1]).unwrap_or_else(|| vec![args[1].clone()]),
        other => vec![other.clone()],
    };

    Err(signal(&sym_name, data))
}

/// `(error-message-string ERROR-DATA)` — format an error for display.
///
/// ERROR-DATA is `(ERROR-SYMBOL . DATA)` as bound by `condition-case`.
/// Looks up `error-message` on the symbol's plist and appends the data.
pub(crate) fn builtin_error_message_string(
    eval: &super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("error-message-string"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let error_data = &args[0];

    // error_data should be (SYMBOL . DATA) — a cons cell.
    let (sym_name, data) = match error_data {
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            let sym = match pair.car.as_symbol_name() {
                Some(name) => name.to_string(),
                None => return Ok(Value::string("peculiar error")),
            };
            let rest = match &pair.cdr {
                Value::Nil => vec![],
                Value::Cons(_) => list_to_vec(&pair.cdr).unwrap_or_else(|| vec![pair.cdr.clone()]),
                other => vec![other.clone()],
            };
            (sym, rest)
        }
        Value::Symbol(s) => (s.clone(), vec![]),
        Value::Nil => return Ok(Value::string("peculiar error")),
        Value::True => return Ok(Value::string("peculiar error")),
        _ => return Ok(Value::string("peculiar error")),
    };

    // Look up the error-message property.
    let base_message = eval
        .obarray
        .get_property(&sym_name, "error-message")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| sym_name.clone());

    if data.is_empty() {
        return Ok(Value::string(base_message));
    }

    // `user-error` always renders payload data directly.
    if sym_name == "user-error" {
        if let Some(first_str) = data.first().and_then(|v| v.as_str().map(|s| s.to_string())) {
            let rest = &data[1..];
            if rest.is_empty() {
                return Ok(Value::string(first_str));
            }
            let rest_strs: Vec<String> = rest.iter().map(|v| format_error_arg(v, false)).collect();
            return Ok(Value::string(format!("{first_str}, {}", rest_strs.join(", "))));
        }
        let data_strs: Vec<String> = data.iter().map(|v| format_error_arg(v, false)).collect();
        return Ok(Value::string(data_strs.join(", ")));
    }

    let is_file_error_family = signal_matches_hierarchical(&eval.obarray, &sym_name, "file-error");

    // `error` and file-error-family conditions use a leading string for
    // user-facing detail.
    if sym_name == "error" || is_file_error_family {
        if let Some(first_str) = data.first().and_then(|v| v.as_str().map(|s| s.to_string())) {
            let rest = &data[1..];
            if rest.is_empty() {
                return Ok(Value::string(first_str));
            }
            let quote_strings = sym_name == "error";
            let rest_strs: Vec<String> = rest
                .iter()
                .map(|v| format_error_arg(v, quote_strings))
                .collect();
            return Ok(Value::string(format!("{first_str}: {}", rest_strs.join(", "))));
        }
        // Exact `error`/`file-error` without a leading string keep Emacs'
        // "peculiar error" behavior.
        if sym_name == "error" || sym_name == "file-error" {
            if let Some(detail) = data.get(1) {
                return Ok(Value::string(format!(
                    "peculiar error: {}",
                    format_error_arg(detail, true)
                )));
            }
            return Ok(Value::string("peculiar error"));
        }
    }

    let data_strs: Vec<String> = data
        .iter()
        .map(|v| format_error_arg(v, true))
        .collect();
    Ok(Value::string(format!(
        "{}: {}",
        base_message,
        data_strs.join(", ")
    )))
}

fn format_error_arg(value: &Value, quote_strings: bool) -> String {
    if !quote_strings {
        if let Some(s) = value.as_str() {
            return s.to_string();
        }
    }
    super::print::print_value(value)
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
    parents: std::collections::HashMap<String, Vec<String>>,
}

impl ErrorRegistry {
    /// Create a new registry pre-populated with the standard Emacs error
    /// hierarchy.
    pub fn new() -> Self {
        let mut reg = Self {
            parents: std::collections::HashMap::new(),
        };
        reg.init_standard_hierarchy();
        reg
    }

    /// Register a new error type.
    pub fn define_error(&mut self, name: &str, _message: &str, parents: &[&str]) {
        let parent_list = if parents.is_empty() {
            vec!["error".to_string()]
        } else {
            parents.iter().map(|s| s.to_string()).collect()
        };
        self.parents.insert(name.to_string(), parent_list);
    }

    /// Check whether `signal` inherits from `condition` (directly or
    /// transitively).
    pub fn signal_matches_condition(&self, signal_sym: &str, condition: &str) -> bool {
        if condition == "t" {
            return true;
        }
        if signal_sym == condition {
            return true;
        }
        // BFS/DFS through parents.
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![signal_sym.to_string()];
        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(parents) = self.parents.get(&current) {
                for parent in parents {
                    if parent == condition {
                        return true;
                    }
                    stack.push(parent.clone());
                }
            }
        }
        false
    }

    /// Collect the full condition list for a signal (self + all ancestors).
    pub fn conditions_for(&self, signal_sym: &str) -> Vec<String> {
        let mut result = vec![signal_sym.to_string()];
        let mut visited = std::collections::HashSet::new();
        visited.insert(signal_sym.to_string());
        let mut stack = vec![signal_sym.to_string()];
        while let Some(current) = stack.pop() {
            if let Some(parents) = self.parents.get(&current) {
                for parent in parents {
                    if visited.insert(parent.clone()) {
                        result.push(parent.clone());
                        stack.push(parent.clone());
                    }
                }
            }
        }
        result
    }

    fn init_standard_hierarchy(&mut self) {
        // Root.
        self.parents.insert("error".to_string(), vec![]);

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
            "text-read-only",
            "void-function",
            "void-variable",
            "wrong-number-of-arguments",
            "wrong-type-argument",
            "cl-assertion-failed",
            "permission-denied",
            "recursion-error",
        ];
        for name in &simple_children_of_error {
            self.parents
                .insert(name.to_string(), vec!["error".to_string()]);
        }

        // arith-error family.
        self.parents
            .insert("arith-error".to_string(), vec!["error".to_string()]);
        for name in &[
            "overflow-error",
            "range-error",
            "domain-error",
            "underflow-error",
        ] {
            self.parents
                .insert(name.to_string(), vec!["arith-error".to_string()]);
        }

        // file-error family.
        self.parents
            .insert("file-error".to_string(), vec!["error".to_string()]);
        for name in &[
            "file-already-exists",
            "file-date-error",
            "file-locked",
            "file-missing",
            "file-notify-error",
        ] {
            self.parents
                .insert(name.to_string(), vec!["file-error".to_string()]);
        }

        // json-error family.
        self.parents
            .insert("json-error".to_string(), vec!["error".to_string()]);
        for name in &["json-parse-error", "json-serialize-error"] {
            self.parents
                .insert(name.to_string(), vec!["json-error".to_string()]);
        }

        // remote-file-error is a child of file-error.
        self.parents.insert(
            "remote-file-error".to_string(),
            vec!["file-error".to_string()],
        );
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
mod tests {
    use super::*;

    // =======================================================================
    // ErrorRegistry (standalone HashMap-based) tests
    // =======================================================================

    #[test]
    fn registry_new_has_standard_errors() {
        let reg = ErrorRegistry::new();
        assert!(reg.parents.contains_key("error"));
        assert!(reg.parents.contains_key("void-variable"));
        assert!(reg.parents.contains_key("file-missing"));
        assert!(reg.parents.contains_key("overflow-error"));
    }

    #[test]
    fn registry_direct_match() {
        let reg = ErrorRegistry::new();
        assert!(reg.signal_matches_condition("void-variable", "void-variable"));
    }

    #[test]
    fn registry_parent_match() {
        let reg = ErrorRegistry::new();
        assert!(reg.signal_matches_condition("void-variable", "error"));
    }

    #[test]
    fn registry_grandparent_match() {
        let reg = ErrorRegistry::new();
        // overflow-error -> arith-error -> error
        assert!(reg.signal_matches_condition("overflow-error", "arith-error"));
        assert!(reg.signal_matches_condition("overflow-error", "error"));
    }

    #[test]
    fn registry_no_match() {
        let reg = ErrorRegistry::new();
        assert!(!reg.signal_matches_condition("void-variable", "void-function"));
        assert!(!reg.signal_matches_condition("void-variable", "arith-error"));
    }

    #[test]
    fn registry_t_catches_all() {
        let reg = ErrorRegistry::new();
        assert!(reg.signal_matches_condition("void-variable", "t"));
        assert!(reg.signal_matches_condition("file-missing", "t"));
        assert!(reg.signal_matches_condition("error", "t"));
    }

    #[test]
    fn registry_define_error_custom() {
        let mut reg = ErrorRegistry::new();
        reg.define_error("my-error", "My custom error", &["user-error"]);
        assert!(reg.signal_matches_condition("my-error", "user-error"));
        assert!(reg.signal_matches_condition("my-error", "error"));
        assert!(!reg.signal_matches_condition("my-error", "file-error"));
    }

    #[test]
    fn registry_define_error_multiple_parents() {
        let mut reg = ErrorRegistry::new();
        reg.define_error("hybrid-error", "Hybrid", &["file-error", "arith-error"]);
        assert!(reg.signal_matches_condition("hybrid-error", "file-error"));
        assert!(reg.signal_matches_condition("hybrid-error", "arith-error"));
        assert!(reg.signal_matches_condition("hybrid-error", "error"));
    }

    #[test]
    fn registry_conditions_for() {
        let reg = ErrorRegistry::new();
        let conds = reg.conditions_for("file-missing");
        assert!(conds.contains(&"file-missing".to_string()));
        assert!(conds.contains(&"file-error".to_string()));
        assert!(conds.contains(&"error".to_string()));
    }

    #[test]
    fn registry_file_error_family() {
        let reg = ErrorRegistry::new();
        for child in &[
            "file-already-exists",
            "file-date-error",
            "file-locked",
            "file-missing",
            "file-notify-error",
        ] {
            assert!(
                reg.signal_matches_condition(child, "file-error"),
                "{} should match file-error",
                child
            );
            assert!(
                reg.signal_matches_condition(child, "error"),
                "{} should match error",
                child
            );
        }
    }

    #[test]
    fn registry_json_error_family() {
        let reg = ErrorRegistry::new();
        assert!(reg.signal_matches_condition("json-parse-error", "json-error"));
        assert!(reg.signal_matches_condition("json-serialize-error", "json-error"));
        assert!(reg.signal_matches_condition("json-parse-error", "error"));
    }

    #[test]
    fn registry_remote_file_error_inherits_file_error() {
        let reg = ErrorRegistry::new();
        assert!(reg.signal_matches_condition("remote-file-error", "file-error"));
        assert!(reg.signal_matches_condition("remote-file-error", "error"));
    }

    // =======================================================================
    // Obarray-based hierarchy tests
    // =======================================================================

    #[test]
    fn obarray_init_standard_errors() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        // Check that error-conditions is set for 'error' itself.
        let conds = ob.get_property("error", "error-conditions").unwrap();
        let items = iter_symbol_list(conds);
        assert_eq!(items, vec!["error"]);
    }

    #[test]
    fn obarray_void_variable_conditions() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        let conds = ob
            .get_property("void-variable", "error-conditions")
            .unwrap();
        let items = iter_symbol_list(conds);
        assert!(items.contains(&"void-variable".to_string()));
        assert!(items.contains(&"error".to_string()));
    }

    #[test]
    fn obarray_overflow_error_conditions() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        let conds = ob
            .get_property("overflow-error", "error-conditions")
            .unwrap();
        let items = iter_symbol_list(conds);
        assert!(items.contains(&"overflow-error".to_string()));
        assert!(items.contains(&"arith-error".to_string()));
        assert!(items.contains(&"error".to_string()));
    }

    #[test]
    fn obarray_file_missing_conditions() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        let conds = ob.get_property("file-missing", "error-conditions").unwrap();
        let items = iter_symbol_list(conds);
        assert!(items.contains(&"file-missing".to_string()));
        assert!(items.contains(&"file-error".to_string()));
        assert!(items.contains(&"error".to_string()));
    }

    #[test]
    fn obarray_hierarchical_match() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        assert!(signal_matches_hierarchical(&ob, "void-variable", "error"));
        assert!(signal_matches_hierarchical(
            &ob,
            "overflow-error",
            "arith-error"
        ));
        assert!(signal_matches_hierarchical(
            &ob,
            "file-missing",
            "file-error"
        ));
        assert!(!signal_matches_hierarchical(
            &ob,
            "void-variable",
            "arith-error"
        ));
    }

    #[test]
    fn obarray_hierarchical_match_exact() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        assert!(signal_matches_hierarchical(
            &ob,
            "void-variable",
            "void-variable"
        ));
    }

    #[test]
    fn obarray_hierarchical_match_t() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        assert!(signal_matches_hierarchical(&ob, "void-variable", "t"));
    }

    #[test]
    fn obarray_error_message_property() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        let msg = ob.get_property("void-variable", "error-message").unwrap();
        assert_eq!(msg.as_str(), Some("Symbol's value as variable is void"));
    }

    #[test]
    fn obarray_condition_pattern_symbol() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        let pat = Expr::Symbol("error".to_string());
        assert!(signal_matches_condition_pattern(&ob, "void-variable", &pat));
    }

    #[test]
    fn obarray_condition_pattern_list() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        let pat = Expr::List(vec![
            Expr::Symbol("arith-error".to_string()),
            Expr::Symbol("file-error".to_string()),
        ]);
        assert!(signal_matches_condition_pattern(
            &ob,
            "overflow-error",
            &pat
        ));
        assert!(signal_matches_condition_pattern(&ob, "file-missing", &pat));
        assert!(!signal_matches_condition_pattern(
            &ob,
            "void-variable",
            &pat
        ));
    }

    #[test]
    fn obarray_unknown_signal_no_conditions() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);
        // A signal that was never registered — only exact match works.
        assert!(!signal_matches_hierarchical(&ob, "unknown-error", "error"));
        assert!(signal_matches_hierarchical(
            &ob,
            "unknown-error",
            "unknown-error"
        ));
        assert!(signal_matches_hierarchical(&ob, "unknown-error", "t"));
    }

    // =======================================================================
    // sf_define_error tests (via Evaluator)
    // =======================================================================

    #[test]
    fn sf_define_error_basic() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        // (define-error 'my-error "My error")
        let tail = vec![
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("my-error".to_string()),
            ]),
            Expr::Str("My error".to_string()),
        ];
        let result = sf_define_error(&mut evaluator, &tail);
        assert!(result.is_ok());

        // Check plist.
        let conds = evaluator
            .obarray
            .get_property("my-error", "error-conditions")
            .unwrap();
        let items = iter_symbol_list(conds);
        assert!(items.contains(&"my-error".to_string()));
        assert!(items.contains(&"error".to_string()));

        let msg = evaluator
            .obarray
            .get_property("my-error", "error-message")
            .unwrap();
        assert_eq!(msg.as_str(), Some("My error"));
    }

    #[test]
    fn sf_define_error_with_parent() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        // (define-error 'my-file-error "My file error" 'file-error)
        let tail = vec![
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("my-file-error".to_string()),
            ]),
            Expr::Str("My file error".to_string()),
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("file-error".to_string()),
            ]),
        ];
        let result = sf_define_error(&mut evaluator, &tail);
        assert!(result.is_ok());

        assert!(signal_matches_hierarchical(
            &evaluator.obarray,
            "my-file-error",
            "file-error"
        ));
        assert!(signal_matches_hierarchical(
            &evaluator.obarray,
            "my-file-error",
            "error"
        ));
    }

    #[test]
    fn sf_define_error_with_parent_list() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        // (define-error 'multi-error "Multi" '(file-error arith-error))
        let tail = vec![
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("multi-error".to_string()),
            ]),
            Expr::Str("Multi".to_string()),
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::List(vec![
                    Expr::Symbol("file-error".to_string()),
                    Expr::Symbol("arith-error".to_string()),
                ]),
            ]),
        ];
        let result = sf_define_error(&mut evaluator, &tail);
        assert!(result.is_ok());

        assert!(signal_matches_hierarchical(
            &evaluator.obarray,
            "multi-error",
            "file-error"
        ));
        assert!(signal_matches_hierarchical(
            &evaluator.obarray,
            "multi-error",
            "arith-error"
        ));
        assert!(signal_matches_hierarchical(
            &evaluator.obarray,
            "multi-error",
            "error"
        ));
    }

    #[test]
    fn sf_define_error_wrong_type_name() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        // (define-error 42 "Bad") — name is not a symbol.
        let tail = vec![Expr::Int(42), Expr::Str("Bad".to_string())];
        let result = sf_define_error(&mut evaluator, &tail);
        assert!(result.is_err());
    }

    #[test]
    fn sf_define_error_wrong_type_message() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        // (define-error 'foo 42) — message is not a string.
        let tail = vec![
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("foo".to_string()),
            ]),
            Expr::Int(42),
        ];
        let result = sf_define_error(&mut evaluator, &tail);
        assert!(result.is_err());
    }

    #[test]
    fn sf_define_error_too_many_args() {
        let mut evaluator = super::super::eval::Evaluator::new();

        let tail = vec![
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("x".to_string()),
            ]),
            Expr::Str("X".to_string()),
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol("error".to_string()),
            ]),
            Expr::Int(99), // extra arg
        ];
        let result = sf_define_error(&mut evaluator, &tail);
        assert!(result.is_err());
    }

    // =======================================================================
    // Builtin tests
    // =======================================================================

    #[test]
    fn builtin_signal_basic() {
        let args = vec![Value::symbol("void-variable"), Value::Nil];
        let result = builtin_signal(args);
        assert!(result.is_err());
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "void-variable");
                assert!(sig.data.is_empty());
            }
            _ => panic!("expected signal"),
        }
    }

    #[test]
    fn builtin_signal_with_data() {
        let data_list = Value::list(vec![Value::symbol("x")]);
        let args = vec![Value::symbol("void-variable"), data_list];
        let result = builtin_signal(args);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "void-variable");
                assert_eq!(sig.data.len(), 1);
            }
            _ => panic!("expected signal"),
        }
    }

    #[test]
    fn builtin_signal_wrong_arity() {
        let result = builtin_signal(vec![Value::symbol("error")]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_signal_non_symbol() {
        let result = builtin_signal(vec![Value::Int(42), Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_error_message_string_basic() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        // (error-message-string '(void-variable x))
        let err_data = Value::list(vec![Value::symbol("void-variable"), Value::symbol("x")]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.as_str(), Some("Symbol\u{2019}s value as variable is void: x"));
    }

    #[test]
    fn builtin_error_message_string_no_data() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        // (error-message-string '(arith-error))
        let err_data = Value::list(vec![Value::symbol("arith-error")]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.as_str(), Some("Arithmetic error"));
    }

    #[test]
    fn builtin_error_message_string_void_function_typography() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let err_data = Value::list(vec![Value::symbol("void-function"), Value::symbol("x")]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(
            msg.as_str(),
            Some("Symbol\u{2019}s function definition is void: x")
        );
    }

    #[test]
    fn builtin_error_message_string_unknown() {
        let evaluator = super::super::eval::Evaluator::new();

        // Unknown error symbol — falls back to symbol name.
        let err_data = Value::list(vec![Value::symbol("mystery-error")]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.as_str(), Some("mystery-error"));
    }

    #[test]
    fn builtin_error_message_string_error_with_string_payload() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let err_data = Value::list(vec![Value::symbol("error"), Value::string("abc")]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.as_str(), Some("abc"));
    }

    #[test]
    fn builtin_error_message_string_error_with_string_and_extra() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let err_data = Value::list(vec![
            Value::symbol("error"),
            Value::string("abc"),
            Value::Int(1),
        ]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.as_str(), Some("abc: 1"));
    }

    #[test]
    fn builtin_error_message_string_user_error_variants() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let with_string = Value::list(vec![
            Value::symbol("user-error"),
            Value::string("u"),
            Value::Int(1),
        ]);
        let with_string_result = builtin_error_message_string(&evaluator, vec![with_string]);
        assert!(with_string_result.is_ok());
        assert_eq!(with_string_result.unwrap().as_str(), Some("u, 1"));

        let non_string = Value::list(vec![
            Value::symbol("user-error"),
            Value::symbol("integerp"),
            Value::string("x"),
        ]);
        let non_string_result = builtin_error_message_string(&evaluator, vec![non_string]);
        assert!(non_string_result.is_ok());
        assert_eq!(non_string_result.unwrap().as_str(), Some("integerp, x"));
    }

    #[test]
    fn builtin_error_message_string_file_error_string_payload() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let err_data = Value::list(vec![
            Value::symbol("file-error"),
            Value::string("No such file"),
            Value::string("foo"),
        ]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.as_str(), Some("No such file: foo"));
    }

    #[test]
    fn builtin_error_message_string_file_missing_string_payload() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let err_data = Value::list(vec![
            Value::symbol("file-missing"),
            Value::string("Opening input file"),
            Value::string("No such file or directory"),
            Value::string("/tmp/probe"),
        ]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(
            msg.as_str(),
            Some("Opening input file: No such file or directory, /tmp/probe")
        );
    }

    #[test]
    fn builtin_error_message_string_peculiar_error_paths() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let error_single = Value::list(vec![Value::symbol("error"), Value::Int(1)]);
        let error_single_result = builtin_error_message_string(&evaluator, vec![error_single]);
        assert!(error_single_result.is_ok());
        assert_eq!(error_single_result.unwrap().as_str(), Some("peculiar error"));

        let error_double = Value::list(vec![Value::symbol("error"), Value::Int(1), Value::Int(2)]);
        let error_double_result = builtin_error_message_string(&evaluator, vec![error_double]);
        assert!(error_double_result.is_ok());
        assert_eq!(error_double_result.unwrap().as_str(), Some("peculiar error: 2"));

        let file_single = Value::list(vec![Value::symbol("file-error"), Value::Int(1)]);
        let file_single_result = builtin_error_message_string(&evaluator, vec![file_single]);
        assert!(file_single_result.is_ok());
        assert_eq!(file_single_result.unwrap().as_str(), Some("peculiar error"));

        let file_double = Value::list(vec![Value::symbol("file-error"), Value::Int(1), Value::Int(2)]);
        let file_double_result = builtin_error_message_string(&evaluator, vec![file_double]);
        assert!(file_double_result.is_ok());
        assert_eq!(file_double_result.unwrap().as_str(), Some("peculiar error: 2"));
    }

    #[test]
    fn builtin_error_message_string_args_out_of_range_uses_base_message() {
        let mut evaluator = super::super::eval::Evaluator::new();
        init_standard_errors(&mut evaluator.obarray);

        let err_data = Value::list(vec![
            Value::symbol("args-out-of-range"),
            Value::string("abc"),
            Value::Int(9),
        ]);
        let result = builtin_error_message_string(&evaluator, vec![err_data]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), Some("Args out of range: \"abc\", 9"));
    }

    #[test]
    fn builtin_error_message_string_not_cons() {
        let evaluator = super::super::eval::Evaluator::new();

        // Non-cons input — "peculiar error".
        let result = builtin_error_message_string(&evaluator, vec![Value::Int(42)]);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.as_str(), Some("peculiar error"));
    }

    #[test]
    fn builtin_error_message_string_wrong_arity() {
        let evaluator = super::super::eval::Evaluator::new();
        let result = builtin_error_message_string(&evaluator, vec![]);
        assert!(result.is_err());
    }

    // =======================================================================
    // Edge case / integration tests
    // =======================================================================

    #[test]
    fn obarray_define_then_match() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);

        // Manually define a custom error.
        let conds = build_conditions_from_obarray(&ob, "my-error", &["user-error"]);
        let cond_refs: Vec<&str> = conds.iter().map(|s| s.as_str()).collect();
        put_error_properties(&mut ob, "my-error", "My custom error", cond_refs);

        assert!(signal_matches_hierarchical(&ob, "my-error", "user-error"));
        assert!(signal_matches_hierarchical(&ob, "my-error", "error"));
        assert!(!signal_matches_hierarchical(&ob, "my-error", "file-error"));
    }

    #[test]
    fn obarray_deep_hierarchy() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);

        // level1 -> file-error -> error
        register_simple(&mut ob, "level1", "L1", &["file-error"]);
        // level2 -> level1 -> file-error -> error
        register_simple(&mut ob, "level2", "L2", &["level1"]);
        // level3 -> level2 -> level1 -> file-error -> error
        register_simple(&mut ob, "level3", "L3", &["level2"]);

        assert!(signal_matches_hierarchical(&ob, "level3", "level2"));
        assert!(signal_matches_hierarchical(&ob, "level3", "level1"));
        assert!(signal_matches_hierarchical(&ob, "level3", "file-error"));
        assert!(signal_matches_hierarchical(&ob, "level3", "error"));
        assert!(!signal_matches_hierarchical(&ob, "level3", "arith-error"));
    }

    #[test]
    fn obarray_all_standard_errors_have_message() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);

        let standard = [
            "error",
            "quit",
            "user-error",
            "args-out-of-range",
            "arith-error",
            "overflow-error",
            "range-error",
            "domain-error",
            "underflow-error",
            "beginning-of-buffer",
            "end-of-buffer",
            "buffer-read-only",
            "coding-system-error",
            "file-error",
            "file-already-exists",
            "file-date-error",
            "file-locked",
            "file-missing",
            "file-notify-error",
            "invalid-function",
            "invalid-read-syntax",
            "invalid-regexp",
            "mark-inactive",
            "no-catch",
            "scan-error",
            "search-failed",
            "setting-constant",
            "text-read-only",
            "void-function",
            "void-variable",
            "wrong-number-of-arguments",
            "wrong-type-argument",
            "cl-assertion-failed",
            "json-error",
            "json-parse-error",
            "json-serialize-error",
            "permission-denied",
            "remote-file-error",
            "recursion-error",
        ];

        for name in &standard {
            assert!(
                ob.get_property(name, "error-message").is_some(),
                "{} should have error-message",
                name
            );
            assert!(
                ob.get_property(name, "error-conditions").is_some(),
                "{} should have error-conditions",
                name
            );
        }
    }

    #[test]
    fn obarray_all_standard_errors_include_self_in_conditions() {
        let mut ob = Obarray::new();
        init_standard_errors(&mut ob);

        let standard = [
            "error",
            "void-variable",
            "overflow-error",
            "file-missing",
            "json-parse-error",
        ];

        for name in &standard {
            let conds = ob.get_property(name, "error-conditions").unwrap();
            let items = iter_symbol_list(conds);
            assert!(
                items.contains(&name.to_string()),
                "{} should contain itself in error-conditions",
                name
            );
        }
    }
}
