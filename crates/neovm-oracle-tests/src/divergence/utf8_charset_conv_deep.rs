//! UTF-8 / multibyte *charset-conversion deep* divergence probes.
//!
//! Targets the three-way distinction between interpreting raw bytes as
//! (`string-make-multibyte` vs `string-as-multibyte` vs `decode-coding-string`),
//! legacy iso-2022 charset construction (`make-char`), `ucs-normalize`
//! NFC/NFD/NFKC, and charset dimension tables.  `string-make-multibyte` of a
//! valid UTF-8 byte sequence is a canonical UTF-8-internal divergence: GNU
//! treats each byte as a raw eight-bit char and does NOT decode, whereas a
//! UTF-8-internal reimpl tends to decode the sequence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- string-make-multibyte must NOT decode UTF-8 ----------------------------

#[test]
fn div_utf8_string_make_multibyte_utf8_bytes_not_decoded() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 (4194243 4194217) t)""#]];
    // Bytes 195 169 are UTF-8 for é. string-make-multibyte must NOT decode
    // them; it must produce two distinct eight-bit chars.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((m (string-make-multibyte (unibyte-string 195 169))))
  (list (length m) (string-bytes m) (append m nil)
        (multibyte-string-p m)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_make_vs_as_vs_decode_three_way() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (5 2 2 (233 8364) (4194243 4194217 4194274 4194178 4194220))""#
    ]];
    // The same bytes interpreted three different ways must give three
    // distinct results in GNU.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((bytes (unibyte-string 195 169 226 130 172)))   ; é, € as UTF-8
  (list (length (string-make-multibyte bytes))
        (length (string-as-multibyte bytes))
        (length (decode-coding-string bytes 'utf-8))
        (append (decode-coding-string bytes 'utf-8) nil)
        (append (string-make-multibyte bytes) nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_string_make_multibyte_each_byte_is_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (4 (4194288 4194207 4194200 4194176) (eight-bit eight-bit eight-bit eight-bit))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((m (string-make-multibyte (unibyte-string 240 159 152 128))))  ; emoji UTF-8
  (list (length m) (append m nil)
        (mapcar #'char-charset (append m nil))))
"#,
        expect,
    );
}

// --- legacy iso-2022 charset construction -----------------------------------

#[test]
fn div_utf8_make_char_legacy_iso2022_charsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12354 38797 12288 169)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (condition-case err (make-char 'japanese-jisx0208 36 34) (error (cons 'err (car err))))
      (condition-case err (make-char 'chinese-gb2312 48 48) (error (cons 'err (car err))))
      (condition-case err (make-char 'korean-ksc5601 33 33) (error (cons 'err (car err))))
      (condition-case err (make-char 'latin-iso8859-1 41) (error (cons 'err (car err)))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_make_char_legacy_charset_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12354 unicode-bmp 9250)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(condition-case err
    (let ((c (make-char 'japanese-jisx0208 36 34)))
      (list c
            (char-charset c)
            (encode-char c 'japanese-jisx0208)))
  (error (cons 'err (car err))))
"#,
        expect,
    );
}

// --- insert raw bytes into a multibyte buffer -------------------------------

#[test]
fn div_utf8_insert_unibyte_into_multibyte_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 t (97 4194248))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "a")
  (insert (unibyte-string 200))
  (list (point-max)
        (multibyte-string-p (buffer-string))
        (append (buffer-string) nil)))
"#,
        expect,
    );
}

// --- ucs-normalize NFC / NFD / NFKC -----------------------------------------

#[test]
fn div_utf8_ucs_normalize_compose_decompose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (ucs-normalize-string "café" 'NFC)
      (ucs-normalize-string "café" 'NFD)
      (length (ucs-normalize-string "café" 'NFD))
      (append (ucs-normalize-string "café" 'NFD) nil)
      (ucs-normalize-string (string #xFB01) 'NFKC)
      (length (ucs-normalize-string (string #xFB01) 'NFKC))
      (equal (ucs-normalize-string "café" 'NFC)
             (ucs-normalize-string (concat "cafe" (string #x301)) 'NFC)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_ucs_normalize_korean_hangul() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    // Hangul has algorithmic (not table) composition in NFC.
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((composed (string #xAC00))                 ; 가
       (decomposed (ucs-normalize-string composed 'NFD)))
  (list (length composed) (length decomposed)
        (append decomposed nil)
        (ucs-normalize-string decomposed 'NFC)))
"#,
        expect,
    );
}

// --- charset dimension tables -----------------------------------------------

#[test]
fn div_utf8_charset_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function charset-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (charset-dimension 'ascii)
      (charset-dimension 'latin-iso8859-1)
      (charset-dimension 'japanese-jisx0208)
      (charset-dimension 'unicode)
      (charset-dimension 'eight-bit)
      (length (charset-list)))
"#,
        expect,
    );
}
