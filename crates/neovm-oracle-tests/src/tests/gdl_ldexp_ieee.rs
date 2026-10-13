use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_gdl_ldexp_ieee() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (ldexp 1.0 -1074) (ldexp -1.0 -1075) (ldexp 0.5 1024) (ldexp 1e300 -1100) (ldexp 1.0e+INF -2000) (ldexp -0.0 2000) (ldexp 7.0e+NaN -2000) (ldexp 4.0 -1076) (ldexp 1.0 most-positive-fixnum) (ldexp -0.0 most-negative-fixnum))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK (5e-324 -0.0 8.98846567431158e+307 7.362151829022863e-32 1.0e+INF -0.0 7.0e+NaN 5e-324 1.0e+INF -0.0)""#
        ]],
    );
}
