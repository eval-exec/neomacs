use super::*;
use crate::heap_types::LispString;
use crate::tagged::gc::TestHeap;

const LAMBDA: SymId = SymId(50_000);
const FIRST_PARAMETER: SymId = SymId(50_001);
const SECOND_PARAMETER: SymId = SymId(50_002);

fn definition(heap: &mut TestHeap, parameters: Value, answer: Value) -> Value {
    let body = heap.alloc_cons(answer, Value::NIL);
    let args = heap.alloc_cons(parameters, body);
    heap.alloc_cons(Value::from_sym_id(LAMBDA), args)
}

fn assert_refused(value: Value) {
    for subset in [ReturnSubset::Scalar, ReturnSubset::ScalarOrName] {
        assert!(matches!(
            parse_syntax(value, LAMBDA, subset),
            Err(FunctionError::UnsupportedDefinition)
        ));
        assert!(matches!(
            decode(value, LAMBDA, subset),
            Err(FunctionError::UnsupportedDefinition)
        ));
    }
}

#[test]
fn nmjit_n4_constant_syntax_preserves_nullary_scalar_and_name_decoding() {
    let mut heap = TestHeap::new();
    for answer in [Value::NIL, Value::T, Value::fixnum(73), Value::fixnum(-97)] {
        let value = definition(&mut heap, Value::NIL, answer);
        for subset in [ReturnSubset::Scalar, ReturnSubset::ScalarOrName] {
            let syntax = parse_syntax(value, LAMBDA, subset).unwrap();
            assert_eq!(syntax.parameters, ParameterSyntax::Nullary);
            assert_eq!(syntax.answer.bits(), answer.bits());
            assert_eq!(decode(value, LAMBDA, subset).unwrap().bits(), answer.bits());
        }
    }
    let name = heap.alloc_string(LispString::from_utf8("constant-name"));
    let value = definition(&mut heap, Value::NIL, name);
    assert!(matches!(
        decode(value, LAMBDA, ReturnSubset::Scalar),
        Err(FunctionError::UnsupportedDefinition)
    ));
    let syntax = parse_syntax(value, LAMBDA, ReturnSubset::ScalarOrName).unwrap();
    assert_eq!(syntax.parameters, ParameterSyntax::Nullary);
    assert_eq!(syntax.answer.bits(), name.bits());
    assert_eq!(
        decode(value, LAMBDA, ReturnSubset::ScalarOrName)
            .unwrap()
            .bits(),
        name.bits()
    );
}

#[test]
fn nmjit_n4_constant_syntax_unary_candidates_retain_parameter_identity_but_refuse_calls() {
    let mut heap = TestHeap::new();
    let answer = Value::fixnum(73);
    let mut observed = Vec::new();
    for parameter in [FIRST_PARAMETER, SECOND_PARAMETER] {
        let parameters = heap.alloc_cons(Value::from_sym_id(parameter), Value::NIL);
        let value = definition(&mut heap, parameters, answer);
        for subset in [ReturnSubset::Scalar, ReturnSubset::ScalarOrName] {
            let syntax = parse_syntax(value, LAMBDA, subset).unwrap();
            assert_eq!(
                syntax.parameters,
                ParameterSyntax::UnaryCandidate(parameter)
            );
            assert_eq!(syntax.answer.bits(), answer.bits());
            assert!(matches!(
                decode(value, LAMBDA, subset),
                Err(FunctionError::UnsupportedDefinition)
            ));
        }
        observed.push(
            parse_syntax(value, LAMBDA, ReturnSubset::Scalar)
                .unwrap()
                .parameters,
        );
    }
    assert_ne!(observed[0], observed[1]);
}

#[test]
fn nmjit_n4_constant_syntax_refuses_malformed_parameters() {
    let mut heap = TestHeap::new();
    let first = Value::from_sym_id(FIRST_PARAMETER);
    let second = Value::from_sym_id(SECOND_PARAMETER);
    let dotted = heap.alloc_cons(first, second);
    let tail = heap.alloc_cons(second, Value::NIL);
    let multiple = heap.alloc_cons(first, tail);
    let nonsymbol = heap.alloc_cons(Value::fixnum(1), Value::NIL);
    for parameters in [first, dotted, multiple, nonsymbol] {
        assert_refused(definition(&mut heap, parameters, Value::fixnum(73)));
    }
}

#[test]
fn nmjit_n4_constant_syntax_refuses_multiple_form_and_nonlambda_bodies() {
    let mut heap = TestHeap::new();
    let second_body = heap.alloc_cons(Value::fixnum(97), Value::NIL);
    let multiple_body = heap.alloc_cons(Value::fixnum(73), second_body);
    let args = heap.alloc_cons(Value::NIL, multiple_body);
    let multiple = heap.alloc_cons(Value::from_sym_id(LAMBDA), args);
    assert_refused(multiple);
    let form = heap.alloc_cons(Value::from_sym_id(FIRST_PARAMETER), Value::NIL);
    assert_refused(definition(&mut heap, Value::NIL, form));
    let body = heap.alloc_cons(Value::fixnum(73), Value::NIL);
    let args = heap.alloc_cons(Value::NIL, body);
    let nonlambda = heap.alloc_cons(Value::from_sym_id(FIRST_PARAMETER), args);
    assert_refused(nonlambda);
    assert_refused(Value::fixnum(73));
}

#[test]
fn nmjit_n4_unary_shape_requires_matching_prepared_parameter_and_scalar() {
    let mut heap = TestHeap::new();
    let parameters = heap.alloc_cons(Value::from_sym_id(FIRST_PARAMETER), Value::NIL);
    let value = definition(&mut heap, parameters, Value::fixnum(73));
    assert_eq!(
        decode_shape(
            value,
            LAMBDA,
            CallShape::UnaryScalar(UnaryParameter(FIRST_PARAMETER))
        )
        .unwrap()
        .bits(),
        Value::fixnum(73).bits()
    );
    assert!(matches!(
        decode_shape(
            value,
            LAMBDA,
            CallShape::UnaryScalar(UnaryParameter(SECOND_PARAMETER))
        ),
        Err(FunctionError::UnsupportedDefinition)
    ));
    let nullary = definition(&mut heap, Value::NIL, Value::fixnum(73));
    assert!(matches!(
        decode_shape(
            nullary,
            LAMBDA,
            CallShape::UnaryScalar(UnaryParameter(FIRST_PARAMETER))
        ),
        Err(FunctionError::UnsupportedDefinition)
    ));
    let name = heap.alloc_string(LispString::from_utf8("constant-name"));
    let managed_answer = definition(&mut heap, parameters, name);
    assert!(matches!(
        decode_shape(
            managed_answer,
            LAMBDA,
            CallShape::UnaryScalar(UnaryParameter(FIRST_PARAMETER))
        ),
        Err(FunctionError::UnsupportedDefinition)
    ));
}
