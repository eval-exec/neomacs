//! Divergence tests: complex error recovery + resource cleanup scenarios.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_temp_buffer_cleanup_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((caught error) t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((buf-count-before (length (buffer-list)))
        (result (condition-case err
                    (with-temp-buffer
                      (insert \"temp data\")
                      (error \"forced error\"))
                  (error (list 'caught (car err))))))
  (list result
        (= (length (buffer-list)) buf-count-before))) ",
        expect,
    );
}

#[test]
fn divergence_unwind_protect_buffer_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"test error\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((temp-buf nil)
        (log nil))
  (unwind-protect
      (progn
        (setq temp-buf (generate-new-buffer \"*test-cleanup*\"))
        (with-current-buffer temp-buf (insert \"data\"))
        (push 'before-error log)
        (error \"test error\"))
    (push 'cleanup log)
    (when (and temp-buf (buffer-live-p temp-buf))
      (kill-buffer temp-buf)
      (push 'buffer-killed log)))
  (nreverse log)) ",
        expect,
    );
}

#[test]
fn divergence_nested_unwind_with_multiple_cleanups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cleanup-1 cleanup-2 cleanup-3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((log nil))
  (ignore-errors
    (unwind-protect
        (unwind-protect
            (unwind-protect
                (error \"inner\")
              (push 'cleanup-1 log))
          (push 'cleanup-2 log))
      (push 'cleanup-3 log)))
  (nreverse log)) ",
        expect,
    );
}

#[test]
fn divergence_condition_case_error_data_integrity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument 2 (listp 42)) nil (arith-error nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (condition-case err
      (car 42)
    (wrong-type-argument (list (car err) (length (cdr err)) (cdr err))))
  (condition-case err
      (nth 10 '(a b c))
    (args-out-of-range (list (car err) (cdr err))))
  (condition-case err
      (/ 1 0)
    (arith-error (list (car err) (cdr err))))) ",
        expect,
    );
}

#[test]
fn divergence_save_excursion_restore_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 11 11)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((orig-point (point)))
    (ignore-errors
      (save-excursion
        (goto-char 5)
        (error \"oops\")))
    (list (= (point) orig-point)
          (point)
          orig-point))) ",
        expect,
    );
}

#[test]
fn divergence_save_restriction_restore_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 7 3 7 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (narrow-to-region 3 7)
  (let ((orig-min (point-min)) (orig-max (point-max)))
    (ignore-errors
      (save-restriction
        (widen)
        (error \"oops\")))
    (list (point-min) (point-max) orig-min orig-max
          (= (point-min) orig-min)
          (= (point-max) orig-max)))) ",
        expect,
    );
}

#[test]
fn divergence_marker_recovery_after_failed_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 \"ABCD123EFGHIJ\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((m (set-marker (make-marker) 5)))
    (undo-boundary)
    (ignore-errors
      (goto-char 5)
      (insert \"123\")
      (error \"fail\"))
    (list (marker-position m)
          (buffer-string)))) ",
        expect,
    );
}

#[test]
fn deficiency_error_message_formatting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"test hello 42\" \"Wrong type argument: listp, 42\" \"Args out of range: 10, 5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (condition-case err
      (error \"test %s %d\" \"hello\" 42)
    (error (error-message-string err)))
  (condition-case err
      (signal 'wrong-type-argument '(listp 42))
    (wrong-type-argument (error-message-string err)))
  (condition-case err
      (signal 'args-out-of-range '(10 5))
    (args-out-of-range (error-message-string err)))) ",
        expect,
    );
}

#[test]
fn divergence_signal_vs_error_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((generic (\"Invalid error symbol\" custom-error-signal-xxx)) (error-handler (\"generic\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (condition-case err
      (signal 'custom-error-signal-xxx '(\"data\"))
    (custom-error-signal-xxx (list 'custom (cdr err)))
    (error (list 'generic (cdr err))))
  (condition-case err
      (signal 'error '(\"generic\"))
    (error (list 'error-handler (cdr err))))) ",
        expect,
    );
}

#[test]
fn divergence_user_error_vs_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((user \"user mistake\") (error \"real error\"))""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (condition-case err
      (user-error \"user mistake\")
    (user-error (list 'user (error-message-string err)))
    (error (list 'error (error-message-string err))))
  (condition-case err
      (error \"real error\")
    (user-error (list 'user (error-message-string err)))
    (error (list 'error (error-message-string err))))) ",
        expect,
    );
}
