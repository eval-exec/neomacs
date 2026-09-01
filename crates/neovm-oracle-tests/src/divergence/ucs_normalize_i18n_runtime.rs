//! Unicode normalization + i18n parity: ucs-normalize NFC composition and
//! idempotency, japanese-katakana/hiragana/zenkaku/hankaku, char-fold-to-regexp
//! search, china-util presence, compose-chars/compose-string, string-glyph-split;
//! plus the NFD canonical and NFKD compatibility decomposition divergences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn i18_char_fold_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'char-fold)
(with-temp-buffer
  (insert "the cafe and café here")
  (goto-char (point-min))
  (let ((case-fold-search t))
    (list (numberp (re-search-forward (char-fold-to-regexp "cafe") nil t)))))"##,
        expect,
    );
}

#[test]
fn i18_chinese_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'china-util)
  (list (functionp 'decode-hz-region) (functionp 'encode-hz-region))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn i18_compose_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument characterp #(\"x\" 0 1 (composition ((1)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (stringp (compose-chars ?a ?b))
        (char-to-string (compose-chars ?x))
        (length (compose-string "test" 0 2)))"##,
        expect,
    );
}

#[test]
fn i18_japanese_kana() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"アイウ\" \"あいう\" \"ａｂｃ\" \"ABC\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'japan-util)
  (list (japanese-katakana "あいう") (japanese-hiragana "アイウ")
        (japanese-zenkaku "abc") (japanese-hankaku "ＡＢＣ"))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn i18_japanese_kana_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12450 12354)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'japan-util)
  (list (japanese-katakana ?あ) (japanese-hiragana ?ア))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn i18_string_glyph_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"a\" \"b\" \"c\") (\"a\" \"b\") 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-glyph-split "abc") (string-glyph-split "ab") (length (string-glyph-split "xyz")))"##,
        expect,
    );
}

#[test]
fn i18_ucs_normalize_combining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ucs-normalize)
(let ((decomposed (string ?e #x0301)))
  (list (length decomposed)
        (length (ucs-normalize-NFC-string decomposed))
        (string= (ucs-normalize-NFC-string decomposed) "é")))"##,
        expect,
    );
}

#[test]
fn i18_ucs_normalize_idempotent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ucs-normalize)
(let ((s "Ωμέγα"))
  (list (string= (ucs-normalize-NFC-string (ucs-normalize-NFC-string s))
                 (ucs-normalize-NFC-string s))
        (string= (ucs-normalize-NFD-string (ucs-normalize-NFD-string s))
                 (ucs-normalize-NFD-string s))))"##,
        expect,
    );
}

#[test]
fn divergence_ucs_normalize_nfd_canonical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 6 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ucs-normalize)
(list (length (ucs-normalize-NFD-string "é"))
      (length (ucs-normalize-NFD-string "àñü"))
      (string= (ucs-normalize-NFD-string "é") (string ?e #x0301)))"##,
        expect,
    );
}

#[test]
fn divergence_ucs_normalize_nfkd_compat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"2\" \"XII\" \"kg\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ucs-normalize)
(list (ucs-normalize-NFKD-string "²")
      (ucs-normalize-NFKC-string "Ⅻ")
      (ucs-normalize-NFKC-string "㎏"))"##,
        expect,
    );
}
