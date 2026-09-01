//! Strict combo oracle probes, batch 206: mode-line formatting. format-mode-line
//! over %b/%l/%p/%m constructs, a :propertize form, an :eval form, the default
//! mode-line-format, and line-number/position percent in a known buffer.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_format_mode_line_percent_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-ml*")
  (fundamental-mode)
  (insert "line1\nline2\nline3\nline4\nline5")
  (goto-char (point-min))
  (forward-line 2)
  (let ((result (list (format-mode-line "%b")
                      (format-mode-line "%m")
                      (format-mode-line "%l")
                      (format-mode-line "%p")
                      (format-mode-line "%I")
                      (format-mode-line "%%")
                      (format-mode-line "%["))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_mode_line_propertize_eval_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-ml2*")
  (fundamental-mode)
  (insert "hello")
  (let ((result (list (format-mode-line '(:propertize "X" face bold))
                      (format-mode-line '(:eval (number-to-string (point))))
                      (format-mode-line '("[" mode-name "]"))
                      (format-mode-line "%b" nil (current-buffer))
                      (length (format-mode-line mode-line-format))
                      (> (length (format-mode-line mode-line-format)) 0))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument windowp #<buffer  *probe-ml2*>)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_header_line_tab_line_format_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-hl*")
  (fundamental-mode)
  (insert "x")
  (let ((result (list (format-mode-line header-line-format)
                      (format-mode-line tab-line-format)
                      (stringp (format-mode-line "%b"))
                      (format-mode-line '(:eval (buffer-name)))
                      (format-mode-line "%n"))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""OK (\"\" \"\" t \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
