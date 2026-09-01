//! UTF-8 / multibyte *coding-system registry matrix* (all GNU coding systems).
//!
//! One focused #[test] per coding system in `(coding-system-list t)` (~124).
//! Each decodes a sample byte sequence and compares; unsupported codings
//! substitute U+FFFD in Neomacs vs real characters in GNU (Theme 9).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_csreg_adobe_standard_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 39 8224 168 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'adobe-standard-encoding) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_chinese_big5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12317 31534 36453)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'chinese-big5) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_chinese_big5_hkscs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12317 31534 36453)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'chinese-big5-hkscs) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_chinese_gb18030() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12293 36393 30229)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'chinese-gb18030) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_chinese_gbk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12293 36393 30229)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'chinese-gbk) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_chinese_hz() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'chinese-hz) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_chinese_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12293 36393 30229)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'chinese-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_compound_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'compound-text) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_compound_text_with_extensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'compound-text-with-extensions) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp1125() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1073 1081 9619 9562 1025 164)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp1125) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp437() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 8976 9619 9562 8801 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp437) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp737() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (954 963 9619 9562 911 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp737) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp775() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (298 174 9619 9562 173 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp775) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp850() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 174 9619 9562 173 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp850) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp851() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (912 918 9619 9562 173 974)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp851) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp852() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 281 9619 9562 173 345)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp852) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp855() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1040 1045 9619 9562 173 167)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp855) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp857() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 174 9619 9562 173 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp857) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp858() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 174 9619 9562 173 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp858) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp860() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 210 9619 9562 8801 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp860) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp861() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 8976 9619 9562 8801 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp861) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp862() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 8976 9619 9562 8801 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp862) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp863() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (180 8976 9619 9562 8801 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp863) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp865() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (237 8976 9619 9562 8801 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp865) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp866() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1073 1081 9619 9562 1025 164)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp866) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp869() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (912 918 9619 9562 173 974)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp869) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cp874() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3585 3593 3602 3624 3664 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cp874) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ctext_no_compositions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ctext-no-compositions) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cyrillic_alternativnyj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1073 1081 9619 9562 1025 164)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cyrillic-alternativnyj) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cyrillic_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1025 1033 1042 1064 8470 167)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cyrillic-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_cyrillic_koi8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9553 9560 9569 1093 1055 1065)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'cyrillic-koi8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ebcdic_uk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8254 122 4194226 72 48 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ebcdic-uk) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ebcdic_us() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (126 122 4194226 72 48 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ebcdic-us) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_emacs_mule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'emacs-mule) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_euc_jis_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65311 23478 39023)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'euc-jis-2004) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_euc_tw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65311 1132133 27211)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'euc-tw) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_eucjp_ms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65311 23478 39023)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'eucjp-ms) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_georgian_academy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 4312 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'georgian-academy) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_georgian_ps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 4311 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'georgian-ps) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_greek_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8216 169 178 920 960 973)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'greek-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_hebrew_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4194209 169 178 4194248 1504 8206)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'hebrew-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_hp_roman8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (192 715 253 224 222 187)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'hp-roman8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm038() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (126 122 4194226 72 48 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm038) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm1047() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (126 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm1047) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (126 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm256) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm273() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (223 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm273) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm274() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (168 122 4194226 72 48 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm274) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm275() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (126 122 4194226 72 48 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm275) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm277() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (252 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm277) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm278() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (252 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm278) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm280() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (236 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm280) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm281() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8254 122 4194226 72 48 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm281) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm284() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (168 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm284) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm285() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8254 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm285) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm290() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8254 12516 4194226 72 48 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm290) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_ibm297() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (168 122 165 72 48 217)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'ibm297) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_in_is13194_devanagari() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2305 2314 2317 2346 1572943 1572956)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'in-is13194-devanagari) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_7bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-7bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_7bit_lock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-7bit-lock) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_7bit_lock_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-7bit-lock-ss2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_7bit_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-7bit-ss2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_8bit_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-8bit-ss2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_cn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-cn) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_cn_ext() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-cn-ext) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_jp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-jp) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_jp_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-jp-2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_jp_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-jp-2004) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_2022_kr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-2022-kr) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_8859_11() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3585 3593 3602 3624 3664 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-8859-11) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_8859_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4194209 4194217 4194226 1576 1616 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-8859-6) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-1) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (260 169 268 200 273 281)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-10) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (260 352 731 268 273 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (294 304 178 200 4194288 365)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-3) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (260 352 731 268 273 361)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-4) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 287 305)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-5) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (260 272 275 268 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-6) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8221 169 178 268 353 380)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-7) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7682 169 288 200 373 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_iso_latin_9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'iso-latin-9) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_japanese_cp932() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65377 65385 65394 65416 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'japanese-cp932) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_japanese_iso_7bit_1978_irv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'japanese-iso-7bit-1978-irv) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_japanese_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65311 23478 39023)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'japanese-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_japanese_shift_jis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65377 65385 65394 65416 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'japanese-shift-jis) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_japanese_shift_jis_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65377 65385 65394 65416 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'japanese-shift-jis-2004) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_koi8_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1263 4194217 178 1093 1055 1065)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'koi8-t) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_koi8_u() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9553 9560 9569 1093 1055 1065)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'koi8-u) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_korean_cp949() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (173 44866 32305)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'korean-cp949) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_korean_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (173 44866 32305)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'korean-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_lao() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'lao) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_mac_roman() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (176 169 8804 187 57374 733)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'mac-roman) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_mik() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1073 1081 1090 9562 8801 178)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'mik) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 8217 8224 168 246 255)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'next) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_no_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'no-conversion) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_no_conversion_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'no-conversion-multibyte) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_prefer_utf_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'prefer-utf-8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_pt154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1038 169 1030 1048 1088 1101)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'pt154) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_raw_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'raw-text) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_thai_tis620() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3585 3593 3602 3624 3664 3677)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'thai-tis620) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_tibetan_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1638408 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'tibetan-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_undecided() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'undecided) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_us_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'us-ascii) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (41385 45768 61693)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-16) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_16be() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (41385 45768 61693)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-16be) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_16be_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (41385 45768 61693)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-16be-with-signature) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_16le() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (43425 51378 65008)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-16le) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_16le_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (43425 51378 65008)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-16le-with-signature) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-7) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_7_imap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-7-imap) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_8_auto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-8-auto) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_8_emacs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-8-emacs) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_utf_8_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'utf-8-with-signature) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_vietnamese_viqr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4194209 4194217 4194226 4194248 4194288 4194301)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'vietnamese-viqr) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_vietnamese_viscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7855 7865 7895 200 273 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'vietnamese-viscii) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_vietnamese_vscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (258 226 771 7849 7894 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'vietnamese-vscii) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1250() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (711 169 731 268 273 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1250) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1251() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1038 169 1030 1048 1088 1101)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1251) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 240 253)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1252) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1253() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (901 169 178 920 960 973)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1253) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1254() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 287 305)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1254) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 1464 1504 8206)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1255) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1548 169 178 1576 1611 8206)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1256) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1257() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4194209 169 178 268 353 380)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1257) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csreg_windows_1258() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (161 169 178 200 273 432)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (decode-coding-string (unibyte-string 161 169 178 200 240 253) 'windows-1258) nil)",
        expect,
    );
}
