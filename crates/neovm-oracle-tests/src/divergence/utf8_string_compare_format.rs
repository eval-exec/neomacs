//! UTF-8 / multibyte *string comparison, width, and formatting* divergence probes.
//!
//! Probes `string-width` / `format` field-width padding (which in GNU pad by
//! *display width*, not char count), `string<` / `compare-strings` over
//! multibyte, equality between decode-recovered and constructed eight-bit
//! chars, and `replace-regexp-in-string` over multibyte regex classes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- string-width (display column width) ------------------------------------

#[test]
fn div_utf8_string_width_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 4 0 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (string-width "café")
      (string-width "世界")
      (string-width "a😀b")
      (string-width "")
      (string-width "héllo"))
"#,
        expect,
    );
}

#[test]
fn div_utf8_string_width_combining_and_wide_mix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (string-width "é")     ; e + combining acute
      (string-width "äb")
      (string-width "ＡＢＣ"))        ; fullwidth latin
"#,
        expect,
    );
}

// --- format field-width padding (by display width) --------------------------

#[test]
fn div_utf8_format_width_padding_ascii_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"       abc\" 10 \"abc       |\" \"    A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (format "%10s" "abc")
      (length (format "%10s" "abc"))
      (format "%-10s|" "abc")
      (format "%5c" 65))
"#,
        expect,
    );
}

#[test]
fn div_utf8_format_width_padding_multibyte_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"      世界\" 8 \"café    |\" 9)""#]];
    // Pads by display width: "世界" is width 4, so "%-10s" adds 6 spaces.
    crate::common::assert_oracle_parity_expect(
        r#"
(list (format "%10s" "世界")
      (length (format "%10s" "世界"))
      (format "%-8s|" "café")
      (length (format "%-8s|" "café")))
"#,
        expect,
    );
}

#[test]
fn div_utf8_format_width_padding_cjk_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 6 \"  あ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (length (format "%-10s|" "中"))
      (string-width (format "%-6s" "中"))
      (format "%4c" #x3042))
"#,
        expect,
    );
}

// --- string comparison over multibyte ---------------------------------------

#[test]
fn div_utf8_string_lessp_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (string< "abc" "abd")
      (string< "café" "cafz")
      (string-lessp "世界" "你好")
      (string< "aéb" "aéb")
      (string> "z" "é"))
"#,
        expect,
    );
}

#[test]
fn div_utf8_compare_strings_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t -3 -4 -1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (compare-strings "café" nil nil "café" nil nil)
      (compare-strings "abc" nil nil "abd" nil nil)
      (compare-strings "cafz" nil nil "café" nil nil)
      (compare-strings "café" 1 3 "ca" nil nil))
"#,
        expect,
    );
}

#[test]
fn div_utf8_eightbit_recovered_vs_constructed_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    // Does string= treat decode-recovered and constructed eight-bit chars as
    // equal? (They share a codepoint but, in Neomacs, differ in byte width.)
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 200) 'utf-8))
      (m (string-make-multibyte (unibyte-string 200))))
  (list (string= d m) (equal d m) (compare-strings d nil nil m nil nil)))
"#,
        expect,
    );
}

// --- replace over multibyte regex -------------------------------------------

#[test]
fn div_utf8_replace_regexp_in_string_ascii_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"XéX\" \"XXXXX\" \"XXX\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (replace-regexp-in-string "[a-z]" "X" "aéb")
      (replace-regexp-in-string "[[:alpha:]]" "X" "héllo")
      (replace-regexp-in-string "\\w" "X" "aéb"))
"#,
        expect,
    );
}

#[test]
fn div_utf8_replace_regexp_in_string_multibyte_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"cafÉ rÉsumÉ\" \"你好WORLD\" \"cafE Elite\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (replace-regexp-in-string "é" "É" "café résumé")
      (replace-regexp-in-string "世界" "WORLD" "你好世界")
      (replace-regexp-in-string "[éè]" "E" "café èlite"))
"#,
        expect,
    );
}

#[test]
fn div_utf8_regexp_opt_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\\\(?:café\\\\|thé\\\\|世界\\\\)\" 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (regexp-opt '("café" "thé" "世界"))
      (length (regexp-opt '("café" "thé"))))
"#,
        expect,
    );
}
