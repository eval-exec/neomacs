//! Oracle parity tests for `with-output-to-temp-buffer` and `standard-output`.
//!
//! GNU's `with-output-to-temp-buffer` is not equivalent to
//! `with-temp-buffer`: BODY does not run with the output buffer current; it
//! binds `standard-output` to the temp buffer, clears and prepares that buffer,
//! then shows it only after normal BODY completion.  These tests exercise that
//! exact contract without substituting a different output path.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_with_output_to_temp_buffer_body_not_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((name " *neovm-output-body-not-current*")
      (outer (current-buffer)))
  (unwind-protect
      (let ((body-result
             (with-output-to-temp-buffer name
               (list
                (eq (current-buffer) outer)
                (eq standard-output (get-buffer name))
                (buffer-name (current-buffer))
                (progn
                  (princ "alpha")
                  (prin1 '(1 "two"))
                  (terpri)
                  (princ "omega")
                  :done)))))
        (list body-result
              (with-current-buffer name
                (list (buffer-string)
                      (point)
                      (buffer-modified-p)
                      buffer-file-name
                      (eq buffer-undo-list t)))))
    (when (get-buffer name)
      (kill-buffer name))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((t t \"*scratch*\" :done) (\"alpha(1 \\\"two\\\")\\nomega\\n\" 1 nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_output_to_temp_buffer_clears_existing_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((name " *neovm-output-clears-existing*"))
  (unwind-protect
      (progn
        (with-current-buffer (get-buffer-create name)
          (insert "stale text")
          (setq buffer-file-name "/tmp/not-a-real-output-file")
          (setq buffer-undo-list nil)
          (set-buffer-modified-p t))
        (let ((first (with-output-to-temp-buffer name
                       (princ "fresh")
                       (buffer-name (current-buffer)))))
          (list first
                (with-current-buffer name
                  (list (buffer-string)
                        buffer-file-name
                        (eq buffer-undo-list t)
                        (buffer-modified-p)
                        (point-min)
                        (point-max))))))
    (when (get-buffer name)
      (kill-buffer name))))
"#;

    let expect = expect_test::expect![[r#""OK (\"*scratch*\" (\"fresh\\n\" nil t nil 1 7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_output_to_temp_buffer_setup_hook_current_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((name " *neovm-output-setup-hook*")
      (seen nil))
  (unwind-protect
      (let ((temp-buffer-setup-hook
             (list (lambda ()
                     (setq seen
                           (list (buffer-name (current-buffer))
                                 (eq (current-buffer) (get-buffer name))
                                 (eq standard-output (get-buffer name))
                                 buffer-read-only
                                 buffer-file-name
                                 (eq buffer-undo-list t)))
                     (insert "hook:")))))
        (let ((body (with-output-to-temp-buffer name
                      (princ "body")
                      'body-value)))
          (list body
                seen
                (with-current-buffer name
                  (buffer-string)))))
    (when (get-buffer name)
      (kill-buffer name))))
"#;

    let expect = expect_test::expect![[
        r#""OK (body-value (\" *neovm-output-setup-hook*\" t nil nil nil t) \"hook:body\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_output_to_temp_buffer_no_show_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((name " *neovm-output-error*")
      (show-called nil))
  (unwind-protect
      (let ((temp-buffer-show-function
             (lambda (_buffer)
               (setq show-called t))))
        (list
         (condition-case err
             (with-output-to-temp-buffer name
               (princ "partial")
               (error "boom"))
           (error (list (car err) (cadr err))))
         show-called
         (with-current-buffer name
           (list (buffer-string)
                 (buffer-modified-p)
                 (point)))))
    (when (get-buffer name)
      (kill-buffer name))))
"#;

    let expect = expect_test::expect![[r#""OK ((error \"boom\") nil (\"partial\" t 8))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
