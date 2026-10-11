use super::*;
use crate::emacs_core::intern::intern;

#[test]
fn p6_typed_slot_decoders_reject_invalid_outer_values() {
    assert!(matches!(
        FunctionParams::try_from(Value::vector(vec![])),
        Err(BytecodeSlotError::Arglist(_))
    ));
    assert!(matches!(
        UnibyteCode::try_from(Value::fixnum(0)),
        Err(BytecodeSlotError::Code(_))
    ));
    assert!(matches!(
        ConstantsVector::try_from(Value::NIL),
        Err(BytecodeSlotError::Constants(_))
    ));
    assert!(matches!(
        StackDepth::try_from(Value::fixnum(-1)),
        Err(StackDepthError::Value(_))
    ));
    assert!(matches!(
        StackDepth::try_from(Value::make_float(1.0)),
        Err(StackDepthError::Value(_))
    ));
}

#[test]
fn p6_typed_stack_depth_preserves_full_width_and_rejects_overflow() {
    assert_eq!(
        StackDepth::try_from(Value::fixnum(65537)).unwrap().get(),
        65537
    );
    assert_eq!(
        StackDepth::try_from(Value::fixnum(Value::MOST_POSITIVE_FIXNUM))
            .unwrap()
            .value(),
        Value::fixnum(Value::MOST_POSITIVE_FIXNUM)
    );
    assert!(matches!(
        StackDepth::try_from(u64::MAX),
        Err(StackDepthError::Range(_))
    ));
}

#[test]
fn p6_typed_arg_template_preserves_signed_and_large_bounds() {
    let negative = ArgTemplate::from(-1);
    assert_eq!(negative.mandatory(), 127);
    assert_eq!(negative.nonrest(), -1);
    assert_eq!(negative.rest(), RestSlot::Present);
    assert!(matches!(
        negative.stack_shape(),
        Err(ParamShapeError::NonRest(-1))
    ));
    let large = ArgTemplate::from(1 << 40);
    assert_eq!(large.nonrest(), 4294967296);
    assert_eq!(large.stack_shape().unwrap().nonrest(), 4294967296);
    let inconsistent = ArgTemplate::from(383);
    assert_eq!(inconsistent.mandatory(), 127);
    assert_eq!(inconsistent.nonrest(), 1);
    assert_eq!(inconsistent.stack_shape().unwrap().optional(), None);
    assert!(!inconsistent.accepts(1));
}

#[test]
fn p6_typed_dynamic_nil_keeps_zero_argument_optimization_shape() {
    let params = FunctionParams::try_from(Value::NIL).unwrap();
    assert!(matches!(params, FunctionParams::Dynamic(_)));
    assert_eq!(params.fixed_arity(), Some(0));
    assert_eq!(params.stack_shape().unwrap().entry_depth().unwrap(), 0);
}

#[test]
fn p6_typed_dynamic_formals_validate_only_when_walked() {
    let raw = Value::list(vec![Value::fixnum(1)]);
    let FunctionParams::Dynamic(params) = FunctionParams::try_from(raw).unwrap() else {
        panic!("cons slot remains dynamic");
    };
    assert_eq!(params.value(), raw);
    assert!(matches!(
        params.invocation().next_binding(),
        Err(FormalListError::NonSymbol(_))
    ));
    let raw = Value::cons(Value::symbol("x"), Value::fixnum(7));
    let FunctionParams::Dynamic(params) = FunctionParams::try_from(raw).unwrap() else {
        panic!("cons slot remains dynamic");
    };
    let mut cursor = params.invocation();
    assert_eq!(
        cursor.next_binding().unwrap(),
        Some(FormalBinding::Required(intern("x")))
    );
    cursor.finish_binding().unwrap();
    assert!(matches!(
        cursor.next_binding(),
        Err(FormalListError::Tail(_))
    ));
}

#[test]
fn p6_typed_dynamic_formals_check_markers_and_accept_multiple_rest_names() {
    for (values, marker_order) in [
        (
            vec![Value::symbol("&optional"), Value::symbol("&optional")],
            true,
        ),
        (vec![Value::symbol("&rest")], false),
    ] {
        let FunctionParams::Dynamic(params) =
            FunctionParams::try_from(Value::list(values)).unwrap()
        else {
            panic!("list slot remains dynamic");
        };
        if marker_order {
            assert!(matches!(
                params.invocation().next_binding(),
                Err(FormalListError::MarkerOrder)
            ));
        } else {
            assert!(matches!(
                params.invocation().next_binding(),
                Err(FormalListError::MissingRestVariable)
            ));
        }
    }
    let raw = Value::list(vec![
        Value::symbol("&rest"),
        Value::symbol("a"),
        Value::symbol("b"),
    ]);
    let FunctionParams::Dynamic(params) = FunctionParams::try_from(raw).unwrap() else {
        panic!("list slot remains dynamic");
    };
    let mut cursor = params.invocation();
    for name in ["a", "b"] {
        assert_eq!(
            cursor.next_binding().unwrap(),
            Some(FormalBinding::Rest(intern(name)))
        );
        cursor.finish_binding().unwrap();
    }
    assert_eq!(cursor.next_binding().unwrap(), None);
}

#[test]
fn p6_typed_dynamic_formals_accept_nil_as_a_symbol() {
    let FunctionParams::Dynamic(params) =
        FunctionParams::try_from(Value::list(vec![Value::NIL])).unwrap()
    else {
        panic!("list slot remains dynamic");
    };
    assert_eq!(
        params.invocation().next_binding().unwrap(),
        Some(FormalBinding::Required(intern("nil")))
    );
}

#[test]
fn p6_typed_constants_view_preserves_original_objects() {
    let list = Value::list(vec![Value::symbol("hash-table"), Value::fixnum(9)]);
    let raw = Value::vector(vec![list]);
    let constants = ConstantsVector::try_from(raw).unwrap();
    assert_eq!(constants.value(), raw);
    assert_eq!(constants.as_slice(), &[list]);
}

#[test]
fn p6_call_shape_matches_the_checked_descriptor_split() {
    // The compact fast path must answer exactly what GNU's signed arity test
    // followed by the host conversion answers, at and around its boundary.
    let templates = [
        0_i64,
        1,
        257,
        514,
        642,
        (127 << 8) | 128 | 127,
        (1 << 15) - 1,
        1 << 15,
        (1 << 15) + 257,
        383,
        -1,
        -257,
        i64::MAX >> 2,
    ];
    for raw in templates {
        let template = ArgTemplate::from(raw);
        for nargs in [0_usize, 1, 2, 3, 126, 127, 128, 200] {
            let expected = if !template.accepts(nargs) {
                None
            } else {
                Some(template.stack_shape().ok())
            };
            let actual = match template.call_shape(nargs) {
                Err(CallShapeError::Arity) => None,
                Err(CallShapeError::Shape(_)) => Some(None),
                Ok(shape) => Some(Some(shape)),
            };
            assert_eq!(actual, expected, "template {raw} nargs {nargs}");
        }
    }
}
