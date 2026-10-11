use super::*;
use malachite::integer::Integer;
#[test]
fn gdl_logb_uses_exact_integer_and_float_exponents() {
    crate::test_utils::init_test_tracing();
    for (input, exponent) in [
        (Value::fixnum(Value::MOST_POSITIVE_FIXNUM), 60),
        (Value::make_float(7.999999999999999), 2),
        (Value::make_float(f64::from_bits(1)), -1074),
        (Value::make_integer(Integer::from(1) << 1024u64), 1024),
    ] {
        assert_eq!(
            builtin_logb(vec![input]).unwrap().as_fixnum(),
            Some(exponent)
        );
    }
}
