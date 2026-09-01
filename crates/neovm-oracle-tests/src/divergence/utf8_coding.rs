//! UTF-8 / multibyte *coding* divergence probes (parity vs GNU `src/coding.c`).
//!
//! Neomacs advertises a UTF-8 internal string representation, whereas GNU Emacs
//! uses its own multibyte encoding with dedicated eight-bit "raw-byte"
//! characters (codepoints up to `MAX_CHAR` = `#x3FFFFF`).  These probes target
//! the encode/decode layer, raw-byte handling, BOM, invalid-byte recovery and
//! char/byte accounting — all areas where the two internal models are most
//! likely to diverge.
//!
//! Each `#[test]` runs the same form in GNU Emacs and the Neomacs release
//! binary via `assert_oracle_parity`; a panic means a real divergence.  Tests
//! that *pass* are kept as parity coverage / regression guards.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- raw bytes vs multibyte encoding ----------------------------------------

#[test]
fn div_utf8_raw_byte_multibyte_byte_accounting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((u (unibyte-string 200 201 202))
       (m (string-make-multibyte u)))
  (list (unibyte-string-p u)
        (multibyte-string-p u)
        (multibyte-string-p m)
        (length u) (string-bytes u)
        (length m) (string-bytes m)
        (append m nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_raw_byte_codepoints_after_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4194176 4194248 4194303)""#]];
    // After string-make-multibyte, each raw byte 128..255 becomes a distinct
    // non-ASCII character.  Its exact codepoint is implementation-defined and
    // a prime UTF-8-internal divergence point.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((m (string-make-multibyte (unibyte-string 128 200 255))))
  (append m nil))
"#,
        expect,
    );
}

#[test]
fn div_utf8_unibyte_string_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((u (unibyte-string 0 1 127 128 200 255)))
  (list (length u) (string-bytes u) (multibyte-string-p u)
        (unibyte-string-p u) (append u nil)))
"#,
        expect,
    );
}

// --- unibyte <-> multibyte character conversion ----------------------------

#[test]
fn div_utf8_unibyte_multibyte_char_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65 4194248 200 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (unibyte-char-to-multibyte 65)
      (unibyte-char-to-multibyte 200)
      (multibyte-char-to-unibyte (unibyte-char-to-multibyte 200))
      (multibyte-char-to-unibyte 233))
"#,
        expect,
    );
}

#[test]
fn div_utf8_unibyte_char_to_multibyte_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((error . \"Not a unibyte character: 256\") (wrong-type-argument . \"Wrong type argument: characterp, -1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (condition-case err (unibyte-char-to-multibyte 256) (error (cons (car err) (error-message-string err))))
      (condition-case err (unibyte-char-to-multibyte -1)  (error (cons (car err) (error-message-string err)))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_multibyte_char_to_unibyte_non_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (-1 -1 255 0)""#]];
    // Characters > 255 cannot map to a single byte; GNU errors.
    crate::common::assert_oracle_parity_expect(
        r#"
(list (condition-case err (multibyte-char-to-unibyte #x100)   (error (car err)))
      (condition-case err (multibyte-char-to-unibyte #x3042)  (error (car err)))
      (multibyte-char-to-unibyte 255)
      (multibyte-char-to-unibyte 0))
"#,
        expect,
    );
}

// --- string-as-unibyte / string-as-multibyte (byte reinterpretation) -------

#[test]
fn div_utf8_string_as_unibyte_reinterpretation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((4194243 4194217) (195 169) 2 2)""#]];
    // string-as-unibyte reinterprets the *internal byte sequence* of a
    // multibyte string as raw bytes.  Diverges sharply under UTF-8-internal.
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((m (string-make-multibyte (unibyte-string 195 169)))
       (u (string-as-unibyte m)))
  (list (append m nil) (append u nil)
        (length u) (string-bytes u)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_string_as_multibyte_reinterpretation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((195 169) (233) 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((u (unibyte-string 195 169))
       (m (string-as-multibyte u)))
  (list (append u nil) (append m nil)
        (length m) (string-bytes m)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_string_to_unibyte_vs_make_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error (0 66))""#]];
    // string-to-unibyte errors on chars >255; string-make-unibyte truncates.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s (string #x100 #x3042)))
  (list (condition-case err (string-to-unibyte s) (error (car err)))
        (append (string-make-unibyte s) nil)))
"#,
        expect,
    );
}

// --- encode/decode round trips ---------------------------------------------

#[test]
fn div_utf8_encode_decode_latin1_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 5 4 4 4 5 t (99 97 102 233))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s "café")
       (e (encode-coding-string s 'latin-1))
       (d (decode-coding-string e 'latin-1)))
  (list (length s) (string-bytes s)
        (length e) (string-bytes e)
        (length d) (string-bytes d)
        (equal s d)
        (append d nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decode_invalid_utf8_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 8 (4194303 4194302 4194301 4194176))""#]];
    // Lone continuation/invalid bytes: GNU recovers each as a raw-byte char.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 255 254 253 128) 'utf-8)))
  (list (length d) (string-bytes d) (append d nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decode_truncated_utf8_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 (4194243 65 66))""#]];
    // Two-byte lead (0xC3) with no continuation is invalid.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 195 65 66) 'utf-8)))
  (list (length d) (string-bytes d) (append d nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_encode_utf8_with_signature_bom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6 nil (239 187 191 97 98 99))""#]];
    // utf-8-with-signature prepends the BOM (EF BB BF).
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((b (encode-coding-string "abc" 'utf-8-with-signature)))
  (list (length b) (string-bytes b) (multibyte-string-p b) (append b nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decode_strips_bom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 6 (65279 97 98 99))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 239 187 191 97 98 99) 'utf-8)))
  (list (length d) (string-bytes d) (append d nil)))
"#,
        expect,
    );
}

// --- char-bytes / encode-char / decode-char ---------------------------------

#[test]
fn div_utf8_char_bytes_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-bytes)""#]];
    // char-bytes reflects the *internal* multibyte width, not UTF-8 width.
    // This is a canonical UTF-8-internal divergence probe.
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar #'char-bytes
        (list ?a ?A ?1
              ?é ?\x100 ?\x250 ?\x3042 ?\x4e2d
              ?\x1f600 ?\x10000))
"#,
        expect,
    );
}

#[test]
fn div_utf8_encode_char_decode_char_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument charsetp utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((e1 (encode-char ?é 'utf-8))
      (e2 (encode-char #x3042 'utf-8))
      (e3 (encode-char #x1f600 'utf-8)))
  (list e1 e2 e3
        (decode-char 'utf-8 e1)
        (decode-char 'utf-8 e2)
        (decode-char 'utf-8 e3)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_max_char_and_char_valid_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable max-char)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list max-char
      (char-valid-p max-char)
      (char-valid-p #x3FFFFF)
      (char-valid-p #x110000)
      (char-valid-p #x10FFFF)
      (char-valid-p 0)
      (characterp #x3FFFFF)
      (characterp #x400000))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decode_coding_string_utf8_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 5 (233 8364))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 195 169 226 130 172) 'utf-8)))
  (list (length d) (string-bytes d) (append d nil)))
"#,
        expect,
    );
}
