//! Oracle parity tests for coding-system primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_coding_system_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (coding-system-p 'utf-8-unix) (coding-system-p 'utf-8) (coding-system-base 'utf-8-unix) (coding-system-eol-type 'utf-8-unix) (check-coding-system 'utf-8-unix))";
    let expect = expect_test::expect![[r#""OK (t t utf-8 0 utf-8-unix)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t t utf-8 0 utf-8-unix)", &oracle, &neovm);
}

#[test]
fn oracle_prop_check_coding_system_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(check-coding-system 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_coding_system_unknown_symbol_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (coding-system-error neovm-no-such-coding)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(coding-system-base 'neovm-no-such-coding)",
        expect,
    );
    assert_err_kind(&oracle, &neovm, "coding-system-error");
}
