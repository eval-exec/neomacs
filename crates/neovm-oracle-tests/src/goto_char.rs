//! Oracle parity tests for `goto-char`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_goto_char_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle_ret, neovm_ret) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"abc\") (goto-char 2))",
        expect,
    );
    assert_ok_eq("2", &oracle_ret, &neovm_ret);

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle_point, neovm_point) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"abc\") (goto-char 2) (point))",
        expect,
    );
    assert_ok_eq("2", &oracle_point, &neovm_point);
}

#[test]
fn oracle_prop_goto_char_error_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments goto-char 0)""#]];
    let (arity_oracle, arity_neovm) =
        crate::common::eval_oracle_and_neovm_expect("(goto-char)", expect);
    assert_err_kind(&arity_oracle, &arity_neovm, "wrong-number-of-arguments");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p \"x\")""#]];
    let (type_oracle, type_neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(goto-char "x")"#, expect);
    assert_err_kind(&type_oracle, &type_neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_goto_char_updates_point(
        pos in 1usize..5usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn (erase-buffer) (insert \"abcd\") (goto-char {}) (point))",
            pos
        );
        let expected = pos.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
