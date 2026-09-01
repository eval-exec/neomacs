/// Batch 541: string-ao, string-as-unibyte, string-as-multibyte, encode-coding.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx541_string_as_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "abc"))
  (string-as-unibyte s))
"##,
        expect,
    );
}

#[test]
fn div_cx541_string_as_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (string-as-unibyte "abc")))
  (string-as-multibyte s))
"##,
        expect,
    );
}

#[test]
fn div_cx541_string_to_unibyte_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "abc"))
  (string-to-unibyte s))
"##,
        expect,
    );
}

#[test]
fn div_cx541_string_to_multibyte_round() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (string-to-unibyte "abc")))
  (string-to-multibyte s))
"##,
        expect,
    );
}

#[test]
fn div_cx541_encode_coding_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abc\" 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (encode-coding-string "abc" 'utf-8)
      (string-bytes (encode-coding-string "abc" 'utf-8)))
"##,
        expect,
    );
}

#[test]
fn div_cx541_decode_coding_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((enc (encode-coding-string "hello" 'utf-8)))
  (decode-coding-string enc 'utf-8))
"##,
        expect,
    );
}

#[test]
fn div_cx541_encode_coding_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (encode-coding-region (point-min) (point-max) 'utf-8)
  (buffer-size))
"##,
        expect,
    );
}

#[test]
fn div_cx541_decode_coding_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (encode-coding-region (point-min) (point-max) 'utf-8)
  (decode-coding-region (point-min) (point-max) 'utf-8)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx541_multibyte_string_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (multibyte-string-p "abc")
      (multibyte-string-p (string-to-multibyte "abc"))
      (multibyte-string-p (string-as-multibyte "abc")))
"##,
        expect,
    );
}

#[test]
fn div_cx541_string_make_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-multibyte-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "abc"))
  (string-multibyte-p (string-make-multibyte s)))
"##,
        expect,
    );
}

#[test]
fn div_cx541_string_unibyte_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-unibyte-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "abc"))
  (list (string-unibyte-p s)
        (string-unibyte-p (string-to-unibyte s))))
"##,
        expect,
    );
}

#[test]
fn div_cx541_unibyte_to_multibyte_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65 4194248)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (unibyte-char-to-multibyte 65)
      (unibyte-char-to-multibyte 200))
"##,
        expect,
    );
}

#[test]
fn div_cx541_multibyte_to_unibyte_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 200""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (multibyte-char-to-unibyte 200)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx541_string_bytes_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "cafe世界"))
  (list (string-bytes s) (length s)))
"##,
        expect,
    );
}

#[test]
fn div_cx541_string_width_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "cafe世界"))
  (list (string-width s) (length s)))
"##,
        expect,
    );
}
