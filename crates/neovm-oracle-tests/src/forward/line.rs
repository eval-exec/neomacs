//! Oracle parity tests for `forward-line`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_forward_line_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"a\\nb\\nc\\n\") (goto-char 1) (forward-line 1))",
        expect,
    );
    assert_ok_eq("0", &oracle, &neovm);

    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn (erase-buffer) (insert \"a\\nb\\nc\\n\") (goto-char 1) (forward-line 10))",
        expect,
    );
}

#[test]
fn oracle_prop_forward_line_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integerp \"x\")""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(forward-line "x")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_forward_line_remainder_and_point(
        n in -10i64..10i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn (erase-buffer) (insert \"a\\nb\\nc\\n\") (goto-char 1) (list (forward-line {}) (point)))",
            n
        );
        assert_oracle_parity(&form);
    }
}
