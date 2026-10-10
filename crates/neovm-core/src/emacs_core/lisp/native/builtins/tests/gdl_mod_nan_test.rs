use super::*;
#[test]
fn gdl_mod_preserves_nan_sign_and_payload() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::eval::Context::new();
    for bits in [
        0x7ff8_0000_0000_0000,
        0x7ff8_0000_0000_0007,
        0xfff8_0000_0000_0007,
    ] {
        let n = Value::make_float(f64::from_bits(bits));
        for (a, b) in [(n, Value::fixnum(5)), (Value::fixnum(5), n)] {
            assert_eq!(
                builtin_mod(&mut ctx, a, b).unwrap().xfloat().to_bits(),
                bits
            );
        }
    }
}
