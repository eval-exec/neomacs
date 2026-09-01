//! Divergence tests: error conditions, condition-case, signal deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_condition_case_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((caught (error . \"test\")) (caught (wrong-type-argument . \"test\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (condition-case err
      (signal 'error "test")
    (error (list 'caught err)))
  (condition-case err
      (signal 'wrong-type-argument "test")
    (wrong-type-argument (list 'caught err))
    (error (list 'caught-error err)))) "#,
        expect,
    );
}

#[test]
fn divergence_condition_case_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (args-caught)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (condition-case err
      (signal 'args-out-of-range '(0 10))
    (args-out-of-range 'args-caught)
    (wrong-type-argument 'wrong-type-caught)
    (error 'error-caught))) "#,
        expect,
    );
}

#[test]
fn divergence_error_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'define-error)
  (fboundp 'signal)
  (fboundp 'error)
  (fboundp 'warn)
  (fboundp 'user-error)
  (fboundp 'message)) "#,
        expect,
    );
}

#[test]
fn divergence_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error . \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((result nil))
  (unwind-protect
      (progn (push 'body result)
             (signal 'error "test"))
    (push 'cleanup result))
  result) "#,
        expect,
    );
}

#[test]
fn divergence_with_condition_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (handler cleanup body)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((result nil))
  (condition-case err
      (unwind-protect
          (progn
            (push 'body result)
            (signal 'error "test"))
        (push 'cleanup result))
    (error (push 'handler result)))
  result) "#,
        expect,
    );
}

#[test]
fn divergence_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'error-message-string)
  (stringp (error-message-string '(error "test message")))
  (fboundp 'format-message)) "#,
        expect,
    );
}

#[test]
fn divergence_debug_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'debug-on-error)
  (booleanp debug-on-error)
  (boundp 'debug-on-quit)
  (booleanp debug-on-quit)) "#,
        expect,
    );
}

#[test]
fn divergence_signal_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t user-error-caught)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'signal)
  (fboundp 'error)
  (fboundp 'user-error)
  (condition-case nil
      (user-error "test")
    (user-error 'user-error-caught)
    (error 'error-caught))) "#,
        expect,
    );
}

#[test]
fn divergence_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (caught wrong-type-argument (listp \"not-a-list\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (car "not-a-list")
  (wrong-type-argument
   (list 'caught (car err) (cdr err)))) "#,
        expect,
    );
}

#[test]
fn divergence_void_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (nonexistent-function-xyz-123)
  (void-function
   (list 'caught (car err)))
  (error
   (list 'caught-error (car err)))) "#,
        expect,
    );
}
