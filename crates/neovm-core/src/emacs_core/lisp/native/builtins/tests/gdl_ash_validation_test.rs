use super::*;
#[test]
fn gdl_ash_checks_value_before_count() {
    crate::test_utils::init_test_tracing();
    let bad = Value::symbol("x");
    for count in [
        Value::symbol("y"),
        Value::make_integer(Integer::from(1) << 100u64),
    ] {
        let error = builtin_ash_slice(&[bad, count]).unwrap_err();
        let error = error.as_signal().unwrap();
        assert_eq!(error.symbol, intern("wrong-type-argument"));
        assert_eq!(error.data, vec![Value::symbol("integerp"), bad]);
    }
}
