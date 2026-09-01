//! UTF-8 / multibyte *charset-chars matrix* (all GNU charsets).
//!
//! One focused #[test] per charset: `(charset-chars \'NAME)`. Confirmed root
//! cause (Theme 10): charset-chars ERRORS (wrong-type-argument arrayp nil)
//! in Neomacs for every charset, GNU returns the char count.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_cschars_adobe_standard_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 224""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'adobe-standard-encoding) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_alternativnyj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'alternativnyj) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_arabic_1_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'arabic-1-column) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_arabic_2_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'arabic-2-column) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_arabic_digit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 9""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'arabic-digit) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_arabic_iso8859_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'arabic-iso8859-6) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 128""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ascii) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_assamese_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'assamese-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_bengali_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'bengali-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_bengali_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'bengali-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_big5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 191""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'big5) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_big5_hkscs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 191""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'big5-hkscs) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_big5_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-big5-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_big5_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-big5-2) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_15() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-15) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-2) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-3) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-4) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-5) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-6) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_cns11643_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-cns11643-7) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_gb2312() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-gb2312) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_gbk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 191""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-gbk) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_chinese_sisheng() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'chinese-sisheng) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_control_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 32""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'control-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp00858() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp00858) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp038() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp038) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1047() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1047) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1125() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1125) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1250() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1250) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1251() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1251) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1252) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1253() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1253) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1254() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1254) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1255) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1256) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1257() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1257) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp1258() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp1258) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp154) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp437() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp437) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp720() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp720) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp737() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp737) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp775() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp775) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp850() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp850) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp851() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp851) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp852() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp852) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp855() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp855) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp857() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp857) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp858() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp858) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp860() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp860) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp861() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp861) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp862() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp862) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp863() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp863) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp864() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp864) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp865() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp865) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp866() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp866) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp866u() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp866u) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp869() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp869) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp874() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp874) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp932() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp932) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp932_2_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 189""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp932-2-byte) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp936() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 191""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp936) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp949() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 255""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp949) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cp949_2_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 190""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cp949-2-byte) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_cyrillic_iso8859_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'cyrillic-iso8859-5) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_devanagari_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'devanagari-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_devanagari_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'devanagari-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ebcdic_int() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ebcdic-int) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ebcdic_uk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ebcdic-uk) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ebcdic_us() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ebcdic-us) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_eight_bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 128""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'eight-bit) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_eight_bit_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 32""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'eight-bit-control) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_eight_bit_graphic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'eight-bit-graphic) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_emacs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'emacs) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ethiopic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ethiopic) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gb18030() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gb18030) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gb18030_2_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 191""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gb18030-2-byte) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gb18030_4_byte_bmp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gb18030-4-byte-bmp) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gb18030_4_byte_ext_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gb18030-4-byte-ext-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gb18030_4_byte_ext_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gb18030-4-byte-ext-2) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gb18030_4_byte_smp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gb18030-4-byte-smp) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_georgian_academy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'georgian-academy) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_georgian_ps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'georgian-ps) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_greek_iso8859_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'greek-iso8859-7) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gujarati_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gujarati-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_gujarati_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'gujarati-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_hebrew_iso8859_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'hebrew-iso8859-8) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_hp_roman8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'hp-roman8) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm038() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm038) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm1047() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm1047) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm256) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm273() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm273) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm274() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm274) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm275() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm275) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm277() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm277) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm278() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm278) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm280() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm280) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm281() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm281) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm284() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm284) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm285() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm285) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm290() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm290) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm297() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm297) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm850() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm850) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ibm866() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ibm866) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_indian_1_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'indian-1-column) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_indian_2_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'indian-2-column) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_indian_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'indian-glyph) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_indian_is13194() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'indian-is13194) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ipa() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ipa) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-10) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_11() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-11) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_13() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-13) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_14() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-14) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_15() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-15) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-16) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-2) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-3) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-4) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-5) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-6) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-7) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-8) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_iso_8859_9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'iso-8859-9) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_japanese_jisx0208() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'japanese-jisx0208) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_japanese_jisx0208_1978() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'japanese-jisx0208-1978) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_japanese_jisx0212() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'japanese-jisx0212) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_japanese_jisx0213_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'japanese-jisx0213-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_japanese_jisx0213_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'japanese-jisx0213-2) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_japanese_jisx0213_a() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'japanese-jisx0213-a) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_japanese_jisx0213_2004_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'japanese-jisx0213.2004-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_jisx0201() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 224""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'jisx0201) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_kannada_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'kannada-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_kannada_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'kannada-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_katakana_jisx0201() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'katakana-jisx0201) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_katakana_sjis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 63""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'katakana-sjis) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_koi8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'koi8) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_koi8_r() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'koi8-r) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_koi8_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'koi8-t) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_koi8_u() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'koi8-u) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_korean_ksc5601() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'korean-ksc5601) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_lao() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'lao) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-1) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-10) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_13() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-13) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_14() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-14) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_15() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-15) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-16) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-2) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-3) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-4) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_iso8859_9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-iso8859-9) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_latin_jisx0201() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'latin-jisx0201) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_mac_roman() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'mac-roman) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_malayalam_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'malayalam-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_malayalam_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'malayalam-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_mik() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'mik) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_mule_lao() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'mule-lao) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_mule_unicode_0100_24ff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'mule-unicode-0100-24ff) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_mule_unicode_2500_33ff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'mule-unicode-2500-33ff) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_mule_unicode_e000_ffff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'mule-unicode-e000-ffff) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'next) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_oriya_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'oriya-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_oriya_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'oriya-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_pt154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'pt154) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ptcp154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ptcp154) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_punjabi_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'punjabi-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_punjabi_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'punjabi-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ruscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ruscii) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_sanskrit_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'sanskrit-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 224""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'symbol) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_tamil_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'tamil-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_tamil_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'tamil-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_tcvn_5712() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'tcvn-5712) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_telugu_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'telugu-akruti) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_telugu_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'telugu-cdac) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_thai_iso8859_11() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'thai-iso8859-11) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_thai_tis620() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'thai-tis620) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_tibetan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'tibetan) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_tibetan_1_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'tibetan-1-column) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_tis620_2533() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'tis620-2533) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_ucs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'ucs) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'unicode) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_unicode_bmp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'unicode-bmp) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_unicode_sip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'unicode-sip) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_unicode_smp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'unicode-smp) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_unicode_ssp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'unicode-ssp) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_vietnamese_viscii_lower() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'vietnamese-viscii-lower) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_vietnamese_viscii_upper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'vietnamese-viscii-upper) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_viscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'viscii) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_vscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'vscii) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_vscii_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'vscii-2) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1250() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1250) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1251() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1251) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1252) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1253() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1253) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1254() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1254) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1255) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1256) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1257() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1257) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_1258() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 256""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-1258) (error (cons (quote errored) (car e))))",
        expect,
    );
}

#[test]
fn div_utf8_cschars_windows_936() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 191""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case e (charset-chars \'windows-936) (error (cons (quote errored) (car e))))",
        expect,
    );
}
