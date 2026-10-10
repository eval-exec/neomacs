use super::super::*;
use super::assert_gnu;

#[test]
fn value_lt_exact() {
    assert_gnu(
        "value_lt_exact",
        r#"(list (value< most-positive-fixnum (float most-positive-fixnum)) (value< 9007199254740992.0 9007199254740993) (value< 9007199254740993 9007199254740992.0) (value< -9007199254740993 -9007199254740992.0) (value< -9007199254740992.0 -9007199254740993) (value< 1 1.5) (value< 1.5 1) (value< 1 0.0e+NaN) (value< 0.0e+NaN 1) (value< most-positive-fixnum 1.0e+INF) (value< -1.0e+INF most-negative-fixnum) (sort (list 9007199254740993 9007199254740992.0)))"#,
        include_str!("value_lt_exact.expect"),
    );
}

#[test]
fn value_lt_orders_adjacent_integer_and_float_exactly() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let integer = Value::fixnum(9_007_199_254_740_993);
    let float = Value::make_float(9_007_199_254_740_992.0);
    assert_eq!(
        super::super::super::symbols::builtin_value_lt(&mut context, vec![float, integer]).unwrap(),
        Value::T
    );
    assert_eq!(
        super::super::super::symbols::builtin_value_lt(&mut context, vec![integer, float]).unwrap(),
        Value::NIL
    );
}

#[test]
fn value_lt_numeric_decoding_preserves_fixnum_boundaries_and_nan() {
    use std::cmp::Ordering;
    crate::test_utils::init_test_tracing();
    let context = Context::new();
    let compare = |left, right| {
        super::super::super::symbols::compare_value_lt(&context, &left, &right).unwrap()
    };
    // GNU fns.c:3091-3100: conversion ties must compare in the integer
    // domain, including the rounded value just above the fixnum ceiling.
    for (integer, float, expected) in [
        (
            9_007_199_254_740_993,
            9_007_199_254_740_992.0,
            Ordering::Greater,
        ),
        (
            -9_007_199_254_740_993,
            -9_007_199_254_740_992.0,
            Ordering::Less,
        ),
        (
            Value::MOST_POSITIVE_FIXNUM,
            Value::MOST_POSITIVE_FIXNUM as f64,
            Ordering::Less,
        ),
        (
            Value::MOST_NEGATIVE_FIXNUM,
            Value::MOST_NEGATIVE_FIXNUM as f64,
            Ordering::Equal,
        ),
        (0, -0.0, Ordering::Equal),
        (1, 1.5, Ordering::Less),
        (-1, -1.5, Ordering::Greater),
        (1, f64::INFINITY, Ordering::Less),
        (1, f64::NEG_INFINITY, Ordering::Greater),
        (1, f64::NAN, Ordering::Equal),
    ] {
        let left = Value::fixnum(integer);
        let right = Value::make_float(float);
        assert_eq!(compare(left, right), expected, "{integer} versus {float}");
        assert_eq!(compare(right, left), expected.reverse());
    }
    // GNU fns.c:3265-3274: either-side NaN is incomparable, rather than
    // assigned a position by a binary64 total-order implementation.
    for float in [f64::NEG_INFINITY, -0.0, 0.0, 1.5, f64::INFINITY, f64::NAN] {
        let value = Value::make_float(float);
        let nan = Value::make_float(f64::NAN);
        assert_eq!(compare(value, nan), Ordering::Equal);
        assert_eq!(compare(nan, value), Ordering::Equal);
    }
}

#[test]
fn value_lt_borrows_large_integer_operands_without_losing_mixed_order() {
    use malachite::integer::Integer;
    use std::cmp::Ordering;
    crate::test_utils::init_test_tracing();
    let context = Context::new();
    let compare = |left, right| {
        super::super::super::symbols::compare_value_lt(&context, &left, &right).unwrap()
    };
    let magnitude = Integer::from(1) << 500_u32;
    let big = Value::make_integer(magnitude.clone());
    let next = Value::make_integer(&magnitude + Integer::from(1));
    let negative = Value::make_integer(-magnitude);
    let small = Value::fixnum(1);
    let float = Value::make_float(2.0_f64.powi(500));
    assert_eq!(compare(big, next), Ordering::Less);
    assert_eq!(compare(next, big), Ordering::Greater);
    assert_eq!(compare(big, small), Ordering::Greater);
    assert_eq!(compare(small, big), Ordering::Less);
    assert_eq!(compare(negative, small), Ordering::Less);
    assert_eq!(compare(small, negative), Ordering::Greater);
    // GNU fns.c:3272-3274 uses exact mpz_cmp_d, not a rounded integer.
    assert_eq!(compare(big, float), Ordering::Equal);
    assert_eq!(compare(next, float), Ordering::Greater);
    assert_eq!(compare(float, next), Ordering::Less);
    let nan = Value::make_float(f64::NAN);
    assert_eq!(compare(big, nan), Ordering::Equal);
    assert_eq!(compare(nan, big), Ordering::Equal);
}
