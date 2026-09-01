//! Strict combo oracle probes, batch 373: print/prin1 with custom
//! standard-output. let-bind standard-output to a buffer, print/prin1/princ
//! to it, and with-output-to-string with nested output.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_print_prin1_to_custom_standard_output_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-stdout*")
  (erase-buffer)
  (let ((standard-output (current-buffer)))
    (print '(a b c))
    (prin1 42)
    (princ "literal")
    (let ((result (buffer-string)))
      (kill-buffer (current-buffer))
      result)))
"##;
    let expect = expect_test::expect![[r#""OK \"\\n(a b c)\\n42literal\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_with_output_to_string_nested_terpri() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-output-to-string
        (princ "outer ")
        (let ((standard-output standard-output))
          (princ "inner ")
          (terpri)
          (princ "after-terpri"))
        (princ "end"))
      (length (with-output-to-string
                (princ "x")
                (terpri)
                (princ "y")))
      (with-output-to-string
        (princ (format "%d + %d = %d" 1 2 3))))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"outer inner \\nafter-terpriend\" 3 \"1 + 2 = 3\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_prin1_to_string_vs_princ_to_string_face_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s (propertize "styled" 'face 'bold 'font-lock-face 'italic)))
  (list (prin1-to-string s)
        (princ-to-string s)
        (let ((print-escape-multibyte t))
          (prin1-to-string "café"))
        (let ((print-escape-newlines t))
          (prin1-to-string "a\nb"))
        (let ((print-level 1))
          (prin1-to-string '(1 (2 (3 (4))))))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function princ-to-string)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
