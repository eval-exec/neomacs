use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_gdl_mod_nan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (mod 5 0.0e+NaN) (mod 0.0e+NaN 5) (mod (read "7.0e+NaN") 5) (mod 5 (read "7.0e+NaN")) (mod -7.0e+NaN 5) (mod 5 -7.0e+NaN) (mod 1.0e+INF 5) (mod 5 1.0e+INF) (mod -5 3.0))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK (0.0e+NaN 0.0e+NaN 7.0e+NaN 7.0e+NaN -7.0e+NaN -7.0e+NaN -0.0e+NaN 5.0 1.0)""#
        ]],
    );
}
