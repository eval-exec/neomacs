//! Subr/primitive introspection builtins.
//!
//! Provides type predicates and introspection for callable objects:
//! - `subrp`, `subr-name`, `subr-arity`
//! - `commandp`, `functionp`, `byte-code-function-p`, `closurep`
//! - `interpreted-function-p`, `special-form-p`, `macrop`
//! - `func-arity`, `indirect-function`

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Argument helpers
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

// ---------------------------------------------------------------------------
// Evaluator/public callable classification
// ---------------------------------------------------------------------------

/// Returns true if `name` is recognized by the evaluator's special-form
/// dispatch path.
///
/// This list mirrors `Evaluator::try_special_form()` in `eval.rs`.
fn is_evaluator_special_form_name(name: &str) -> bool {
    matches!(
        name,
        "quote"
            | "function"
            | "let"
            | "let*"
            | "setq"
            | "setq-local"
            | "if"
            | "and"
            | "or"
            | "cond"
            | "while"
            | "progn"
            | "prog1"
            | "lambda"
            | "defun"
            | "defvar"
            | "defconst"
            | "defmacro"
            | "funcall"
            | "catch"
            | "throw"
            | "unwind-protect"
            | "condition-case"
            | "interactive"
            | "declare"
            | "when"
            | "unless"
            | "defalias"
            | "provide"
            | "require"
            | "save-excursion"
            | "save-restriction"
            | "with-current-buffer"
            | "ignore-errors"
            | "dotimes"
            | "dolist"
            // Custom / defcustom
            | "defcustom"
            | "defgroup"
            | "setq-default"
            | "defvar-local"
            // Autoload
            | "autoload"
            | "eval-when-compile"
            | "eval-and-compile"
            | "declare-function"
            | "define-obsolete-function-alias"
            | "define-obsolete-variable-alias"
            | "make-obsolete"
            | "make-obsolete-variable"
            | "with-eval-after-load"
            // Error hierarchy
            | "define-error"
            // Pattern matching
            | "pcase"
            | "pcase-let"
            | "pcase-let*"
            | "pcase-dolist"
            // Generalized variables
            | "setf"
            | "push"
            | "pop"
            | "cl-incf"
            | "cl-decf"
            | "gv-define-simple-setter"
            | "gv-define-setter"
            // CL extended
            | "cl-defstruct"
            | "cl-loop"
            | "cl-destructuring-bind"
            | "cl-push"
            | "cl-pop"
            | "cl-pushnew"
            | "cl-assert"
            | "cl-check-type"
            | "cl-case"
            | "cl-ecase"
            | "cl-typecase"
            | "cl-etypecase"
            | "cl-block"
            | "cl-return-from"
            | "cl-dotimes"
            | "cl-dolist"
            | "cl-flet"
            | "cl-labels"
            | "cl-progv"
            // Reader/printer
            | "with-output-to-string"
            // Threading
            | "with-mutex"
            // Misc
            | "prog2"
            | "with-temp-buffer"
            | "save-current-buffer"
            | "track-mouse"
            | "with-syntax-table"
            // Mode definition
            | "define-minor-mode"
            | "define-derived-mode"
            | "define-generic-mode"
    )
}

/// Returns true for special forms exposed by `special-form-p`.
///
/// Emacs distinguishes evaluator internals from public special forms:
/// many evaluator-recognized constructs are macros/functions in user-visible
/// introspection.
fn is_public_special_form_name(name: &str) -> bool {
    matches!(
        name,
        "quote"
            | "function"
            | "let"
            | "let*"
            | "setq"
            | "if"
            | "and"
            | "or"
            | "cond"
            | "while"
            | "progn"
            | "prog1"
            | "defvar"
            | "defconst"
            | "catch"
            | "unwind-protect"
            | "condition-case"
            | "interactive"
            | "inline"
            | "save-excursion"
            | "save-restriction"
            | "save-current-buffer"
    )
}

pub(crate) fn is_special_form(name: &str) -> bool {
    is_public_special_form_name(name)
}

pub(crate) fn is_evaluator_macro_name(name: &str) -> bool {
    let is_macro = matches!(
        name,
        "when"
            | "unless"
            | "dotimes"
            | "dolist"
            | "with-temp-buffer"
            | "with-current-buffer"
            | "ignore-errors"
            | "setq-local"
            | "defvar-local"
            | "with-output-to-string"
            | "prog2"
            | "track-mouse"
            | "with-syntax-table"
            | "eval-when-compile"
            | "eval-and-compile"
            | "declare"
            | "with-eval-after-load"
    );
    debug_assert!(!is_macro || is_evaluator_special_form_name(name));
    is_macro
}

pub(crate) fn is_evaluator_callable_name(name: &str) -> bool {
    // These are evaluator-dispatched entries that still behave as normal
    // callable symbols in introspection (`fboundp`/`functionp`/`symbol-function`).
    matches!(name, "throw")
}

#[derive(Clone, Copy)]
struct FallbackMacroSpec {
    min: usize,
    max: Option<usize>,
}

fn fallback_macro_spec(name: &str) -> Option<FallbackMacroSpec> {
    match name {
        "when" | "unless" | "dotimes" | "dolist" => Some(FallbackMacroSpec { min: 1, max: None }),
        "with-current-buffer" | "with-syntax-table" | "with-eval-after-load" => {
            Some(FallbackMacroSpec { min: 1, max: None })
        }
        "ignore-errors"
        | "setq-local"
        | "with-temp-buffer"
        | "with-output-to-string"
        | "track-mouse"
        | "eval-when-compile"
        | "eval-and-compile" => Some(FallbackMacroSpec { min: 0, max: None }),
        "defvar-local" => Some(FallbackMacroSpec {
            min: 2,
            max: Some(3),
        }),
        "prog2" => Some(FallbackMacroSpec { min: 2, max: None }),
        _ => None,
    }
}

fn fallback_macro_params(spec: FallbackMacroSpec) -> LambdaParams {
    let required: Vec<String> = (0..spec.min).map(|idx| format!("arg{idx}")).collect();
    let (optional, rest) = match spec.max {
        None => (Vec::new(), Some("rest".to_string())),
        Some(max) => {
            debug_assert!(max >= spec.min);
            let optional_count = max.saturating_sub(spec.min);
            let optional = (0..optional_count)
                .map(|idx| format!("arg{}", spec.min + idx))
                .collect();
            (optional, None)
        }
    };

    LambdaParams {
        required,
        optional,
        rest,
    }
}

/// Return a placeholder macro object for evaluator-integrated macro names.
///
/// This keeps `fboundp`/`symbol-function`/`indirect-function`/`macrop`
/// introspection aligned with Emacs for core macros even when they are not
/// materialized via Elisp bootstrap code in the function cell.
pub(crate) fn fallback_macro_value(name: &str) -> Option<Value> {
    let spec = fallback_macro_spec(name)?;
    Some(Value::Macro(Arc::new(LambdaData {
        params: fallback_macro_params(spec),
        body: vec![],
        env: None,
        docstring: None,
    })))
}

// ---------------------------------------------------------------------------
// Arity helpers
// ---------------------------------------------------------------------------

/// Build a cons cell `(MIN . MAX)` representing arity.
/// `max` of `None` means "many" (unbounded &rest), represented by the
/// symbol `many`.
fn arity_cons(min: usize, max: Option<usize>) -> Value {
    let min_val = Value::Int(min as i64);
    let max_val = match max {
        Some(n) => Value::Int(n as i64),
        None => Value::symbol("many"),
    };
    Value::cons(min_val, max_val)
}

fn subr_arity_value(name: &str) -> Value {
    match name {
        // Oracle-compatible overrides for core subrs used in vm-compat.
        "car" | "cdr" => arity_cons(1, Some(1)),
        "message" => arity_cons(1, None),
        "if" => Value::cons(Value::Int(2), Value::symbol("unevalled")),
        _ => arity_cons(0, None),
    }
}

fn is_macro_object(value: &Value) -> bool {
    match value {
        Value::Macro(_) => true,
        Value::Cons(cell) => cell.lock().expect("poisoned").car.as_symbol_name() == Some("macro"),
        _ => false,
    }
}

fn autoload_macro_marker(value: &Value) -> Option<Value> {
    if !super::autoload::is_autoload_value(value) {
        return None;
    }

    let items = list_to_vec(value)?;
    let autoload_type = items.get(4)?;
    if autoload_type.as_symbol_name() == Some("macro") {
        Some(Value::list(vec![Value::symbol("macro"), Value::True]))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Pure builtins (no evaluator access)
// ---------------------------------------------------------------------------

/// `(subrp OBJECT)` -- return t if OBJECT is a built-in function (Value::Subr).
pub(crate) fn builtin_subrp(args: Vec<Value>) -> EvalResult {
    expect_args("subrp", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Subr(_))))
}

/// `(subr-name SUBR)` -- return the name of a subroutine as a string.
pub(crate) fn builtin_subr_name(args: Vec<Value>) -> EvalResult {
    expect_args("subr-name", &args, 1)?;
    match &args[0] {
        Value::Subr(name) => Ok(Value::string(name.clone())),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("subrp"), other.clone()],
        )),
    }
}

/// `(subr-arity SUBR)` -- return (MIN . MAX) cons cell for argument counts.
///
/// Built-in subrs are dispatched by name and we do not yet have complete
/// per-subr metadata in NeoVM, so this is a partial compatibility table:
/// known shapes are special-cased and other subrs default to `(0 . many)`.
pub(crate) fn builtin_subr_arity(args: Vec<Value>) -> EvalResult {
    expect_args("subr-arity", &args, 1)?;
    match &args[0] {
        Value::Subr(name) => Ok(subr_arity_value(name)),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("subrp"), other.clone()],
        )),
    }
}

/// `(subr-native-elisp-p OBJECT)` -- return t if OBJECT is a native-compiled
/// Elisp subr.
///
/// NeoVM does not currently model native-compiled Elisp subrs, so this always
/// returns nil.
pub(crate) fn builtin_subr_native_elisp_p(args: Vec<Value>) -> EvalResult {
    expect_args("subr-native-elisp-p", &args, 1)?;
    Ok(Value::Nil)
}

/// `(subr-primitive-p OBJECT)` -- return t if OBJECT is a primitive subr.
pub(crate) fn builtin_subr_primitive_p(args: Vec<Value>) -> EvalResult {
    expect_args("subr-primitive-p", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Subr(_))))
}

/// `(functionp OBJECT)` -- return t if OBJECT is a function.
///
/// A function is one of: lambda, subr, or byte-compiled function.
pub(crate) fn builtin_functionp(args: Vec<Value>) -> EvalResult {
    expect_args("functionp", &args, 1)?;
    Ok(Value::bool(args[0].is_function()))
}

/// `(byte-code-function-p OBJECT)` -- return t if OBJECT is a byte-compiled
/// function.
pub(crate) fn builtin_byte_code_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("byte-code-function-p", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::ByteCode(_))))
}

/// `(closurep OBJECT)` -- return t if OBJECT is a closure.
///
/// A closure is a Lambda with a captured lexical environment.
pub(crate) fn builtin_closurep(args: Vec<Value>) -> EvalResult {
    expect_args("closurep", &args, 1)?;
    let is_closure = match &args[0] {
        Value::Lambda(l) => l.env.is_some(),
        _ => false,
    };
    Ok(Value::bool(is_closure))
}

/// `(interpreted-function-p OBJECT)` -- return t if OBJECT is an interpreted
/// function (a Lambda that is NOT byte-compiled).
///
/// In our VM, any `Value::Lambda` is interpreted (as opposed to
/// `Value::ByteCode`).
pub(crate) fn builtin_interpreted_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("interpreted-function-p", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Lambda(_))))
}

/// `(special-form-p OBJECT)` -- return t if OBJECT is a symbol that names a
/// special form.
///
/// Accepts a symbol (including nil/t) and checks it against the evaluator's
/// special-form table.
pub(crate) fn builtin_special_form_p(args: Vec<Value>) -> EvalResult {
    expect_args("special-form-p", &args, 1)?;
    let result = match &args[0] {
        Value::Symbol(name) => is_public_special_form_name(name),
        Value::Subr(name) => is_public_special_form_name(name),
        _ => false,
    };
    Ok(Value::bool(result))
}

/// `(macrop OBJECT)` -- return t if OBJECT is a macro.
pub(crate) fn builtin_macrop(args: Vec<Value>) -> EvalResult {
    expect_args("macrop", &args, 1)?;
    if let Some(marker) = autoload_macro_marker(&args[0]) {
        return Ok(marker);
    }
    Ok(Value::bool(is_macro_object(&args[0])))
}

/// `(commandp FUNCTION &optional FOR-CALL-INTERACTIVELY)` -- return t if
/// FUNCTION is an interactive command.
///
/// In our simplified VM, any callable value (lambda, subr, bytecode) is
/// treated as a potential command.  A more complete implementation would
/// check for an `interactive` declaration.
pub(crate) fn builtin_commandp(args: Vec<Value>) -> EvalResult {
    expect_min_args("commandp", &args, 1)?;
    Ok(Value::bool(args[0].is_function()))
}

/// `(func-arity FUNCTION)` -- return (MIN . MAX) for any callable.
///
/// Works for lambdas (reads `LambdaParams`), byte-code (reads `params`),
/// and subrs (returns `(0 . many)` as a conservative default).
pub(crate) fn builtin_func_arity(args: Vec<Value>) -> EvalResult {
    expect_args("func-arity", &args, 1)?;
    match &args[0] {
        Value::Lambda(l) => {
            let min = l.params.min_arity();
            let max = l.params.max_arity();
            Ok(arity_cons(min, max))
        }
        Value::ByteCode(bc) => {
            let min = bc.params.min_arity();
            let max = bc.params.max_arity();
            Ok(arity_cons(min, max))
        }
        Value::Subr(name) => Ok(subr_arity_value(name)),
        Value::Macro(m) => {
            let min = m.params.min_arity();
            let max = m.params.max_arity();
            Ok(arity_cons(min, max))
        }
        other => Err(signal("invalid-function", vec![other.clone()])),
    }
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// `(indirect-function OBJECT &optional NOERROR)` -- follow symbol function
/// indirection.
///
/// If OBJECT is a symbol, look up its function cell and follow any chain of
/// symbol aliases (defalias).  If OBJECT is not a symbol, return it as-is.
/// When the function cell is void and NOERROR is nil (the default), signal
/// a `void-function` error.
pub(crate) fn builtin_indirect_function(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("indirect-function", &args, 1)?;
    let noerror = args.get(1).map_or(false, |v| v.is_truthy());

    match &args[0] {
        Value::Symbol(name) => {
            if let Some(func) = eval.obarray.indirect_function(name) {
                Ok(func)
            } else if let Some(func) = eval.obarray.symbol_function(name) {
                // Direct function cell exists but is not a symbol chain.
                Ok(func.clone())
            } else if noerror {
                Ok(Value::Nil)
            } else {
                Err(signal("void-function", vec![Value::symbol(name.clone())]))
            }
        }
        // nil as a symbol
        Value::Nil => {
            if noerror {
                Ok(Value::Nil)
            } else {
                Err(signal("void-function", vec![Value::symbol("nil")]))
            }
        }
        // Non-symbol: return as-is (identity).
        other => Ok(other.clone()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::value::{LambdaData, LambdaParams};
    use std::sync::Arc;

    fn make_lambda(required: Vec<&str>, optional: Vec<&str>, rest: Option<&str>) -> Value {
        Value::Lambda(Arc::new(LambdaData {
            params: LambdaParams {
                required: required.into_iter().map(String::from).collect(),
                optional: optional.into_iter().map(String::from).collect(),
                rest: rest.map(String::from),
            },
            body: vec![],
            env: None,
            docstring: None,
        }))
    }

    fn make_closure(required: Vec<&str>) -> Value {
        use std::collections::HashMap;
        Value::Lambda(Arc::new(LambdaData {
            params: LambdaParams::simple(required.into_iter().map(String::from).collect()),
            body: vec![],
            env: Some(vec![HashMap::new()]),
            docstring: None,
        }))
    }

    fn make_macro(required: Vec<&str>) -> Value {
        Value::Macro(Arc::new(LambdaData {
            params: LambdaParams::simple(required.into_iter().map(String::from).collect()),
            body: vec![],
            env: None,
            docstring: None,
        }))
    }

    fn make_bytecode(required: Vec<&str>, rest: Option<&str>) -> Value {
        use crate::elisp::bytecode::ByteCodeFunction;
        let params = LambdaParams {
            required: required.into_iter().map(String::from).collect(),
            optional: vec![],
            rest: rest.map(String::from),
        };
        Value::ByteCode(Arc::new(ByteCodeFunction::new(params)))
    }

    // -- subrp --

    #[test]
    fn subrp_true_for_subr() {
        let result = builtin_subrp(vec![Value::Subr("car".into())]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn subrp_false_for_lambda() {
        let lam = make_lambda(vec![], vec![], None);
        let result = builtin_subrp(vec![lam]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn subrp_false_for_int() {
        let result = builtin_subrp(vec![Value::Int(42)]).unwrap();
        assert!(result.is_nil());
    }

    // -- subr-name --

    #[test]
    fn subr_name_returns_string() {
        let result = builtin_subr_name(vec![Value::Subr("cons".into())]).unwrap();
        assert_eq!(result.as_str(), Some("cons"));
    }

    #[test]
    fn subr_name_error_for_non_subr() {
        let result = builtin_subr_name(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    // -- subr-arity --

    #[test]
    fn subr_arity_returns_cons() {
        let result = builtin_subr_arity(vec![Value::Subr("+".into())]).unwrap();
        // Should be (0 . many)
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(0));
            assert_eq!(pair.cdr.as_symbol_name(), Some("many"));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn subr_arity_error_for_non_subr() {
        let result = builtin_subr_arity(vec![Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn subr_arity_message_is_one_or_more() {
        let result = builtin_subr_arity(vec![Value::Subr("message".into())]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(1));
            assert_eq!(pair.cdr.as_symbol_name(), Some("many"));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn subr_arity_if_is_unevalled() {
        let result = builtin_subr_arity(vec![Value::Subr("if".into())]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(2));
            assert_eq!(pair.cdr.as_symbol_name(), Some("unevalled"));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn subr_primitive_and_native_predicates() {
        let primitive = builtin_subr_primitive_p(vec![Value::Subr("car".into())]).unwrap();
        assert!(primitive.is_truthy());

        let non_subr = builtin_subr_primitive_p(vec![Value::Int(1)]).unwrap();
        assert!(non_subr.is_nil());

        let native = builtin_subr_native_elisp_p(vec![Value::Subr("car".into())]).unwrap();
        assert!(native.is_nil());
    }

    // -- functionp --

    #[test]
    fn functionp_true_for_lambda() {
        let lam = make_lambda(vec!["x"], vec![], None);
        let result = builtin_functionp(vec![lam]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn functionp_true_for_subr() {
        let result = builtin_functionp(vec![Value::Subr("car".into())]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn functionp_true_for_bytecode() {
        let bc = make_bytecode(vec![], None);
        let result = builtin_functionp(vec![bc]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn functionp_false_for_macro() {
        let m = make_macro(vec!["form"]);
        let result = builtin_functionp(vec![m]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn functionp_false_for_int() {
        let result = builtin_functionp(vec![Value::Int(5)]).unwrap();
        assert!(result.is_nil());
    }

    // -- byte-code-function-p --

    #[test]
    fn byte_code_function_p_true() {
        let bc = make_bytecode(vec!["a"], None);
        let result = builtin_byte_code_function_p(vec![bc]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn byte_code_function_p_false_for_lambda() {
        let lam = make_lambda(vec!["a"], vec![], None);
        let result = builtin_byte_code_function_p(vec![lam]).unwrap();
        assert!(result.is_nil());
    }

    // -- closurep --

    #[test]
    fn closurep_true_for_closure() {
        let cl = make_closure(vec!["x"]);
        let result = builtin_closurep(vec![cl]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn closurep_false_for_plain_lambda() {
        let lam = make_lambda(vec!["x"], vec![], None);
        let result = builtin_closurep(vec![lam]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn closurep_false_for_subr() {
        let result = builtin_closurep(vec![Value::Subr("car".into())]).unwrap();
        assert!(result.is_nil());
    }

    // -- interpreted-function-p --

    #[test]
    fn interpreted_function_p_true_for_lambda() {
        let lam = make_lambda(vec!["x"], vec![], None);
        let result = builtin_interpreted_function_p(vec![lam]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn interpreted_function_p_false_for_bytecode() {
        let bc = make_bytecode(vec![], None);
        let result = builtin_interpreted_function_p(vec![bc]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn interpreted_function_p_false_for_subr() {
        let result = builtin_interpreted_function_p(vec![Value::Subr("car".into())]).unwrap();
        assert!(result.is_nil());
    }

    // -- special-form-p --

    #[test]
    fn special_form_p_true_for_if() {
        let result = builtin_special_form_p(vec![Value::symbol("if")]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn special_form_p_true_for_quote() {
        let result = builtin_special_form_p(vec![Value::symbol("quote")]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn special_form_p_true_for_setq() {
        let result = builtin_special_form_p(vec![Value::symbol("setq")]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn special_form_p_true_for_inline() {
        let result = builtin_special_form_p(vec![Value::symbol("inline")]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn special_form_p_false_for_car() {
        let result = builtin_special_form_p(vec![Value::symbol("car")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn special_form_p_false_for_when() {
        let result = builtin_special_form_p(vec![Value::symbol("when")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn special_form_p_false_for_throw() {
        let result = builtin_special_form_p(vec![Value::symbol("throw")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn special_form_p_false_for_int() {
        let result = builtin_special_form_p(vec![Value::Int(42)]).unwrap();
        assert!(result.is_nil());
    }

    // -- macrop --

    #[test]
    fn macrop_true_for_macro() {
        let m = make_macro(vec!["form"]);
        let result = builtin_macrop(vec![m]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn macrop_false_for_lambda() {
        let lam = make_lambda(vec!["x"], vec![], None);
        let result = builtin_macrop(vec![lam]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn macrop_false_for_nil() {
        let result = builtin_macrop(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn macrop_true_for_macro_cons_marker() {
        let marker = Value::cons(Value::symbol("macro"), Value::Int(1));
        let result = builtin_macrop(vec![marker]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn macrop_autoload_macro_returns_macro_marker_list() {
        let autoload_macro = Value::list(vec![
            Value::symbol("autoload"),
            Value::string("dummy-file"),
            Value::Nil,
            Value::Nil,
            Value::symbol("macro"),
        ]);
        let result = builtin_macrop(vec![autoload_macro]).unwrap();
        assert_eq!(
            result,
            Value::list(vec![Value::symbol("macro"), Value::True])
        );
    }

    #[test]
    fn macrop_autoload_function_is_nil() {
        let autoload_function = Value::list(vec![
            Value::symbol("autoload"),
            Value::string("dummy-file"),
            Value::Nil,
            Value::True,
            Value::Nil,
        ]);
        let result = builtin_macrop(vec![autoload_function]).unwrap();
        assert!(result.is_nil());
    }

    // -- commandp --

    #[test]
    fn commandp_true_for_subr() {
        let result = builtin_commandp(vec![Value::Subr("car".into())]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn commandp_true_for_lambda() {
        let lam = make_lambda(vec![], vec![], None);
        let result = builtin_commandp(vec![lam]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn commandp_false_for_int() {
        let result = builtin_commandp(vec![Value::Int(42)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn commandp_false_for_nil() {
        let result = builtin_commandp(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    // -- func-arity --

    #[test]
    fn func_arity_lambda_required_only() {
        let lam = make_lambda(vec!["a", "b"], vec![], None);
        let result = builtin_func_arity(vec![lam]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(2));
            assert_eq!(pair.cdr.as_int(), Some(2));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn func_arity_lambda_with_optional() {
        let lam = make_lambda(vec!["a"], vec!["b", "c"], None);
        let result = builtin_func_arity(vec![lam]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(1));
            assert_eq!(pair.cdr.as_int(), Some(3));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn func_arity_lambda_with_rest() {
        let lam = make_lambda(vec!["a"], vec![], Some("rest"));
        let result = builtin_func_arity(vec![lam]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(1));
            assert_eq!(pair.cdr.as_symbol_name(), Some("many"));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn func_arity_bytecode() {
        let bc = make_bytecode(vec!["x", "y"], Some("rest"));
        let result = builtin_func_arity(vec![bc]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(2));
            assert_eq!(pair.cdr.as_symbol_name(), Some("many"));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn func_arity_subr() {
        let result = builtin_func_arity(vec![Value::Subr("+".into())]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(0));
            assert_eq!(pair.cdr.as_symbol_name(), Some("many"));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn func_arity_subr_uses_compat_overrides() {
        let message = builtin_func_arity(vec![Value::Subr("message".into())]).unwrap();
        if let Value::Cons(cell) = &message {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(1));
            assert_eq!(pair.cdr.as_symbol_name(), Some("many"));
        } else {
            panic!("expected cons cell");
        }

        let car = builtin_func_arity(vec![Value::Subr("car".into())]).unwrap();
        if let Value::Cons(cell) = &car {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(1));
            assert_eq!(pair.cdr.as_int(), Some(1));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn func_arity_macro() {
        let m = make_macro(vec!["a", "b"]);
        let result = builtin_func_arity(vec![m]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(2));
            assert_eq!(pair.cdr.as_int(), Some(2));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn fallback_macro_defvar_local_preserves_optional_arity() {
        let macro_value = fallback_macro_value("defvar-local").expect("fallback macro exists");
        let result = builtin_func_arity(vec![macro_value]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert_eq!(pair.car.as_int(), Some(2));
            assert_eq!(pair.cdr.as_int(), Some(3));
        } else {
            panic!("expected cons cell");
        }
    }

    #[test]
    fn func_arity_error_for_non_callable() {
        let result = builtin_func_arity(vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    // -- indirect-function --

    #[test]
    fn indirect_function_non_symbol_passthrough() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let lam = make_lambda(vec![], vec![], None);
        let result = builtin_indirect_function(&mut eval, vec![lam.clone()]).unwrap();
        // Non-symbol returns as-is.
        assert!(matches!(result, Value::Lambda(_)));
    }

    #[test]
    fn indirect_function_resolves_symbol() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        eval.set_function("my-fn", Value::Subr("+".into()));
        let result = builtin_indirect_function(&mut eval, vec![Value::symbol("my-fn")]).unwrap();
        assert!(matches!(result, Value::Subr(ref n) if n == "+"));
    }

    #[test]
    fn indirect_function_follows_chain() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        eval.set_function("real-fn", Value::Subr("+".into()));
        eval.set_function("alias", Value::Symbol("real-fn".into()));
        let result = builtin_indirect_function(&mut eval, vec![Value::symbol("alias")]).unwrap();
        assert!(matches!(result, Value::Subr(ref n) if n == "+"));
    }

    #[test]
    fn indirect_function_void_with_noerror() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let result =
            builtin_indirect_function(&mut eval, vec![Value::symbol("nonexistent"), Value::True])
                .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn indirect_function_void_signals_error() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let result = builtin_indirect_function(&mut eval, vec![Value::symbol("nonexistent")]);
        assert!(result.is_err());
    }

    // -- wrong arg count --

    #[test]
    fn subrp_wrong_args() {
        let result = builtin_subrp(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn subr_name_wrong_args() {
        let result = builtin_subr_name(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn func_arity_wrong_args() {
        let result = builtin_func_arity(vec![]);
        assert!(result.is_err());
    }
}
