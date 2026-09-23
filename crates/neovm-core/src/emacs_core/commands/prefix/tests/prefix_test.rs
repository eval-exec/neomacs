use super::*;

#[test]
fn raw_prefix_symbol_domain_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        RawPrefixSymbol::from_lisp_value(&Value::symbol("-")),
        Some(RawPrefixSymbol::Minus)
    );
    assert_eq!(RawPrefixSymbol::Minus.name(), "-");
    assert_eq!(RawPrefixSymbol::from_lisp_value(&Value::symbol("+")), None);
}

#[test]
fn prefix_numeric_value_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(prefix_numeric_value(&Value::NIL), 1);
    assert_eq!(prefix_numeric_value(&Value::symbol("-")), -1);
    assert_eq!(prefix_numeric_value(&Value::fixnum(4)), 4);
    assert_eq!(
        prefix_numeric_value(&Value::cons(Value::fixnum(16), Value::NIL)),
        16
    );
    assert_eq!(prefix_numeric_value(&Value::make_float(2.0)), 1);
    assert_eq!(
        prefix_numeric_value(&Value::cons(Value::make_float(2.0), Value::NIL)),
        1
    );
    assert_eq!(prefix_numeric_value(&Value::symbol("other")), 1);
}
