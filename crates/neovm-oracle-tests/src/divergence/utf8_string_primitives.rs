//! UTF-8 / multibyte *string primitive* divergence probes.
//!
//! Probes `length`/`string-bytes`, `aref`/`aset`/`substring`/`store-substring`,
//! `concat` mixing unibyte and multibyte, `split-string`, `char-to-string`,
//! `format` with `%c`/`%s`, and `mapconcat` — all over non-ASCII text.  Under a
//! UTF-8-internal model the byte accounting (`string-bytes`) and raw-byte
//! promotion in `concat`/`store-substring` are the likeliest divergence points.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_str_length_vs_string_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 16 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s "héllo世界😀"))
  (list (length s) (string-bytes s) (multibyte-string-p s)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_str_length_bytes_latin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s "café"))
  (list (length s) (string-bytes s)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_aref_multibyte_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (104 233 108 111)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s "héllo"))
  (list (aref s 0) (aref s 1) (aref s 2) (aref s 4)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_aset_high_codepoint_growth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Attempt to store non-byte value into unibyte string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s (copy-sequence "abcdef")))
  (aset s 1 #x3042)
  (list (length s) (string-bytes s) (append s nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_substring_multibyte_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aé\" \"ébç\" \"bçd\" \"bçd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s "aébçd"))
  (list (substring s 0 2) (substring s 1 4) (substring s -3) (substring s 2)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_concat_unibyte_and_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    // Concatenating a unibyte-with-raw-bytes and a multibyte string must
    // promote the raw bytes to eight-bit characters.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((r (concat (unibyte-string 200 201) "xy")))
  (list (multibyte-string-p r) (unibyte-string-p r)
        (length r) (string-bytes r) (append r nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_concat_two_unibyte_stays_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((r (concat (unibyte-string 200) (unibyte-string 201))))
  (list (multibyte-string-p r) (unibyte-string-p r)
        (length r) (string-bytes r) (append r nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_split_string_multibyte_sep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"bçd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(split-string "aébçd" "é")
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_to_string_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é\" 1 2 1 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-to-string ?é)
      (length (char-to-string ?é))
      (string-bytes (char-to-string ?é))
      (length (char-to-string #x3042))
      (string-bytes (char-to-string #x3042)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_format_percent_c_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é\" \"あ\" \"😀\" 1 1 1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((c1 (format "%c" 233))
      (c2 (format "%c" #x3042))
      (c3 (format "%c" #x1f600)))
  (list c1 c2 c3
        (length c1) (length c2) (length c3)
        (string-bytes c1) (string-bytes c2) (string-bytes c3)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_format_percent_s_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" \"café4\" \"\\\"café\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s "café"))
  (list (format "%s" s) (format "%s%d" s (length s)) (format "%S" s)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_mapconcat_char_codes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a-é-b\" \"61,E9,7A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (mapconcat #'char-to-string "aéb" "-")
      (mapconcat (lambda (c) (format "%X" c)) "aéz" ","))
"#,
        expect,
    );
}

#[test]
fn div_utf8_store_substring_byte_indexed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Attempt to store non-byte value into unibyte string\")""#
    ]];
    // store-substring is byte-indexed and can grow the string's byte length.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s (copy-sequence "abcdef")))
  (store-substring s 2 #x3042)
  (list (length s) (string-bytes s) (append s nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_store_substring_raw_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Attempt to store non-byte value into unibyte string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s (copy-sequence "abcdef")))
  (store-substring s 1 (unibyte-char-to-multibyte 200))
  (list (length s) (string-bytes s) (append s nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_empty_string_multibyte_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (multibyte-string-p "")
      (unibyte-string-p "")
      (multibyte-string-p (unibyte-string))
      (multibyte-string-p (string-make-multibyte "")))
"#,
        expect,
    );
}

#[test]
fn div_utf8_string_equal_after_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((literal "café")
      (decoded (decode-coding-string (unibyte-string 99 97 102 233) 'latin-1)))
  (list (equal literal decoded)
        (equal (append literal nil) (append decoded nil))))
"#,
        expect,
    );
}
