//! Divergence tests: error condition shape and error hierarchy edge cases.
//!
//! These tests verify that error conditions, error data format, and
//! the error hierarchy match GNU Emacs exactly.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn divergence_error_conditions_for_wrong_type_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'wrong-type-argument 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (wrong-type-argument error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(wrong-type-argument error)", &oracle, &neovm);
}

#[test]
fn divergence_error_conditions_for_args_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'args-out-of-range 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (args-out-of-range error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(args-out-of-range error)", &oracle, &neovm);
}

#[test]
fn divergence_error_conditions_for_void_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'void-function 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (void-function error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(void-function error)", &oracle, &neovm);
}

#[test]
fn divergence_error_conditions_for_void_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'void-variable 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (void-variable error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(void-variable error)", &oracle, &neovm);
}

#[test]
fn divergence_error_conditions_for_file_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'file-error 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (file-error error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(file-error error)", &oracle, &neovm);
}

#[test]
fn divergence_error_conditions_for_file_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'file-missing 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (file-missing file-error error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(file-missing file-error error)", &oracle, &neovm);
}

#[test]
fn divergence_error_conditions_for_arith_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'arith-error 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (arith-error error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(arith-error error)", &oracle, &neovm);
}

#[test]
fn divergence_error_conditions_for_range_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(get 'range-error 'error-conditions)"#;
        let expect = expect_test::expect![[r#""OK (range-error arith-error error)""#]];
let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(range-error arith-error error)", &oracle, &neovm);
}

#[test]
fn divergence_condition_case_error_data_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (wrong-type-argument listp 42)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (signal 'wrong-type-argument '(listp 42))
  (wrong-type-argument err))"#, expect);
}

#[test]
fn divergence_condition_case_nested_handlers_first_match_wins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (error-handler (args-out-of-range 1 2 3))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (signal 'args-out-of-range '(1 2 3))
  (error (list 'error-handler err))
  (args-out-of-range (list 'args-handler err)))"#, expect);
}

#[test]
fn divergence_condition_case_with_no_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (scan-caught (scan-error \"test\"))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (progn
      (condition-case inner
          (signal 'scan-error '("test"))
        (wrong-type-argument 'wrong-type-caught))
      'outer-not-reached)
  (scan-error (list 'scan-caught err)))"#, expect);
}

#[test]
fn divergence_unwind_protect_runs_during_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK ((handler (error . \"test\")) cleanup)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
  (condition-case err
      (unwind-protect
          (signal 'error "test")
        (setq log (cons 'cleanup log)))
    (error (setq log (cons (list 'handler err) log))))
  log)"#, expect);
}

#[test]
fn divergence_condition_case_success_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (success 3)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(condition-case val
    (+ 1 2)
  (:success (list 'success val))
  (error (list 'error val)))"#, expect);
}

#[test]
fn divergence_condition_case_error_message_string_for_quit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK \"Quit\"""#]];
crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (signal 'quit nil)
  (quit (error-message-string err)))"#, expect);
}

#[test]
fn divergence_define_error_custom_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK ((test-error-parent error) (test-error-child test-error-parent error) (parent-caught (test-error-child . \"child data\")))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-error 'test-error-parent "Test parent error")
  (define-error 'test-error-child "Test child error" 'test-error-parent)
  (list
   (get 'test-error-parent 'error-conditions)
   (get 'test-error-child 'error-conditions)
   (condition-case err
       (signal 'test-error-child "child data")
     (test-error-parent (list 'parent-caught err))
     (test-error-child (list 'child-caught err)))))"#, expect);
}

#[test]
fn divergence_signal_with_nil_uses_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (error-caught (error \"Invalid error symbol\" error-msg))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (signal nil '(error-msg "data"))
  (error (list 'error-caught err)))"#, expect);
}

#[test]
fn divergence_user_error_not_quit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (error-caught quit-caught)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (condition-case err
      (signal 'user-error "test")
    (error 'error-caught)
    (user-error 'user-error-caught))
  (condition-case err
      (signal 'quit nil)
    (error 'error-caught)
    (quit 'quit-caught)))"#, expect);
}
