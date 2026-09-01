//! UTF-8 / multibyte *coding* deep probes — BOM/signature, UTF-16, eight-bit
//! byte width, and charset classification.
//!
//! Follow-up to `divergence_utf8_coding.rs`, expanding the three confirmed
//! divergence themes: (a) `-with-signature` BOM handling, (b) internal byte
//! width of eight-bit raw-byte characters, (c) eight-bit charset
//! classification.  Also probes UTF-16 endianness/BOM which is structurally
//! similar and a likely additional divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- coding-system existence / aliasing -------------------------------------

#[test]
fn div_utf8_coding_system_p_signature_and_utf16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (coding-system-p 'utf-8)
      (coding-system-p 'utf-8-with-signature)
      (coding-system-p 'utf-8-with-signature-unix)
      (coding-system-p 'utf-16)
      (coding-system-p 'utf-16le)
      (coding-system-p 'utf-16be)
      (coding-system-p 'utf-16le-with-signature))
"#,
        expect,
    );
}

// --- BOM / signature on encode ----------------------------------------------

#[test]
fn div_utf8_encode_signature_byte_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 6 (239 187 191 97 98 99) 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (length (encode-coding-string "abc" 'utf-8))
      (length (encode-coding-string "abc" 'utf-8-with-signature))
      (append (encode-coding-string "abc" 'utf-8-with-signature) nil)
      (string-bytes (encode-coding-string "abc" 'utf-8-with-signature)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_encode_signature_multibyte_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 8 (239 187 191 99 97 102 195 169))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((e (encode-coding-string "café" 'utf-8-with-signature)))
  (list (length e) (string-bytes e) (append e nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decode_signature_strips_bom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 (97 98 99))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 239 187 191 97 98 99) 'utf-8-with-signature)))
  (list (length d) (string-bytes d) (append d nil)))
"#,
        expect,
    );
}

// --- UTF-16 endianness / BOM ------------------------------------------------

#[test]
fn div_utf8_utf16_be_with_bom_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6 (254 255 0 65 0 66))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((e (encode-coding-string "AB" 'utf-16)))
  (list (length e) (string-bytes e) (append e nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_utf16_le_no_bom_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 (65 0 66 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((e (encode-coding-string "AB" 'utf-16le)))
  (list (length e) (string-bytes e) (append e nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_utf16_be_no_bom_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 (0 65 0 66))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((e (encode-coding-string "AB" 'utf-16be)))
  (list (length e) (string-bytes e) (append e nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_utf16_decode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AB\" 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((bytes (unibyte-string 254 255 0 65 0 66)))
  (list (decode-coding-string bytes 'utf-16)
        (length (decode-coding-string bytes 'utf-16))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_utf16_encode_supplementary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 (254 255 216 61 222 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((e (encode-coding-string "😀" 'utf-16)))
  (list (length e) (append e nil)))
"#,
        expect,
    );
}

// --- eight-bit raw-byte width -----------------------------------------------

#[test]
fn div_utf8_eightbit_char_bytes_per_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-bytes)""#]];
    // Per-char byte cost of eight-bit characters — GNU reports 2.
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-bytes (unibyte-char-to-multibyte 128))
      (char-bytes (unibyte-char-to-multibyte 160))
      (char-bytes (unibyte-char-to-multibyte 200))
      (char-bytes (unibyte-char-to-multibyte 255)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_eightbit_string_bytes_byte_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 8 (4194176 4194177 4194248 4194303))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((m (string-make-multibyte (unibyte-string 128 129 200 255))))
  (list (length m) (string-bytes m) (append m nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_eightbit_mixed_string_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 5 (97 4194248 233))""#]];
    // ASCII + eight-bit + multibyte (é) in one string.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s (concat "a"
                 (string-make-multibyte (unibyte-string 200))
                 "é")))
  (list (length s) (string-bytes s) (append s nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_bytes_table_with_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-bytes)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar #'char-bytes
        (list ?a ?é ?\x3042
              (unibyte-char-to-multibyte 200)
              (unibyte-char-to-multibyte 255)
              #x3FFFFF))
"#,
        expect,
    );
}

// --- eight-bit charset classification ---------------------------------------

#[test]
fn div_utf8_char_charset_eightbit_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (eight-bit eight-bit eight-bit eight-bit eight-bit)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar (lambda (b) (char-charset (unibyte-char-to-multibyte b)))
        (list 128 160 200 254 255))
"#,
        expect,
    );
}

#[test]
fn div_utf8_encode_decode_char_eightbit_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (200 4194248 eight-bit)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((c (unibyte-char-to-multibyte 200)))
  (list (encode-char c 'eight-bit)
        (decode-char 'eight-bit 200)
        (char-charset (decode-char 'eight-bit 200))))
"#,
        expect,
    );
}

// --- charset text properties on decode --------------------------------------

#[test]
fn div_utf8_decode_coding_string_charset_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"café\" 0 4 (charset iso-8859-1)) (charset iso-8859-1) 4)""#
    ]];
    // Does in-memory latin-1 decode (not file I/O) also attach a charset prop?
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 99 97 102 233) 'latin-1)))
  (list d (text-properties-at 0 d) (length d)))
"#,
        expect,
    );
}
