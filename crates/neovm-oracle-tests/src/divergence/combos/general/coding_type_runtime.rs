//! Coding-system encode/decode/detect + type-of parity, targeting the
//! recent coding work (utf-8-auto, signatures, detection) and the
//! type-of bignum change.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn coding_type_of_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (integer integer integer integer t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (type-of (expt 2 100)) (type-of 10) (type-of most-positive-fixnum) (type-of (1+ most-positive-fixnum)) (integerp (expt 2 70)) (fixnump (expt 2 70)) (bignump (expt 2 70)))"##,
        expect,
    );
}

#[test]
fn coding_type_of_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (float string integer symbol cons vector hash-table marker)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (type-of 1.5) (type-of "s") (type-of ?x) (type-of 'sym) (type-of '(1)) (type-of []) (type-of (make-hash-table)) (type-of (make-marker)))"##,
        expect,
    );
}

#[test]
fn coding_utf8_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "héllo ⚡ wörld"))
  (string= s (decode-coding-string (encode-coding-string s 'utf-8) 'utf-8)))"##,
        expect,
    );
}

#[test]
fn coding_utf8_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 3 (195 169))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (length (encode-coding-string "café" 'utf-8))
        (length (encode-coding-string "⚡" 'utf-8))
        (append (encode-coding-string "é" 'utf-8) nil))"##,
        expect,
    );
}

#[test]
fn coding_utf16() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((65 0 66 0) (0 65 0 66) \"héllo\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (append (encode-coding-string "AB" 'utf-16le) nil)
        (append (encode-coding-string "AB" 'utf-16be) nil)
        (decode-coding-string (encode-coding-string "héllo" 'utf-16) 'utf-16))"##,
        expect,
    );
}

#[test]
fn coding_utf8_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((239 187 191 104 105) 5 \"hi\" \"\u{feff}hi\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((enc (encode-coding-string "hi" 'utf-8-with-signature)))
  (list (append enc nil) (length enc)
        (decode-coding-string enc 'utf-8-with-signature)
        (decode-coding-string enc 'utf-8)))"##,
        expect,
    );
}

#[test]
fn coding_latin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((233) #(\"é\" 0 1 (charset iso-8859-1)) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (append (encode-coding-string "é" 'latin-1) nil)
        (decode-coding-string (unibyte-string 233) 'latin-1)
        (length (encode-coding-string "café" 'latin-1)))"##,
        expect,
    );
}

#[test]
fn coding_no_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (unibyte-string 0 1 255 128)))
  (list (string= s (encode-coding-string s 'no-conversion))
        (length (encode-coding-string s 'raw-text))))"##,
        expect,
    );
}

#[test]
fn coding_eol_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 1 2 \"a\\nb\" \"a\\nb\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-eol-type 'utf-8-unix)
        (coding-system-eol-type 'utf-8-dos)
        (coding-system-eol-type 'utf-8-mac)
        (decode-coding-string "a\r\nb" 'utf-8-dos)
        (decode-coding-string "a\rb" 'utf-8-mac))"##,
        expect,
    );
}

#[test]
fn coding_check_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (check-coding-system 'utf-8)
        (coding-system-type 'utf-8)
        (coding-system-base 'utf-8-dos))"##,
        expect,
    );
}

#[test]
fn coding_multibyte_string_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 3 t \"αβγ\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "αβγ"))
  (list (string-bytes s) (length s) (multibyte-string-p s)
        (string-to-multibyte s) (multibyte-string-p (string-to-unibyte (encode-coding-string s 'utf-8)))))"##,
        expect,
    );
}

#[test]
fn divergence_detect_coding_string_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8 utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (detect-coding-string (encode-coding-string "héllo wörld" 'utf-8) t)
      (detect-coding-string (encode-coding-string "日本語テスト" 'utf-8) t))"##,
        expect,
    );
}
