//! Strict combo oracle probes, batch 83: micro-tail — decode/encode-coding-char
//! (char-level coding), charset-info/plist (charset introspection), and
//! format-decode (format detection).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p7_decode_encode_coding_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function decode-coding-char)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (decode-coding-char ?\341 'utf-8)
      (encode-coding-char ?é 'utf-8)
      (encode-coding-char ?é 'iso-8859-1)
      (encode-coding-char ?日 'utf-8)
      (length (encode-coding-char ?日 'utf-8)))
"##,
        expect,
    );
}

#[test]
fn div_p7_charset_info_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function charset-id)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (charset-plist 'ascii)
      (charset-dimension 'ascii)
      (charset-dimension 'japanese-jisx0208)
      (charset-chars 'ascii)
      (charset-chars 'japanese-jisx0208)
      (charset-id 'ascii)
      (charset-info 'unicode-bmp))
"##,
        expect,
    );
}

#[test]
fn div_p7_format_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Unknown format unix\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "plain text content here")
  (list (format-decode '(unix) 20 'utf-8)
        (buffer-string)))
"##,
        &["format.el"],
        expect,
    );
}
