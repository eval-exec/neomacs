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
    use crate::emacs_core::buffer::builtin_ntake;
    use crate::emacs_core::builtins_extra::builtin_take;
    use crate::emacs_core::error::{EvalResult, FlowKind};

    fn assert_wrong_type(result: EvalResult, predicate: Value, original: Value) {
        let error = result.expect_err("invalid argument must signal");
        let FlowKind::Signal(signal) = error.into_kind() else {
            panic!("expected a type-error signal");
        };
        assert_eq!(signal.symbol, intern("wrong-type-argument"));
        assert_eq!(signal.data, vec![predicate, original]);
    }

    type PrefixBuiltin = fn(Vec<Value>) -> EvalResult;
    let positive = Value::make_integer(Integer::from(1u64) << 100u64);
    let negative = Value::make_integer(-(Integer::from(1u64) << 100u64));
    let invalid_list = Value::symbol("not-a-list");
    let float_count = Value::make_float(1.0);
    // GNU fns.c:1675-1687 and1720-1729 validate counts before CHECK_LIST.
    // Both APIs already exist on the baseline: this test does not import the
    // new count-decoding type whose behavior it verifies.
    for (name, prefix) in [
        ("take", builtin_take as PrefixBuiltin),
        ("ntake", builtin_ntake as PrefixBuiltin),
    ] {
        let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
        let result = prefix(vec![positive, list]).expect("positive bignum retains the full list");
        assert!(result.is_cons(), "{name}: expected the first cell");
        assert_eq!(result.cons_car(), Value::fixnum(1), "{name}");
        let tail = result.cons_cdr();
        assert!(tail.is_cons(), "{name}: expected the second cell");
        assert_eq!(tail.cons_car(), Value::fixnum(2), "{name}");
        assert!(tail.cons_cdr().is_nil(), "{name}: no extra prefix elements");
        assert_eq!(
            prefix(vec![positive, Value::NIL]).unwrap(),
            Value::NIL,
            "{name}"
        );
        for count in [negative, Value::fixnum(-1), Value::fixnum(0)] {
            assert_eq!(
                prefix(vec![count, invalid_list]).unwrap(),
                Value::NIL,
                "{name}"
            );
        }
        // A noninteger count wins over the invalid list and reports itself.
        assert_wrong_type(
            prefix(vec![float_count, invalid_list]),
            Value::symbol("integerp"),
            float_count,
        );
        // Once the count is accepted, preserve the original list argument in
        // error data, for both machine-size and bignum positive counts.
        for count in [Value::fixnum(1), positive] {
            assert_wrong_type(
                prefix(vec![count, invalid_list]),
                Value::symbol("listp"),
                invalid_list,
            );
        }
    }
}
