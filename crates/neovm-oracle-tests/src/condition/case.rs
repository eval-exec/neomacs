//! Oracle parity tests for `condition-case`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_condition_case_handles_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(condition-case nil (/ 1 0) (arith-error 42))",
        expect,
    );
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_prop_condition_case_no_error_passthrough() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(condition-case nil (+ 1 2) (error 0))",
        expect,
    );
    assert_ok_eq("3", &oracle, &neovm);
}

#[test]
fn oracle_prop_condition_case_error_symbol_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK arith-error""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (/ 1 0) (arith-error (car err)))",
        expect,
    );
}
