//! One bounded direct-constant shape parser; no evaluator or mutable graph loan.
use super::*;

/// Selected by the entry capability, then retained by the rooted activation.
#[derive(Clone, Copy)]
pub(in crate::tagged::gc::mutator_registration::symbol_authority) enum ReturnSubset {
    Scalar,
    ScalarOrName,
}

/// Syntax alone does not authorize an ordinary parameter or confer a call
/// capability. A unary candidate still needs separate admission and arity proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParameterSyntax {
    Nullary,
    UnaryCandidate(SymId),
}

/// A bounded syntactic observation, never an executable function selection.
struct ConstantSyntax {
    parameters: ParameterSyntax,
    answer: Value,
}

pub(super) fn decode(
    value: Value,
    lambda: SymId,
    subset: ReturnSubset,
) -> Result<Value, FunctionError> {
    let syntax = parse_syntax(value, lambda, subset)?;
    match syntax.parameters {
        ParameterSyntax::Nullary => Ok(syntax.answer),
        ParameterSyntax::UnaryCandidate(_) => Err(FunctionError::UnsupportedDefinition),
    }
}

fn parse_syntax(
    value: Value,
    lambda: SymId,
    subset: ReturnSubset,
) -> Result<ConstantSyntax, FunctionError> {
    if !value.is_cons() || value.cons_car_unobserved().as_bare_symbol_id() != Some(lambda) {
        return Err(FunctionError::UnsupportedDefinition);
    }
    let args = value.cons_cdr_unobserved();
    if !args.is_cons() {
        return Err(FunctionError::UnsupportedDefinition);
    }
    let parameter_list = args.cons_car_unobserved();
    let parameters = if parameter_list.is_nil() {
        ParameterSyntax::Nullary
    } else if parameter_list.is_cons() && parameter_list.cons_cdr_unobserved().is_nil() {
        let parameter = parameter_list
            .cons_car_unobserved()
            .as_bare_symbol_id()
            .ok_or(FunctionError::UnsupportedDefinition)?;
        ParameterSyntax::UnaryCandidate(parameter)
    } else {
        return Err(FunctionError::UnsupportedDefinition);
    };
    let body = args.cons_cdr_unobserved();
    if !body.is_cons() || !body.cons_cdr_unobserved().is_nil() {
        return Err(FunctionError::UnsupportedDefinition);
    }
    let answer = body.cons_car_unobserved();
    if answer.is_nil() || answer.is_t() || answer.is_fixnum() {
        return Ok(ConstantSyntax { parameters, answer });
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
            Ok(ConstantSyntax { parameters, answer })
        }
    }
}

#[cfg(test)]
#[path = "constant/tests.rs"]
mod tests;
