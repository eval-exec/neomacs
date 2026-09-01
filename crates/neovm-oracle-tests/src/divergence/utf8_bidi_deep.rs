//! UTF-8 / multibyte *bidi paragraph direction deep* divergence probes.
//!
//! Follows the RTL paragraph-direction bug (#35): probes direction detection
//! across scripts (Arabic, CJK, digits, empty), whether the
//! `bidi-paragraph-direction` user variable is honored, mixed-direction buffer
//! substring, and char-after in an RTL region.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_bidi_direction_across_scripts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (right-to-left left-to-right left-to-right left-to-right right-to-left)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (with-temp-buffer (insert "مرحبا") (current-bidi-paragraph-direction))
      (with-temp-buffer (insert "你好") (current-bidi-paragraph-direction))
      (with-temp-buffer (insert "") (current-bidi-paragraph-direction))
      (with-temp-buffer (insert "123") (current-bidi-paragraph-direction))
      (with-temp-buffer (insert "السلام") (current-bidi-paragraph-direction)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_bidi_paragraph_direction_variable_honored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (left-to-right left-to-right)""#]];
    // Forcing the paragraph direction via the variable.
    crate::common::assert_oracle_parity_expect(
        r#"
(list (let ((bidi-paragraph-direction 'right-to-left))
        (with-temp-buffer (insert "abc") (current-bidi-paragraph-direction)))
      (let ((bidi-paragraph-direction 'left-to-right))
        (with-temp-buffer (insert "abc") (current-bidi-paragraph-direction))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_bidi_mixed_direction_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"abcאבגdef\" (97 98 99 1488 1489 1490 100 101 102) 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "abcאבגdef")
  (list (buffer-substring 1 (point-max))
        (append (buffer-substring 1 (point-max)) nil)
        (point-max)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_after_in_rtl_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1513 1500 1493 1501)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "שלום")
  (list (char-after 1) (char-after 2) (char-after 3) (char-after 4)))
"#,
        expect,
    );
}

// --- sort-lines / upcase-region over multibyte (coverage) -------------------

#[test]
fn div_utf8_sort_lines_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"apple\\ncafé\\nzoo\\n世界\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café\napple\nzoo\n世界\n")
  (sort-lines nil (point-min) (point-max))
  (buffer-string))
"#,
        expect,
    );
}

#[test]
fn div_utf8_upcase_region_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"CAFÉ STRASSE\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café straße")
  (upcase-region (point-min) (point-max))
  (buffer-string))
"#,
        expect,
    );
}
