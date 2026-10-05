use super::*;
#[test]
fn gdl_random_bignum_range_and_negative_limit() {
    crate::test_utils::init_test_tracing();
    let limit = Value::make_integer(Integer::from(1) << 100u64);
    for _ in 0..128 {
        let r = builtin_random(vec![limit]).unwrap();
        let r = bignum_or_int_to_integer(&r).unwrap();
        assert!(r >= 0 && r < *limit.as_bignum().unwrap());
    }
    let bad = Value::make_integer(-(Integer::from(1) << 100u64));
    let err = builtin_random(vec![bad]).unwrap_err();
    let err = err.as_signal().unwrap();
    assert_eq!(err.symbol, intern("args-out-of-range"));
    assert_eq!(err.data, vec![bad]);
}
