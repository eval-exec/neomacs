use super::*;
#[test]
fn gdl_ldexp_preserves_ieee_edge_values() {
    crate::test_utils::init_test_tracing();
    for (x, e, want) in [
        (1.0, -1074, f64::from_bits(1)),
        (-1.0, -1075, -0.0),
        (0.5, 1024, f64::from_bits(0x7fe0_0000_0000_0000)),
        (4.0, -1076, f64::from_bits(1)),
        (f64::INFINITY, -2000, f64::INFINITY),
        (-0.0, 2000, -0.0),
    ] {
        let got = builtin_ldexp(vec![Value::make_float(x), Value::fixnum(e)]).unwrap();
        assert_eq!(got.xfloat().to_bits(), want.to_bits(), "ldexp({x}, {e})");
    }
}
