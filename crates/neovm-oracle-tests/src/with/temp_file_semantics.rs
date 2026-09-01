//! Oracle parity tests for GNU `with-temp-file` semantics.
//!
//! GNU's `with-temp-file` macro evaluates BODY in a generated temp buffer,
//! returns BODY's last value, writes the buffer only after normal completion,
//! and kills the temp buffer in cleanup without running normal kill-buffer
//! hooks.  These tests pin that contract directly.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_with_temp_file_writes_after_body_and_returns_body_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((file (make-temp-file "neomacs-with-temp-file-"))
      (outer (current-buffer))
      (seen nil)
      (result nil))
  (unwind-protect
      (progn
        (with-temp-file file
          (setq seen
                (list (eq (current-buffer) outer)
                      (string-prefix-p " *temp file*" (buffer-name))
                      buffer-file-name
                      (buffer-string)
                      (buffer-modified-p)
                      (file-exists-p file)
                      (with-temp-buffer
                        (insert-file-contents file)
                        (buffer-string))))
          (insert "alpha\n")
          (insert (format "%S" '(beta gamma)))
          (setq result :body-value))
        (list result
              seen
              (eq (current-buffer) outer)
              (with-temp-buffer
                (insert-file-contents file)
                (buffer-string))))
    (when (file-exists-p file)
      (delete-file file))))
"#;

    let expect = expect_test::expect![[
        r#""OK (:body-value (nil t nil \"\" nil t \"\") t \"alpha\\n(beta gamma)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_file_does_not_write_file_when_body_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((file (make-temp-file "neomacs-with-temp-file-error-"))
      (temp-buffer nil)
      (initial nil))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert "original"))
        (setq initial
              (with-temp-buffer
                (insert-file-contents file)
                (buffer-string)))
        (condition-case err
            (with-temp-file file
              (setq temp-buffer (current-buffer))
              (insert "replacement")
              (error "boom"))
          (error
           (list (car err)
                 (cadr err)
                 (buffer-live-p temp-buffer)
                 initial
                 (with-temp-buffer
                   (insert-file-contents file)
                   (buffer-string)))))))
    (when (file-exists-p file)
      (delete-file file))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 27 27)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_file_cleanup_skips_kill_buffer_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((file (make-temp-file "neomacs-with-temp-file-hooks-"))
      (hook-events nil)
      (temp-name nil)
      (temp-buffer nil))
  (unwind-protect
      (let ((kill-buffer-hook
             (list (lambda ()
                     (push (list 'kill-buffer-hook (buffer-name)) hook-events))))
            (kill-buffer-query-functions
             (list (lambda ()
                     (push (list 'kill-buffer-query-functions (buffer-name)) hook-events)
                     t)))
            (buffer-list-update-hook
             (list (lambda ()
                     (push 'buffer-list-update-hook hook-events)))))
        (list
         (with-temp-file file
           (setq temp-name (buffer-name)
                 temp-buffer (current-buffer))
           (insert "hook-test")
           :done)
         (buffer-live-p temp-buffer)
         (get-buffer temp-name)
         (nreverse hook-events)
         (with-temp-buffer
           (insert-file-contents file)
           (buffer-string))))
    (when (file-exists-p file)
      (delete-file file))))
"#;

    let expect = expect_test::expect![[r#""OK (:done nil nil nil \"hook-test\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
