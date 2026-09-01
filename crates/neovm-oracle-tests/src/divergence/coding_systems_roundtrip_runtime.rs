//! Coding-system breadth parity: round-trips for iso-2022 variants, euc-cn/
//! kr/tw, cp1252, mac-roman, ctext, big5-hkscs/gb18030, utf-7/utf-8-auto;
//! aliases/base/eol; charset of decoded chars; plus the
//! coding-system-priority-list HIGHESTP divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn big5_hkscs_gb18030() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-p 'big5-hkscs) (coding-system-p 'gb18030)
        (let ((s "中文")) (string= s (decode-coding-string (encode-coding-string s 'gb18030) 'gb18030)))
        (let ((s "繁體")) (string= s (decode-coding-string (encode-coding-string s 'big5-hkscs) 'big5-hkscs))))"##,
        expect,
    );
}

#[test]
fn charset_after_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 (unicode-bmp))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (decode-coding-string (encode-coding-string "日" 'euc-jp) 'euc-jp)))
  (list (length d) (mapcar (lambda (c) (char-charset c)) (string-to-list d))))"##,
        expect,
    );
}

#[test]
fn coding_aliases_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((utf-8 mule-utf-8 cp65001) (iso-latin-1 iso-8859-1 latin-1) iso-2022-jp 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-aliases 'utf-8) (coding-system-aliases 'iso-latin-1)
        (coding-system-base 'iso-2022-jp-dos) (coding-system-eol-type 'euc-jp-unix))"##,
        expect,
    );
}

#[test]
fn cp_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "café €"))
  (list (coding-system-p 'cp1252) (coding-system-p 'windows-1252)
        (let ((s2 "café")) (string= s2 (decode-coding-string (encode-coding-string s2 'cp1252) 'cp1252)))))"##,
        expect,
    );
}

#[test]
fn ctext_compound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "hello"))
  (list (coding-system-p 'ctext) (coding-system-p 'compound-text)
        (string= s (decode-coding-string (encode-coding-string s 'ctext) 'ctext))))"##,
        expect,
    );
}

#[test]
fn euc_tw_cn_kr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-p 'euc-tw) (coding-system-p 'euc-cn) (coding-system-p 'euc-kr)
        (let ((s "中文")) (string= s (decode-coding-string (encode-coding-string s 'euc-cn) 'euc-cn)))
        (let ((s "한국어")) (string= s (decode-coding-string (encode-coding-string s 'euc-kr) 'euc-kr))))"##,
        expect,
    );
}

#[test]
fn iso2022_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "日本ABC"))
  (list (string= s (decode-coding-string (encode-coding-string s 'iso-2022-jp) 'iso-2022-jp))
        (string= s (decode-coding-string (encode-coding-string s 'iso-2022-jp-2) 'iso-2022-jp-2))
        (string= s (decode-coding-string (encode-coding-string s 'iso-2022-7bit) 'iso-2022-7bit))))"##,
        expect,
    );
}

#[test]
fn mac_roman_viscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-p 'mac-roman) (coding-system-p 'viscii)
        (let ((s "abc")) (string= s (decode-coding-string (encode-coding-string s 'mac-roman) 'mac-roman))))"##,
        expect,
    );
}

#[test]
fn utf7_utf8_auto() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \"x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-p 'utf-7) (coding-system-p 'utf-8-auto)
        (let ((s "Hi")) (string= s (decode-coding-string (encode-coding-string s 'utf-8-auto) 'utf-8-auto)))
        (decode-coding-string (encode-coding-string "x" 'utf-8-with-signature) 'utf-8-auto))"##,
        expect,
    );
}

#[test]
fn divergence_coding_priority_list_highestp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-priority-list t)
      (symbolp (coding-system-priority-list t))
      (listp (coding-system-priority-list)))"##,
        expect,
    );
}
