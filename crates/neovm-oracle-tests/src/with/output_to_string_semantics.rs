//! Oracle parity tests for GNU `with-output-to-string` semantics.
//!
//! GNU implements this macro in `subr.el` by generating a hidden buffer,
//! dynamically rebinding `standard-output` only while BODY runs, returning the
//! buffer contents, and killing the buffer in `unwind-protect`.  These tests
//! pin that contract beyond simple `princ`/`prin1` output cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_with_output_to_string_dynamic_output_buffer_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((outer-buffer (current-buffer))
      (seen nil)
      (after-body nil))
  (list
   (with-output-to-string
     (setq seen
           (list (bufferp standard-output)
                 (eq (current-buffer) outer-buffer)
                 (with-current-buffer standard-output
                   (list (buffer-name)
                         (buffer-string)
                         (point-min)
                         (point-max)
                         buffer-file-name
                         (buffer-modified-p)))))
     (princ "alpha")
     (prin1 '(beta "gamma"))
     (setq after-body :body-result))
   seen
   after-body
   (eq (current-buffer) outer-buffer)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"alpha(beta \\\"gamma\\\")\" (t t (\" *string-output*\" \"\" 1 1 nil nil)) :body-result t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_output_to_string_kills_temp_buffer_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((captured-buffer nil)
      (captured-name nil)
      (inside-live nil))
  (condition-case err
      (with-output-to-string
        (setq captured-buffer standard-output
              captured-name (buffer-name standard-output)
              inside-live (buffer-live-p standard-output))
        (princ "partial")
        (error "boom"))
    (error
     (list (car err)
           (cadr err)
           inside-live
           (buffer-live-p captured-buffer)
           (get-buffer captured-name)))))
"#;

    let expect = expect_test::expect![[r#""OK (error \"boom\" t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_output_to_string_nested_capture_isolated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((inner-result nil)
      (inner-buffer-name nil)
      (outer-sees-before nil)
      (outer-sees-after nil))
  (list
   (with-output-to-string
     (princ "outer-a:")
     (setq outer-sees-before
           (with-current-buffer standard-output
             (buffer-string)))
     (setq inner-result
           (with-output-to-string
             (setq inner-buffer-name (buffer-name standard-output))
             (princ "inner")))
     (setq outer-sees-after
           (with-current-buffer standard-output
             (buffer-string)))
     (princ ":outer-b"))
   inner-result
   outer-sees-before
   outer-sees-after
   (get-buffer inner-buffer-name)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"outer-a::outer-b\" \"inner\" \"outer-a:\" \"outer-a:\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
