use super::super::*;
use super::assert_gnu;

#[test]
fn take_bignum() {
    assert_gnu(
        "take_bignum",
        r#"(list (take (expt 2 100) '(1 2)) (ntake (expt 2 100) (list 1 2)) (take (- (expt 2 100)) '(1 2)) (ntake (- (expt 2 100)) (list 1 2)) (butlast '(1 2 3) (expt 2 100)) (take (1+ most-positive-fixnum) nil) (condition-case e (take (expt 2 100) 'x) (error e)) (condition-case e (take 1.0 '(1 2)) (error e)))"#,
        include_str!("take_bignum.expect"),
    );
}

#[test]
fn prefix_count_is_validated_before_the_list() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::builtins_extra::TakeCount;
    let positive = Value::make_integer(Integer::from(1u64) << 100u64);
    let negative = Value::make_integer(-(Integer::from(1u64) << 100u64));
    assert!(matches!(
        TakeCount::try_from(positive),
        Ok(TakeCount::Positive(_))
    ));
    assert!(matches!(
        TakeCount::try_from(negative),
        Ok(TakeCount::Empty)
    ));
    assert!(matches!(
        TakeCount::try_from(Value::fixnum(0)),
        Ok(TakeCount::Empty)
    ));
    assert!(TakeCount::try_from(Value::make_float(1.0)).is_err());
    assert_eq!(
        builtin_ntake(vec![negative, Value::symbol("not-a-list")]).unwrap(),
        Value::NIL
    );
}
