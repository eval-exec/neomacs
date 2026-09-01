//! UTF-8 / multibyte *coding-system encode matrix* (all GNU coding systems).
//!
//! One focused #[test] per coding system: encode "café世界" (built via char
//! codes to avoid Rust string-escape pitfalls) and compare bytes. Unsupported
//! codings return nil in Neomacs vs a byte string in GNU (Theme 9, encode).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_csenc_adobe_standard_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'adobe-standard-encoding) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_chinese_big5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 165 64 172 201)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'chinese-big5) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_chinese_big5_hkscs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 165 64 172 201)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'chinese-big5-hkscs) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_chinese_gb18030() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 168 166 202 192 189 231)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'chinese-gb18030) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_chinese_gbk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 168 166 202 192 189 231)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'chinese-gbk) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_chinese_hz() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 126 123 40 38 74 64 61 103 126 125)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'chinese-hz) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_chinese_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 168 166 202 192 189 231)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'chinese-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_compound_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (99 97 102 233 27 36 40 65 74 64 61 103 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'compound-text) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_compound_text_with_extensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (99 97 102 233 27 36 40 65 74 64 61 103 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'compound-text-with-extensions) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp1125() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp1125) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp437() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp437) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp737() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp737) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp775() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp775) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp850() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp850) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp851() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp851) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp852() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp852) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp855() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp855) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp857() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp857) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp858() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp858) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp860() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp860) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp861() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp861) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp862() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp862) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp863() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp863) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp865() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 130 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp865) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp866() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp866) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp869() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp869) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cp874() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cp874) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ctext_no_compositions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 27 36 65 74 64 61 103 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ctext-no-compositions) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cyrillic_alternativnyj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cyrillic-alternativnyj) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cyrillic_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cyrillic-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_cyrillic_koi8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'cyrillic-koi8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ebcdic_uk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ebcdic-uk) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ebcdic_us() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ebcdic-us) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_emacs_mule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 129 233 145 202 192 145 189 231)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'emacs-mule) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_euc_jis_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 169 223 192 164 179 166)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'euc-jis-2004) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_euc_tw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 197 224 210 204)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'euc-tw) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_eucjp_ms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 143 171 177 192 164 179 166)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'eucjp-ms) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_georgian_academy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'georgian-academy) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_georgian_ps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'georgian-ps) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_greek_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'greek-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_hebrew_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'hebrew-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_hp_roman8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 197 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'hp-roman8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm038() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm038) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm1047() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 81 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm1047) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 81 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm256) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm273() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 81 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm273) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm274() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 192 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm274) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm275() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 208 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm275) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm277() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 81 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm277) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm278() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 121 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm278) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm280() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 90 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm280) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm281() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm281) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm284() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 81 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm284) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm285() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 81 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm285) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm290() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (32 32 32 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm290) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_ibm297() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (131 129 134 192 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'ibm297) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_in_is13194_devanagari() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'in-is13194-devanagari) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_7bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (99 97 102 27 44 65 105 27 36 65 74 64 61 103 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-7bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_7bit_lock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (99 97 102 27 45 65 14 105 27 36 65 15 74 64 61 103 27 40 66)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-7bit-lock) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_7bit_lock_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (99 97 102 27 36 65 40 38 27 36 66 64 36 51 38 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-7bit-lock-ss2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_7bit_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (99 97 102 27 46 65 27 78 105 27 36 65 74 64 61 103 27 40 66)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-7bit-ss2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_8bit_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (99 97 102 27 46 65 142 233 27 36 65 74 64 61 103 27 40 66)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-8bit-ss2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_cn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 27 36 65 40 38 74 64 61 103 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-cn) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_cn_ext() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 27 36 65 40 38 74 64 61 103 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-cn-ext) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_jp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 27 36 66 64 36 51 38 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-jp) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_jp_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (99 97 102 27 36 40 68 43 49 27 36 66 64 36 51 38 27 40 66)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-jp-2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_jp_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (99 97 102 27 36 40 81 41 95 27 36 66 64 36 51 38 27 40 66)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-jp-2004) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_2022_kr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 27 36 40 67 97 38 77 35 27 40 66)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-2022-kr) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_8859_11() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-8859-11) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_8859_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-8859-6) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-1) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-10) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-2) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-3) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-4) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-5) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-6) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-7) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_iso_latin_9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'iso-latin-9) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_japanese_cp932() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 144 162 138 69)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'japanese-cp932) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_japanese_iso_7bit_1978_irv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (99 97 102 27 36 40 68 43 49 27 36 64 64 36 51 38 27 40 74)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'japanese-iso-7bit-1978-irv) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_japanese_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 143 171 177 192 164 179 166)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'japanese-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_japanese_shift_jis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 144 162 138 69)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'japanese-shift-jis) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_japanese_shift_jis_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 133 126 144 162 138 69)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'japanese-shift-jis-2004) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_koi8_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'koi8-t) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_koi8_u() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'koi8-u) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_korean_cp949() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 225 166 205 163)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'korean-cp949) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_korean_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 225 166 205 163)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'korean-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_lao() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (32 32 32 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'lao) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_mac_roman() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 142 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'mac-roman) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_mik() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'mik) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 221 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'next) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_no_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'no-conversion) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_no_conversion_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'no-conversion-multibyte) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_prefer_utf_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'prefer-utf-8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_pt154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'pt154) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_raw_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'raw-text) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_thai_tis620() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'thai-tis620) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_tibetan_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'tibetan-iso-8bit) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_undecided() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'undecided) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_us_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 63 63 63)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'us-ascii) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (254 255 0 99 0 97 0 102 0 233 78 22 117 76)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-16) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_16be() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 99 0 97 0 102 0 233 78 22 117 76)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-16be) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_16be_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (254 255 0 99 0 97 0 102 0 233 78 22 117 76)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-16be-with-signature) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_16le() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 0 97 0 102 0 233 0 22 78 76 117)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-16le) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_16le_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (255 254 99 0 97 0 102 0 233 0 22 78 76 117)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-16le-with-signature) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 43 65 79 108 79 70 110 86 77)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-7) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_7_imap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 38 65 79 108 79 70 110 86 77 45)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-7-imap) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-8) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_8_auto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (239 187 191 99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-8-auto) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_8_emacs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-8-emacs) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_utf_8_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (239 187 191 99 97 102 195 169 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'utf-8-with-signature) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_vietnamese_viqr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 101 39 228 184 150 231 149 140)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'vietnamese-viqr) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_vietnamese_viscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'vietnamese-viscii) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_vietnamese_vscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 208 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'vietnamese-vscii) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1250() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1250) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1251() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1251) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1252) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1253() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1253) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1254() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1254) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 32 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1255) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1256) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1257() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1257) nil)",
        expect,
    );
}

#[test]
fn div_utf8_csenc_windows_1258() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 97 102 233 32 32)""#]];
    crate::common::assert_oracle_parity_expect(
        "(append (encode-coding-string (string 99 97 102 233 19990 30028) 'windows-1258) nil)",
        expect,
    );
}
