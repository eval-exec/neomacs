//! Strict combo oracle probes, batch 366: format-mode-line :eval/:propertize
//! deep. :eval dynamic computation, :propertize face runs, list constructs,
//! and mode-line-format variable access.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_format_mode_line_eval_propertize_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-fml*")
  (fundamental-mode)
  (insert "content")
  (let ((result (list (format-mode-line '(:eval (buffer-name)))
                      (format-mode-line '(:propertize "X" face bold))
                      (format-mode-line '("[" mode-name "]"))
                      (format-mode-line '(:eval (number-to-string (point-max)))))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_mode_line_width_truncation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-fmlw*")
  (fundamental-mode)
  (insert "hello world test")
  (let ((result (list (stringp (format-mode-line mode-line-format))
                      (> (length (format-mode-line mode-line-format)) 0)
                      (format-mode-line "%b")
                      (format-mode-line "%m")
                      (format-mode-line "%l"))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""OK (t nil \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_mode_line_face_text_property_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-fmlf*")
  (fundamental-mode)
  (insert "x")
  (let* ((s (format-mode-line '(:propertize "TEXT" face italic)))
         (props (text-properties-at 0 s)))
    (kill-buffer (current-buffer))
    (list (stringp s)
          (plist-get props 'face)
          (plist-get props 'font-lock-face))))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
