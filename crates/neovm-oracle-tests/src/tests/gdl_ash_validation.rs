use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_gdl_ash_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (condition-case e (ash 'x 'y) (error e)) (condition-case e (ash 1.5 'y) (error e)) (condition-case e (lsh 1.5 'y) (error e)) (condition-case e (ash 'x (expt 2 100)) (error e)) (condition-case e (funcall (byte-compile (lambda (a b) (ash a b))) 'x 'y) (error e)))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((wrong-type-argument integerp x) (wrong-type-argument integerp 1.5) (wrong-type-argument integerp 1.5) (wrong-type-argument integerp x) (wrong-type-argument integerp x))""#
        ]],
    );
}
