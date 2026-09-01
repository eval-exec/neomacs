//! UTF-8 / multibyte *print-escape & byte serialization* divergence probes.
//!
//! Probes `print-escape-nonascii` / `print-escape-multibyte` (which emit the
//! internal byte representation as octal escapes), `encode-hex-string`, and
//! `set-buffer-multibyte` toggling.  All depend on the internal byte layout,
//! so eight-bit chars (3 vs 2 byte width) are the likely divergence vector.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- print-escape-nonascii --------------------------------------------------

#[test]
fn div_utf8_print_escape_nonascii_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\\"café\\\"\" \"\\\"世界\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((print-escape-nonascii t))
  (list (prin1-to-string "café")
        (prin1-to-string "世界")))
"#,
        expect,
    );
}

#[test]
fn div_utf8_print_escape_nonascii_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\\"\\\\310\\\"\" \"\\\"\\\\310\\\"\")""#]];
    // Escaped octal of an eight-bit char exposes the 2-vs-3 byte divergence.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((print-escape-nonascii t))
  (list (prin1-to-string (decode-coding-string (unibyte-string 200) 'utf-8))
        (prin1-to-string (string-make-multibyte (unibyte-string 200)))))
"#,
        expect,
    );
}

// --- print-escape-multibyte -------------------------------------------------

#[test]
fn div_utf8_print_escape_multibyte_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"caf\\\\x00e9\\\"\" \"\\\"\\\\x4e16\\\\x754c\\\"\" \"\\\"\\\\310\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((print-escape-multibyte t))
  (list (prin1-to-string "café")
        (prin1-to-string "世界")
        (prin1-to-string (string-make-multibyte (unibyte-string 200)))))
"#,
        expect,
    );
}

// --- encode-hex-string ------------------------------------------------------

#[test]
fn div_utf8_encode_hex_string_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function encode-hex-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (encode-hex-string "abc")
      (encode-hex-string "café")
      (encode-hex-string "世界"))
"#,
        expect,
    );
}

#[test]
fn div_utf8_encode_hex_string_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function encode-hex-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (encode-hex-string (decode-coding-string (unibyte-string 200 255) 'utf-8))
      (encode-hex-string (string-make-multibyte (unibyte-string 200 255))))
"#,
        expect,
    );
}

// --- set-buffer-multibyte toggling ------------------------------------------

#[test]
fn div_utf8_set_buffer_multibyte_toggle_with_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" 5 nil (99 97 102 195 169))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café")
  (let ((multibyte-before (buffer-string)))
    (set-buffer-multibyte nil)
    (list (buffer-string)
          (length (buffer-string))
          (multibyte-string-p (buffer-string))
          (append (buffer-string) nil))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_set_buffer_multibyte_toggle_with_raw_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (4194248 4194249 65) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65))
  (list (length (buffer-string)) (append (buffer-string) nil))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (append (buffer-string) nil)
        (multibyte-string-p (buffer-string))))
"#,
        expect,
    );
}

// --- prin1 round-trip stability ---------------------------------------------

#[test]
fn div_utf8_prin1_roundtrip_eightbit_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"\\\"\\\\310\\\\311\\\\377\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s (string-make-multibyte (unibyte-string 200 201 255)))
       (p (prin1-to-string s))
       (back (car (read-from-string p))))
  (list (equal s back) p))
"#,
        expect,
    );
}
