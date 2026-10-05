use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_gdl_logb_exact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(mapcar #'logb (list most-positive-fixnum (1- (expt 2 53)) 7.999999999999999 15.999999999999998 (1- (expt 2 100)) (expt 2 1024) (expt 7 3000) 5e-324 0 -0.0 1.0e+INF -1.0e+INF 7.0e+NaN))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK (60 52 2 3 99 1024 8422 -1074 -1.0e+INF -1.0e+INF 1.0e+INF 1.0e+INF 7.0e+NaN)""#
        ]],
    );
}
