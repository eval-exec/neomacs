//! Oracle parity tests for `condition-case` — nested and edge cases.
//!
//! GNU src/eval.c: `condition-case` error handling has subtle semantics
//! around nested handlers, re-signaling, error data propagation, and
//! interaction with `unwind-protect`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_condition_case_catches_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK caught""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (/ 1 0)
       (arith-error 'caught))"#,
        expect,
    );
    assert_ok_eq("caught", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_no_error_returns_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         42
       (error 'never-reached))"#,
        expect,
    );
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_error_data_is_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (error "test message")
       (error (consp err)))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_unwind_protect_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (cleanup caught)""#]];
    // Uses setq+cons instead of push (push is a cl-lib macro, not in minimal eval).
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-cc-uwp-log '())
  (condition-case nil
      (unwind-protect
          (error "inside")
        (setq neovm--test-cc-uwp-log (cons 'cleanup neovm--test-cc-uwp-log)))
    (error
     (setq neovm--test-cc-uwp-log (cons 'caught neovm--test-cc-uwp-log))))
  (nreverse neovm--test-cc-uwp-log))"#,
        expect,
    );
    assert_ok_eq("(cleanup caught)", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_re_signals_when_no_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK outer-caught""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case nil
         (condition-case nil
             (error "inner")
           (arith-error 'wrong-handler))
       (error 'outer-caught))"#,
        expect,
    );
    assert_ok_eq("outer-caught", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_multiple_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK generic-error""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (error "test")
       (arith-error 'arith)
       (error 'generic-error))"#,
        expect,
    );
    assert_ok_eq("generic-error", &oracle, &neovm);
}
