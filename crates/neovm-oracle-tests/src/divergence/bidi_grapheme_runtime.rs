//! Bidi + grapheme/normalization parity: bidi-string-mark-left-to-right,
//! Hangul NFD/NFC (algorithmic jamo), reverse/string-reverse, char mirroring,
//! bidi-class properties, compose-region, Arabic normalization; plus the
//! string-glyph-split grapheme-cluster and bidi-paragraph-direction divergences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn bd_bidi_class_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (L AL EN WS)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?A 'bidi-class)
        (get-char-code-property ?ا 'bidi-class)
        (get-char-code-property ?1 'bidi-class)
        (get-char-code-property ?\s 'bidi-class))"##,
        expect,
    );
}

#[test]
fn bd_bidi_mark_ltr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "abcשלום"))
  (list (stringp (bidi-string-mark-left-to-right s))
        (>= (length (bidi-string-mark-left-to-right s)) (length s))))"##,
        expect,
    );
}

#[test]
fn divergence_bidi_paragraph_direction_rtl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (left-to-right right-to-left)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (list (current-bidi-paragraph-direction)
        (progn (erase-buffer) (insert "שלום") (current-bidi-paragraph-direction))))"##,
        expect,
    );
}

#[test]
fn bd_char_mirror() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (41 93 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?\( 'mirroring)
        (get-char-code-property ?\[ 'mirroring)
        (get-char-code-property ?a 'mirroring))"##,
        expect,
    );
}

#[test]
fn bd_compose_region_auto() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 101 769)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert (string ?e #x0301))
  (list (buffer-size) (char-after 1) (char-after 2)))"##,
        expect,
    );
}

#[test]
fn divergence_string_glyph_split_grapheme() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 3 (\"a\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (length (string-glyph-split (string ?e #x0301)))
        (length (string-glyph-split "abc"))
        (string-glyph-split "a"))"##,
        expect,
    );
}

#[test]
fn bd_hangul_nfd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ucs-normalize)
(let ((s "한"))
  (list (length s) (length (ucs-normalize-NFD-string s))
        (length (ucs-normalize-NFC-string (ucs-normalize-NFD-string s)))))"##,
        expect,
    );
}

#[test]
fn bd_nfc_hangul_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ucs-normalize)
(let ((jamo (string #x1112 #x1161 #x11ab)))
  (list (length jamo) (length (ucs-normalize-NFC-string jamo))
        (string= (ucs-normalize-NFC-string jamo) "한")))"##,
        expect,
    );
}

#[test]
fn bd_normalize_arabic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ucs-normalize)
(let ((s "السلام"))
  (list (string= (ucs-normalize-NFC-string s) s)
        (stringp (ucs-normalize-NFD-string s))))"##,
        expect,
    );
}

#[test]
fn bd_string_reverse_bidi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function string-reverse)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (reverse "abc") (reverse "héllo")
        (string-reverse "abcd") (reverse [1 2 3]))"##,
        expect,
    );
}
