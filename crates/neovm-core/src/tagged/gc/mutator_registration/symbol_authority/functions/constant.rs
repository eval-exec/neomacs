//! One bounded direct-constant shape parser; no evaluator or mutable graph loan.
use super::*;

/// Selected by the entry capability, then retained by the rooted activation.
#[derive(Clone, Copy)]
pub(in crate::tagged::gc::mutator_registration::symbol_authority) enum ReturnSubset {
    Scalar,
    ScalarOrName,
}

pub(super) fn decode(
    value: Value,
    lambda: SymId,
    subset: ReturnSubset,
) -> Result<Value, FunctionError> {
    if !value.is_cons() || value.cons_car_unobserved().as_bare_symbol_id() != Some(lambda) {
        return Err(FunctionError::UnsupportedDefinition);
    }
    let args = value.cons_cdr_unobserved();
    if !args.is_cons() || !args.cons_car_unobserved().is_nil() {
        return Err(FunctionError::UnsupportedDefinition);
    }
    let body = args.cons_cdr_unobserved();
    if !body.is_cons() || !body.cons_cdr_unobserved().is_nil() {
        return Err(FunctionError::UnsupportedDefinition);
    }
    let answer = body.cons_car_unobserved();
    if answer.is_nil() || answer.is_t() || answer.is_fixnum() {
        return Ok(answer);
    }
    match subset {
        ReturnSubset::Scalar => Err(FunctionError::UnsupportedDefinition),
        ReturnSubset::ScalarOrName => {
            let name = answer
                .as_lisp_string()
                .ok_or(FunctionError::UnsupportedDefinition)?;
            // Do not recursively admit property graphs or copy/strip this
            // object. Ordinary-name and keyword rules remain C5's intern rule.
            if name.has_intervals() {
                return Err(FunctionError::UnsupportedDefinition);
            }
            Ok(answer)
        }
    }
}
