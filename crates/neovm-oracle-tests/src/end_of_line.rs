//! Oracle parity tests for `end-of-line`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_end_of_line_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 4""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"abc\") (goto-char 1) (end-of-line) (point))",
        expect,
    );
    assert_ok_eq("4", &oracle, &neovm);
}

#[test]
fn oracle_prop_end_of_line_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump \"x\")""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(end-of-line "x")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_end_of_line_optional_n_parity(
        n in 1i64..5i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn (erase-buffer) (insert \"a\\nb\\nc\\nd\\n\") (goto-char 5) (list (end-of-line {}) (point)))",
            n
        );
        assert_oracle_parity(&form);
    }
}
