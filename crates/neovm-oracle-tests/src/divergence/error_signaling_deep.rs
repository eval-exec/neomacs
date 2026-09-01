//! Divergence tests: error signaling deep - custom errors, error symbols, debug.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_signal_custom_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Unknown signal ‘my-category’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-error 'my-custom-error "A custom error" '(error my-category))
  (condition-case err
      (signal 'my-custom-error '(42 "details"))
    (my-custom-error (list (car err) (cdr err)))
    (error (list 'caught-general err))))"#,
        expect,
    );
}

#[test]
fn divergence_signal_with_list_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (error (a b c))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  (signal 'error '((a b c)))
  (error (list (car err) (cadr err))))"#,
        expect,
    );
}

#[test]
fn divergence_signal_wrong_number_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\\\"\" 3 60)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  (car 1 2 3)
  (wrong-number-of-arguments (list (car err) (cadr err))))#" ,
    );
}

#[test]
fn divergence_signal_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(condition-case err
  (+ "string" 42)
  (wrong-type-argument (list (car err) (cadr err))))"#,
        expect,
    );
}

#[test]
fn divergence_signal_void_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (void-function nonexistent-function-xyz-123)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  (nonexistent-function-xyz-123)
  (void-function (list (car err) (cadr err))))"#,
        expect,
    );
}

#[test]
fn divergence_signal_void_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (void-variable nonexistent-variable-xyz-456)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  nonexistent-variable-xyz-456
  (void-variable (list (car err) (cadr err))))"#,
        expect,
    );
}

#[test]
fn divergence_signal_args_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (args-out-of-range ([1 2 3] 10))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  (aref [1 2 3] 10)
  (args-out-of-range (list (car err) (cdr err))))"#,
        expect,
    );
}

#[test]
fn divergence_signal_cyclic_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (#0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  (let ((x (list 'a)))
    (setcar x x)
    x)
  (circular-list (list 'caught-circular))
  (error (list 'caught-error (car err))))"#,
        expect,
    );
}

#[test]
fn divergence_error_message_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"test message 42\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  (error "test message %d" 42)
  (error (error-message-string err)))"#,
        expect,
    );
}

#[test]
fn divergence_user_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (user-error \"user message test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
  (user-error "user message %s" "test")
  (user-error (list (car err) (error-message-string err)))
  (error (list 'caught-error err)))"#,
        expect,
    );
}
