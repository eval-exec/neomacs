//! Strict combo oracle probes, batch 207: bidirectional text direction.
//! bidi-paragraph-direction over LTR (English) and RTL (Hebrew/Arabic) text,
//! explicit bidi-paragraph-separate, and bidi-resolved-level probes.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_bidi_paragraph_direction_ltr_rtl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "English text here")
        (bidi-paragraph-direction))
      (with-temp-buffer
        (insert "שלום עולם טקסט")
        (bidi-paragraph-direction))
      (with-temp-buffer
        (insert "مرحبا بالعالم")
        (bidi-paragraph-direction))
      (with-temp-buffer
        (insert "123 456")
        (bidi-paragraph-direction))
      (with-temp-buffer
        (insert "")
        (bidi-paragraph-direction))
      (with-temp-buffer
        (insert "Mixed שלום text")
        (bidi-paragraph-direction)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function bidi-paragraph-direction)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_bidi_resolved_level_and_mirror() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (bidi-resolved-level "abc" 0)
      (bidi-resolved-level "abc" 1)
      (bidi-resolved-level "(a)" 0)
      (bidi-resolved-level "שלום" 0)
      (with-temp-buffer
        (insert "Hello (world)")
        (bidi-paragraph-direction)
        (buffer-substring 1 13)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function bidi-resolved-level)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_bidi_paragraph_explicit_direction_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "English")
        (setq bidi-paragraph-direction 'right-to-left)
        (bidi-paragraph-direction))
      (with-temp-buffer
        (insert "שלום")
        (setq bidi-paragraph-direction 'left-to-right)
        (bidi-paragraph-direction))
      (eq 'left-to-right 'left-to-right)
      (eq 'right-to-left 'right-to-left))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function bidi-paragraph-direction)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
