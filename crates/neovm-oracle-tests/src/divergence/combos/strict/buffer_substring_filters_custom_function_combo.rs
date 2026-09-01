//! Strict combo oracle probes, batch 375: buffer-substring-filters custom.
//! Custom filter-buffer-substring-functions that transform output,
//! format-write onto substring, and filter-buffer-substring with NO-PROPS.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_filter_buffer_substring_custom_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "Hello World")
  (add-text-properties 1 6 '(face bold))
  (let ((saved filter-buffer-substring-functions))
    (unwind-protect
        (progn
          (setq filter-buffer-substring-functions
                (list (lambda (beg end delete)
                        (upcase (buffer-substring beg end)))))
          (list (filter-buffer-substring 1 12 nil)
                (filter-buffer-substring 1 12 'sans)))
      (setq filter-buffer-substring-functions saved))))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (beg end delete) (upcase (buffer-substring beg end))) 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_substring_filters_default_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "visible HIDDEN visible")
  (add-text-properties 8 14 '(invisible t))
  (list (buffer-substring 1 22)
        (filter-buffer-substring 1 22 nil)
        (filter-buffer-substring 1 22 t)
        (buffer-substring-no-properties 1 22)))
"##;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 22)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_propertize_face_extract_from_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s1 (format-propertize "hello" 'face 'bold))
      (s2 (propertize "world" 'face 'italic 'font-lock-face 'underline)))
  (list (plist-get (text-properties-at 0 s1) 'face)
        (plist-get (text-properties-at 0 s2) 'face)
        (plist-get (text-properties-at 0 s2) 'font-lock-face)
        (length s1)
        (length s2)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-propertize)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
