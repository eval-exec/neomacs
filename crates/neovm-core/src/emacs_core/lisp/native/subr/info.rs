//! Subr/primitive introspection builtins.
//!
//! Provides type predicates and introspection for callable objects:
//! - `subrp`, `subr-name`, `subr-arity`
//! - `commandp`, `functionp`, `byte-code-function-p`, `closurep`
//! - `interpreted-function-p`, `special-form-p`, `macrop`
//! - `func-arity`, `indirect-function`

use super::error::{EvalResult, signal};
use super::intern::{SymId, intern, resolve_sym};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::tagged::header::{SubrDispatchKind, SubrObj};

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Context/public callable classification
// ---------------------------------------------------------------------------

/// Returns true if `name` is recognized by the evaluator's special-form
/// dispatch path.
///
/// This list mirrors `Context::try_special_form()` in `eval.rs`.
/// Only includes forms that are evaluator-owned by construction:
/// GNU C special forms, evaluator internals, and NeoVM-owned runtime forms.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn is_evaluator_special_form_name(name: &str) -> bool {
    super::eval::evaluator_dispatch_kind(name) == Some(SubrDispatchKind::SpecialForm)
        || matches!(name, "lambda" | "byte-code-literal" | "byte-code")
}

/// Returns true for special forms exposed by `special-form-p`.
///
/// Emacs distinguishes evaluator internals from public special forms:
/// many evaluator-recognized constructs are macros/functions in user-visible
/// introspection.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn is_special_form(name: &str) -> bool {
    super::eval::evaluator_dispatch_kind(name) == Some(SubrDispatchKind::SpecialForm)
}

// ---------------------------------------------------------------------------
// Arity helpers
// ---------------------------------------------------------------------------

/// Build a cons cell `(MIN . MAX)` representing arity.
/// `max` of `None` means "many" (unbounded &rest), represented by the
/// symbol `many`.
fn arity_cons(min: usize, max: Option<usize>) -> Value {
    let min_val = Value::fixnum(min as i64);
    let max_val = match max {
        Some(n) => Value::fixnum(n as i64),
        None => Value::symbol("many"),
    };
    Value::cons(min_val, max_val)
}

fn arity_unevalled(min: usize) -> Value {
    Value::cons(Value::fixnum(min as i64), Value::symbol("unevalled"))
}

fn macro_wrapper_payload(function: Value) -> Option<Value> {
    if function.is_cons() && function.cons_car().as_symbol_name() == Some("macro") {
        Some(function.cons_cdr())
    } else {
        None
    }
}

fn lambda_arity_from_arglist(function: Value, arglist: Value) -> EvalResult {
    let mut syms_left = arglist;
    let mut minargs = 0;
    let mut maxargs = 0;
    let mut optional = false;

    while syms_left.is_cons() {
        let next = syms_left.cons_car();
        syms_left = syms_left.cons_cdr();
        let next = if next.is_symbol_with_pos() {
            next.as_symbol_with_pos_sym().unwrap_or(next)
        } else {
            next
        };
        let Some(name) = next.as_symbol_name() else {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        };

        match name {
            "&rest" => return Ok(arity_cons(minargs, None)),
            "&optional" => optional = true,
            _ => {
                if !optional {
                    minargs += 1;
                }
                maxargs += 1;
            }
        }
    }

    if !syms_left.is_nil() {
        return Err(signal(LispCondition::InvalidFunction, vec![function]));
    }

    Ok(arity_cons(minargs, Some(maxargs)))
}

fn lambda_arity_from_cons(function: Value) -> EvalResult {
    let syms_left = function.cons_cdr();
    if !syms_left.is_cons() {
        return Err(signal(LispCondition::InvalidFunction, vec![function]));
    }
    lambda_arity_from_arglist(function, syms_left.cons_car())
}

pub(crate) fn dispatch_subr_arity_value(name: &str) -> Value {
    let sym_id = intern(name);
    let Some(entry) = super::eval::lookup_global_subr_entry(sym_id) else {
        return arity_cons(0, None);
    };
    if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
        arity_unevalled(entry.min_args as usize)
    } else {
        arity_cons(entry.min_args as usize, entry.max_args.map(usize::from))
    }
}

// ---------------------------------------------------------------------------
// Pure builtins (no evaluator access)
// ---------------------------------------------------------------------------

/// `(subr-name SUBR)` -- return the name of a subroutine as a string.
pub(crate) fn builtin_subr_name(args: Vec<Value>) -> EvalResult {
    expect_args("subr-name", &args, 1)?;
    match args[0].kind() {
        ValueKind::Subr(id) => Ok(Value::string(resolve_sym(id))),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = args[0].as_subr_id().unwrap();
            Ok(Value::string(resolve_sym(id)))
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("subrp"), args[0]],
        )),
    }
}

/// `(subr-arity SUBR)` -- return (MIN . MAX) cons cell for argument counts.
///
/// Reads arity from the canonical static subr registry (single source of truth).
pub(crate) fn builtin_subr_arity(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("subr-arity", &args, 1)?;
    match args[0].kind() {
        ValueKind::Subr(id) => Ok(subr_arity_from_registry(ctx, id)),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = args[0].as_subr_id().unwrap();
            Ok(subr_arity_from_registry(ctx, id))
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("subrp"), args[0]],
        )),
    }
}

/// Look up arity from the global subr table.
fn subr_arity_from_registry(_ctx: &super::eval::Context, sym_id: SymId) -> Value {
    let entry = super::eval::lookup_global_subr_entry(sym_id)
        .expect("a subr value must have a registered declaration");
    // GNU Emacs: special forms (UNEVALLED) return (MIN . unevalled).
    // The Elisp `special-form-p` checks `(eq (cdr (subr-arity x)) 'unevalled)`.
    if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
        return arity_unevalled(entry.min_args as usize);
    }
    arity_cons(entry.min_args as usize, entry.max_args.map(usize::from))
}

/// `(native-comp-function-p OBJECT)` -- return t if OBJECT is a native-compiled
/// function object.
///
/// NeoVM does not currently model native-compiled function objects, so this
/// always returns nil.
pub(crate) fn builtin_native_comp_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("native-comp-function-p", &args, 1)?;
    Ok(Value::NIL)
}

/// `(interpreted-function-p OBJECT)` -- return t if OBJECT is an interpreted
/// function (a Lambda that is NOT byte-compiled).
///
/// In our VM, any `Value::Lambda` is interpreted (as opposed to
/// `Value::ByteCode`).
pub(crate) fn builtin_interpreted_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("interpreted-function-p", &args, 1)?;
    Ok(Value::bool_val(args[0].is_lambda()))
}

/// `(special-form-p OBJECT)` -- return t if OBJECT is a special form.
///
/// GNU Emacs (eval.c): checks if OBJECT is a symbol whose function cell
/// contains a subr with max_args == UNEVALLED.  NeoVM checks the symbol
/// name against the evaluator's special-form table.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_special_form_p(args: Vec<Value>) -> EvalResult {
    expect_args("special-form-p", &args, 1)?;
    let result = match args[0].kind() {
        ValueKind::Symbol(id) => {
            super::eval::evaluator_dispatch_kind(resolve_sym(id))
                == Some(SubrDispatchKind::SpecialForm)
        }
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
            subr_dispatch_kind_from_value(&args[0])
                .is_some_and(|kind| kind == SubrDispatchKind::SpecialForm)
        }
        _ => false,
    };
    Ok(Value::bool_val(result))
}

pub(crate) fn subr_dispatch_kind_from_value(value: &Value) -> Option<SubrDispatchKind> {
    // New path: look up from global table
    if let Some(sym_id) = value.as_subr_id()
        && let Some(entry) = super::eval::lookup_global_subr_entry(sym_id)
    {
        return Some(entry.dispatch_kind);
    }
    // Old heap path fallback
    if !matches!(value.kind(), ValueKind::Veclike(VecLikeType::Subr)) {
        return None;
    }
    let ptr = value.as_veclike_ptr()? as *const SubrObj;
    Some(unsafe { (*ptr).dispatch_kind })
}

pub(crate) fn subr_is_callable_function_value(value: &Value) -> bool {
    subr_dispatch_kind_from_value(value).is_some_and(|kind| kind != SubrDispatchKind::SpecialForm)
}

// `macrop_check' is gone with the `macrop' subr it existed for: GNU has no
// C `macrop', only `(defun macrop (object) ...)' at lisp/subr.el:4793.
// DIVERGENCES.md 148.

/// `(commandp FUNCTION &optional FOR-CALL-INTERACTIVELY)` -- return t if
/// FUNCTION is an interactive command.
///
/// In our simplified VM, any callable value (lambda, subr, bytecode) is
/// treated as a potential command.  A more complete implementation would
/// check for an `interactive` declaration.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_commandp(args: Vec<Value>) -> EvalResult {
    expect_min_args("commandp", &args, 1)?;
    expect_max_args("commandp", &args, 2)?;
    Ok(Value::bool_val(args[0].is_function()))
}

/// `(func-arity FUNCTION)` -- return (MIN . MAX) for any callable.
///
/// Works for lambdas (reads `LambdaParams`), byte-code (reads `params`),
/// and subrs (reads from the canonical static `SymId` registry entry).
pub(crate) fn builtin_func_arity_ctx(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("func-arity", &args, 1)?;
    let original = args[0];
    let function = macro_wrapper_payload(original).unwrap_or(original);
    if super::autoload::is_autoload_value(&function) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), function],
        ));
    }
    match function.kind() {
        ValueKind::Veclike(VecLikeType::Lambda) => {
            let params = function.closure_params().unwrap();
            let min = params.min_arity();
            let max = params.max_arity();
            Ok(arity_cons(min, max))
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            let bc = function.get_bytecode_data().unwrap();
            let min = bc.params.min_arity();
            let max = bc.params.max_arity();
            Ok(arity_cons(min, max))
        }
        ValueKind::Subr(id) => Ok(subr_arity_from_registry(ctx, id)),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = function.as_subr_id().unwrap();
            Ok(subr_arity_from_registry(ctx, id))
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            let params = function.closure_params().unwrap();
            let min = params.min_arity();
            let max = params.max_arity();
            Ok(arity_cons(min, max))
        }
        ValueKind::Cons if function.cons_car().as_symbol_name() == Some("lambda") => {
            lambda_arity_from_cons(function)
        }
        _other => Err(signal(LispCondition::InvalidFunction, vec![original])),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/info.rs"]
mod tests;
