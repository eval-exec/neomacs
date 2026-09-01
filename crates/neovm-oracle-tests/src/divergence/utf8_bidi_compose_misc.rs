//! UTF-8 / multibyte *bidi, composition & misc* divergence probes.
//!
//! Probes the Unicode bidi-mirroring table (`bidi-mirror-char`), `buffer-hash`
//! over multibyte text, multibyte symbol names (`intern`/`symbol-name`),
//! `translate-region` with a char-table, `find-composition`, and text-property
//! run computation over multibyte regions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- bidi-mirror-char (Unicode Bidi_Mirroring table) ------------------------

#[test]
fn div_utf8_bidi_mirror_char_brackets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function bidi-mirror-char)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (bidi-mirror-char ?\()
      (bidi-mirror-char ?\))
      (bidi-mirror-char ?<)
      (bidi-mirror-char ?>)
      (bidi-mirror-char ?\[)
      (bidi-mirror-char ?\])
      (bidi-mirror-char ?\x3008)   ; ⟨
      (bidi-mirror-char ?\x3009)   ; ⟩
      (bidi-mirror-char ?\x2208)   ; ∈
      (bidi-mirror-char ?a))       ; non-mirroring -> nil
"#,
        expect,
    );
}

// --- buffer-hash over multibyte ---------------------------------------------

#[test]
fn div_utf8_buffer_hash_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"fd5a5c98b79d0a4eeebce6933dd52d4f6400611f\" \"cc67a2c36577b3097371f0b0e6adcef2f2c1ce1a\" \"4b6970254867f699ce70221cd476b2dcab220f3e\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (with-temp-buffer (insert "café世界") (buffer-hash))
      (with-temp-buffer (insert (decode-coding-string (unibyte-string 200) 'utf-8))
        (buffer-hash))
      (with-temp-buffer (insert "aéb") (buffer-hash)))
"#,
        expect,
    );
}

// --- multibyte symbol names -------------------------------------------------

#[test]
fn div_utf8_intern_multibyte_symbol_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" \"世界\" t t café 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s1 (intern "café"))
      (s2 (intern "世界")))
  (list (symbol-name s1)
        (symbol-name s2)
        (eq s1 (intern "café"))
        (eq s2 (intern "世界"))
        (intern-soft "café")
        (length (symbol-name s2))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_intern_multibyte_symbol_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((ob (obarray)))
  (let ((a (intern "λ-table" ob))
        (b (intern "λ-table" ob)))
    (list (eq a b) (symbol-name a)
          (eq (intern-soft "λ-table" ob) a))))
"#,
        expect,
    );
}

// --- translate-region with a char-table -------------------------------------

#[test]
fn div_utf8_translate_region_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"cAfÉい\" 6 (99 65 102 201 12356))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((ct (make-char-table 'translation-table)))
  (aset ct ?a ?A)
  (aset ct ?é ?É)
  (aset ct ?\x3042 ?\x3044)
  (with-temp-buffer
    (insert "caféあ")
    (translate-region (point-min) (point-max) ct)
    (list (buffer-string) (point-max) (append (buffer-string) nil))))
"#,
        expect,
    );
}

// --- find-composition -------------------------------------------------------

#[test]
fn div_utf8_find_composition_explicit_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3 [] t nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(condition-case err
    (with-temp-buffer
      (insert "abc")
      (compose-region 1 3 "")
      (find-composition 1 nil nil t))
  (error (cons (car err) 'errored)))
"#,
        expect,
    );
}

// --- text-property runs over multibyte --------------------------------------

#[test]
fn div_utf8_text_property_runs_over_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 4 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café世界x")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 6 'face 'italic)
  (list (next-single-property-change 1 'face)
        (next-single-property-change 3 'face)
        (text-property-any 1 8 'face 'italic)
        (next-property-change 1)))
"#,
        expect,
    );
}

// --- emoji ZWJ sequence accounting ------------------------------------------

#[test]
fn div_utf8_emoji_zwj_sequence_accounting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 18 6 (128104 8205 128105 8205 128103))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "👨‍👩‍👧")
  (list (length (buffer-string))
        (string-bytes (buffer-string))
        (point-max)
        (append (buffer-string) nil)))
"#,
        expect,
    );
}
