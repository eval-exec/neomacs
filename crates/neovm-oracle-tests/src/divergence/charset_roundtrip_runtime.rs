//! Non-UTF charset round-trips (shift_jis, euc-jp, gbk, big5, koi8-r,
//! iso-2022-jp) and char encode/decode; plus the iso-8859-15 decode
//! charset-text-property gap.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn charset_encode_specific() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"A\" 233 nil 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (encode-coding-char ?A 'utf-8)
        (multibyte-char-to-unibyte ?é)
        (decode-char 'latin-iso8859-1 233)
        (encode-char ?A 'ascii))"##,
        expect,
    );
}

#[test]
fn euc_jp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "テスト123"))
  (list (string= s (decode-coding-string (encode-coding-string s 'euc-jp) 'euc-jp))
        (length (encode-coding-string s 'euc-jp))))"##,
        expect,
    );
}

#[test]
fn gb_big5() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s1 "中文") (s2 "繁體"))
  (list (string= s1 (decode-coding-string (encode-coding-string s1 'gbk) 'gbk))
        (string= s2 (decode-coding-string (encode-coding-string s2 'big5) 'big5))))"##,
        expect,
    );
}

#[test]
fn iso2022_jp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "漢字テスト"))
  (string= s (decode-coding-string (encode-coding-string s 'iso-2022-jp) 'iso-2022-jp)))"##,
        expect,
    );
}

#[test]
fn koi8_cyrillic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "Привет"))
  (list (string= s (decode-coding-string (encode-coding-string s 'koi8-r) 'koi8-r))
        (length (encode-coding-string s 'koi8-r))))"##,
        expect,
    );
}

#[test]
fn shift_jis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "日本語 abc"))
  (list (string= s (decode-coding-string (encode-coding-string s 'shift_jis) 'shift_jis))
        (length (encode-coding-string s 'shift_jis))))"##,
        expect,
    );
}

#[test]
fn divergence_decode_iso8859_15_charset_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (#(\"é€\" 0 2 (charset iso-8859-15)) iso-8859-15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (decode-coding-string (unibyte-string 233 164) 'iso-8859-15)
      (get-text-property 0 'charset (decode-coding-string (unibyte-string 164) 'iso-8859-15)))"##,
        expect,
    );
}
