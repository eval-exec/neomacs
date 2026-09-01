//! Oracle parity tests for `forward-char`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_forward_char_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"abcd\") (goto-char 1) (forward-char 2) (point))",
        expect,
    );
    assert_ok_eq("3", &oracle, &neovm);
}

#[test]
fn oracle_prop_forward_char_error_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump \"x\")""#]];
    let (type_oracle, type_neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(forward-char "x")"#, expect);
    assert_err_kind(&type_oracle, &type_neovm, "wrong-type-argument");

    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    let (eob_oracle, eob_neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"a\") (goto-char 1) (forward-char 10))",
        expect,
    );
    assert_err_kind(&eob_oracle, &eob_neovm, "end-of-buffer");

    let expect = expect_test::expect![[r#""ERR (beginning-of-buffer)""#]];
    let (bob_oracle, bob_neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"a\") (goto-char 1) (forward-char -1))",
        expect,
    );
    assert_err_kind(&bob_oracle, &bob_neovm, "beginning-of-buffer");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_forward_char_parity_with_normalized_error(
        n in -8i64..8i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (erase-buffer)
               (insert \"abcd\")
               (goto-char 2)
               (condition-case err
                   (progn (forward-char {}) (list 'ok (point)))
                 (error (list 'err (car err) (point)))))",
            n
        );
        assert_oracle_parity(&form);
    }
}
