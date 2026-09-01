//! Strict combo oracle probes, batch 308: temp-buffer-window + output-stream.
//! with-temp-buffer-window, with-output-to-string, with-current-buffer-window,
//! and standard-output / print-to-buffer.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_with_temp_buffer_window_output_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((buf-name "*probe-tbw*"))
  (when (get-buffer buf-name) (kill-buffer buf-name))
  (let ((result (with-temp-buffer-window buf-name nil nil
                  (insert "window-content")
                  (current-buffer))))
    (prog1
        (list (bufferp result)
              (buffer-name result)
              (with-output-to-string
                (princ "hello")
                (princ " ")
                (princ "world")))
      (when (get-buffer buf-name) (kill-buffer buf-name)))))
"##;
    let expect = expect_test::expect![[r#""OK (t \"*scratch*\" \"hello world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_with_current_buffer_window_print_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((buf-name "*probe-print*"))
  (when (get-buffer buf-name) (kill-buffer buf-name))
  (let ((result (with-current-buffer-window buf-name nil nil
                  (insert "printed"))))
    (prog1
        (list (bufferp result)
              (with-current-buffer buf-name (buffer-string))
              (let ((standard-output (get-buffer-create " *probe-stdout*")))
                (print '(a b c) standard-output)
                (prog1 (with-current-buffer " *probe-stdout*" (buffer-string))
                  (kill-buffer " *probe-stdout*"))))
      (when (get-buffer buf-name) (kill-buffer buf-name)))))
"##;
    let expect = expect_test::expect![[r#""OK (nil \"printed\" \"\\n(a b c)\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_temp_buffer_show_hook_standard_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (stringp (with-output-to-string (princ "x")))
      (let ((standard-output nil))
        (with-output-to-string (princ "isolated")))
      (with-temp-message "temp-msg"
        (current-message))
      (stringp (format "%s" (with-output-to-string (princ "nested")))))
"##;
    let expect = expect_test::expect![[r#""OK (t \"isolated\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
