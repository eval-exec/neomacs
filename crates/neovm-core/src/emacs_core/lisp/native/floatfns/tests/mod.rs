use super::*;
use crate::emacs_core::value::ValueKind;

// Helper to make float comparison with epsilon
fn assert_float_eq(val: &Value, expected: f64, epsilon: f64) {
    match val.kind() {
        ValueKind::Float => {
            let f = val.as_float().unwrap();
            assert!(
                (f - expected).abs() < epsilon,
                "expected {} but got {}",
                expected,
                f
            );
        }
        _other => panic!("expected Float, got {:?}", val),
    }
}

fn assert_int_eq(val: &Value, expected: i64) {
    match val.kind() {
        ValueKind::Fixnum(n) => assert_eq!(n, expected, "expected {} but got {}", expected, n),
        _other => panic!("expected Int, got {:?}", val),
    }
}

// ===== Classification =====

#[test]
fn test_copysign() {
    crate::test_utils::init_test_tracing();
    let result = builtin_copysign(vec![Value::make_float(5.0), Value::make_float(-1.0)]).unwrap();
    assert_float_eq(&result, -5.0, 1e-10);

    let result = builtin_copysign(vec![Value::make_float(-5.0), Value::make_float(1.0)]).unwrap();
    assert_float_eq(&result, 5.0, 1e-10);
}

#[test]
fn test_frexp() {
    crate::test_utils::init_test_tracing();
    let result = builtin_frexp(vec![Value::make_float(8.0)]).unwrap();
    // 8.0 = 0.5 * 2^4
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_float_eq(&pair_car, 0.5, 1e-10);
        assert_int_eq(&pair_cdr, 4);
    } else {
        panic!("expected cons");
    }

    // frexp(0.0) = (0.0 . 0)
    let result = builtin_frexp(vec![Value::make_float(0.0)]).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_float_eq(&pair_car, 0.0, 1e-10);
        assert_int_eq(&pair_cdr, 0);
    } else {
        panic!("expected cons");
    }

    // frexp(-0.0) preserves signed-zero in significand.
    let result = builtin_frexp(vec![Value::make_float(-0.0)]).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        match pair_car.kind() {
            ValueKind::Float => {
                let f = pair_car.as_float().unwrap();
                assert_eq!(f, 0.0);
                assert!(f.is_sign_negative(), "expected negative zero");
            }
            ref other => panic!("expected Float, got {:?}", other),
        }
        assert_int_eq(&pair_cdr, 0);
    } else {
        panic!("expected cons");
    }
}

#[test]
fn test_frexp_negative() {
    crate::test_utils::init_test_tracing();
    let result = builtin_frexp(vec![Value::make_float(-6.0)]).unwrap();
    // -6.0 = -0.75 * 2^3
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_float_eq(&pair_car, -0.75, 1e-10);
        assert_int_eq(&pair_cdr, 3);
    } else {
        panic!("expected cons");
    }
}

#[test]
fn test_ldexp() {
    crate::test_utils::init_test_tracing();
    // 0.5 * 2^4 = 8.0
    let result = builtin_ldexp(vec![Value::make_float(0.5), Value::fixnum(4)]).unwrap();
    assert_float_eq(&result, 8.0, 1e-10);

    // 1.0 * 2^10 = 1024.0
    let result = builtin_ldexp(vec![Value::make_float(1.0), Value::fixnum(10)]).unwrap();
    assert_float_eq(&result, 1024.0, 1e-10);
}

// ===== logb =====

#[test]
fn test_logb() {
    crate::test_utils::init_test_tracing();
    // logb(8) = 3  (since log2(8) = 3)
    let result = builtin_logb(vec![Value::make_float(8.0)]).unwrap();
    assert_int_eq(&result, 3);

    // logb(1) = 0
    let result = builtin_logb(vec![Value::make_float(1.0)]).unwrap();
    assert_int_eq(&result, 0);

    // logb(0.5) = -1
    let result = builtin_logb(vec![Value::make_float(0.5)]).unwrap();
    assert_int_eq(&result, -1);
}

// ===== Rounding to float =====

#[test]
fn test_fceiling() {
    crate::test_utils::init_test_tracing();
    let result = builtin_fceiling(vec![Value::make_float(1.1)]).unwrap();
    assert_float_eq(&result, 2.0, 1e-10);

    let result = builtin_fceiling(vec![Value::make_float(-1.1)]).unwrap();
    assert_float_eq(&result, -1.0, 1e-10);
}

#[test]
fn test_ffloor() {
    crate::test_utils::init_test_tracing();
    let result = builtin_ffloor(vec![Value::make_float(1.9)]).unwrap();
    assert_float_eq(&result, 1.0, 1e-10);

    let result = builtin_ffloor(vec![Value::make_float(-1.1)]).unwrap();
    assert_float_eq(&result, -2.0, 1e-10);
}

#[test]
fn test_fround() {
    crate::test_utils::init_test_tracing();
    let result = builtin_fround(vec![Value::make_float(1.4)]).unwrap();
    assert_float_eq(&result, 1.0, 1e-10);

    let result = builtin_fround(vec![Value::make_float(1.6)]).unwrap();
    assert_float_eq(&result, 2.0, 1e-10);

    // Banker's rounding
    let result = builtin_fround(vec![Value::make_float(0.5)]).unwrap();
    assert_float_eq(&result, 0.0, 1e-10);

    let result = builtin_fround(vec![Value::make_float(1.5)]).unwrap();
    assert_float_eq(&result, 2.0, 1e-10);

    let result = builtin_fround(vec![Value::make_float(-0.5)]).unwrap();
    match result.kind() {
        ValueKind::Float => {
            let f = result.as_float().unwrap();
            assert_eq!(f, 0.0);
            assert!(f.is_sign_negative(), "expected negative zero");
        }
        _other => panic!("expected Float, got {:?}", result),
    }
}

#[test]
fn test_ftruncate() {
    crate::test_utils::init_test_tracing();
    let result = builtin_ftruncate(vec![Value::make_float(1.9)]).unwrap();
    assert_float_eq(&result, 1.0, 1e-10);

    let result = builtin_ftruncate(vec![Value::make_float(-1.9)]).unwrap();
    assert_float_eq(&result, -1.0, 1e-10);
}

// ===== Wrong type errors =====

#[test]
fn test_wrong_type_errors() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_copysign(vec![Value::string("x"), Value::make_float(1.0)]).is_err());
    assert!(builtin_copysign(vec![Value::fixnum(1), Value::make_float(1.0)]).is_err());
    assert!(builtin_fceiling(vec![Value::NIL]).is_err());
    assert!(builtin_fceiling(vec![Value::fixnum(1)]).is_err());
    assert!(builtin_ffloor(vec![Value::fixnum(1)]).is_err());
    assert!(builtin_fround(vec![Value::fixnum(1)]).is_err());
    assert!(builtin_ftruncate(vec![Value::fixnum(1)]).is_err());
    assert!(builtin_ldexp(vec![Value::make_float(1.0), Value::make_float(2.0)]).is_err());
    assert!(builtin_logb(vec![Value::T]).is_err());
    assert!(builtin_logb(vec![Value::string("y")]).is_err());
}

#[test]
fn test_ldexp_type_check_order_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let err = builtin_ldexp(vec![Value::symbol("sym"), Value::make_float(2.0)])
        .expect_err("ldexp should reject non-fixnum exponent first");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("fixnump"), Value::make_float(2.0)] // TODO(tagged): remove next_float_id()
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let err = builtin_ldexp(vec![Value::symbol("sym"), Value::fixnum(2)])
        .expect_err("ldexp should reject significand after exponent passes");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("numberp"), Value::symbol("sym")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

// ===== Wrong arity errors =====

#[test]
fn test_wrong_arity() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_logb(vec![]).is_err());
    assert!(builtin_logb(vec![Value::make_float(1.0), Value::make_float(2.0)]).is_err());
    assert!(builtin_copysign(vec![Value::make_float(1.0)]).is_err());
    assert!(builtin_ldexp(vec![Value::make_float(1.0)]).is_err());
    assert!(builtin_frexp(vec![]).is_err());
}
