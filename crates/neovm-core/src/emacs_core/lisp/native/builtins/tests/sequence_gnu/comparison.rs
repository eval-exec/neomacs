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
