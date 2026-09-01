//! Oracle parity tests for `run-hooks`.
//!
//! GNU implements `run-hooks` in `src/eval.c` via `Frun_hooks`,
//! which iterates each argument symbol and calls `run_hook`.
//! This is distinct from `run-hook-with-args` which passes explicit arguments.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_run_hooks_no_args_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(run-hooks)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_run_hooks_nil_hook_variable_no_op() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-nil-hook nil)
  (run-hooks 'neovm--test-nil-hook))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_run_hooks_calls_function_value_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-fn-hook nil)
  (defvar neovm--test-hook-called 0)
  (setq neovm--test-fn-hook
        (lambda () (setq neovm--test-hook-called (1+ neovm--test-hook-called))))
  (run-hooks 'neovm--test-fn-hook)
  neovm--test-hook-called)"#,
        expect,
    );
    assert_ok_eq("1", &oracle, &neovm);
}

#[test]
fn oracle_run_hooks_treats_raw_lambda_list_as_one_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-raw-lambda-hook nil)
  (defvar neovm--test-raw-lambda-count 0)
  (setq neovm--test-raw-lambda-hook
        '(lambda ()
           (setq neovm--test-raw-lambda-count
                 (1+ neovm--test-raw-lambda-count))))
  (list (functionp neovm--test-raw-lambda-hook)
        (run-hooks 'neovm--test-raw-lambda-hook)
        neovm--test-raw-lambda-count))"#,
        expect,
    );
    assert_ok_eq("(t nil 1)", &oracle, &neovm);
}

#[test]
fn oracle_run_hooks_calls_list_of_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 99)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-list-hook nil)
  (defvar neovm--test-count1 0)
  (defvar neovm--test-count2 0)
  (setq neovm--test-list-hook
        (list
         (lambda () (setq neovm--test-count1 42))
         (lambda () (setq neovm--test-count2 99))))
  (run-hooks 'neovm--test-list-hook)
  (list neovm--test-count1 neovm--test-count2))"#,
        expect,
    );
    assert_ok_eq("(42 99)", &oracle, &neovm);
}

#[test]
fn oracle_run_hooks_multiple_hooks_in_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (a b)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-multi-a nil)
  (defvar neovm--test-multi-b nil)
  (defvar neovm--test-order '())
  (setq neovm--test-multi-a
        (lambda () (setq neovm--test-order (cons 'a neovm--test-order))))
  (setq neovm--test-multi-b
        (lambda () (setq neovm--test-order (cons 'b neovm--test-order))))
  (run-hooks 'neovm--test-multi-a 'neovm--test-multi-b)
  (nreverse neovm--test-order))"#,
        expect,
    );
    assert_ok_eq("(a b)", &oracle, &neovm);
}

#[test]
fn oracle_run_hooks_returns_nil_even_when_hook_returns_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-ret-hook
    (lambda () 'some-return-value))
  (run-hooks 'neovm--test-ret-hook))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_run_hooks_symbolp_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 42)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(run-hooks 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
