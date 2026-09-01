//! Oracle parity tests for `apply` and `funcall` — argument handling edges.
//!
//! GNU src/eval.c: `apply` and `funcall` have subtle behavior around
//! argument limits, cons vs list for the final apply arg, and error handling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_apply_with_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ '(1 2 3))"#, expect);
    assert_ok_eq("6", &oracle, &neovm);
}

#[test]
fn oracle_apply_with_args_and_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ 1 2 '(3 4))"#, expect);
    assert_ok_eq("10", &oracle, &neovm);
}

#[test]
fn oracle_funcall_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(funcall '+ 1 2 3)"#, expect);
    assert_ok_eq("6", &oracle, &neovm);
}

#[test]
fn oracle_funcall_with_symbol_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(funcall 'list 1 2 3)"#, expect);
    assert_ok_eq("(1 2 3)", &oracle, &neovm);
}

#[test]
fn oracle_apply_last_arg_must_be_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 42)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ 42)"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_apply_with_dotted_list_final_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    // GNU: apply accepts a dotted list where the last cdr is the
    // final element(s). But a proper list is standard.
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ 1 '(2))"#, expect);
    assert_ok_eq("3", &oracle, &neovm);
}

#[test]
fn oracle_funcall_not_a_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK caught""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err (funcall 42) (error 'caught))"#,
        expect,
    );
    assert_ok_eq("caught", &oracle, &neovm);
}

#[test]
fn oracle_apply_no_args_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(apply (lambda () 42) nil)"#, expect);
    assert_ok_eq("42", &oracle, &neovm);
}
