//! GNU Emacs 31.1 parity for coding conversion of unibyte `C0`/`C1` pairs
//! and of characters outside the Unicode scalar range.
//!
//! `utf-8` consumes a unibyte `C0 80` as one eight-bit character and writes
//! that byte. `prefer-utf-8` copies the octets. Expectations come from
//! GNU Emacs 31.1 via `NEOVM_ORACLE_MODE=refresh`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_gdh_utf8_unibyte_c0_c1_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (append (encode-coding-string (unibyte-string 192 128) 'utf-8) nil)
  (append (encode-coding-string (unibyte-string 193 191) 'utf-8) nil)
  (append (encode-coding-string (unibyte-string 192 128 195 169) 'utf-8) nil)
  (append (encode-coding-string (unibyte-string 65 192 128 66) 'utf-8) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'utf-8-emacs) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'emacs-internal) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'utf-8-unix) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'utf-8-dos) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'utf-8-with-signature) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'prefer-utf-8) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'undecided) nil)
  (append (encode-coding-string (unibyte-string 192 128) 'raw-text) nil)
  (append (encode-coding-string (unibyte-string 233) 'utf-8) nil)
  (append (encode-coding-string (string #x3FFF80) 'utf-8) nil)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (unibyte-string 192 128 65 193 191))
    (append (encode-coding-region (point-min) (point-max) 'utf-8 t) nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((128) (255) (128 195 169) (65 128 66) (128) (128) (128) (128) (239 187 191 128) (192 128) (192 128) (192 128) (233) (128) (128 65 255))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
