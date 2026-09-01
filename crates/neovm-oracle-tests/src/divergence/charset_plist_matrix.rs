//! Per-charset *charset-plist* matrix (all GNU charsets).
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cs_plist_adobe_standard_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name adobe-standard-encoding :docstring \"Adobe `standard encoding' used in PostScript\" :short-name \"ADOBE-STANDARD-ENCODING\" :code-space [32 255] :map \"stdenc\" :dimension 1 :long-name \"ADOBE-STANDARD-ENCODING\" :base adobe-standard-encoding)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'adobe-standard-encoding)", expect);
}

#[test]
fn div_cs_plist_alternativnyj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name alternativnyj :docstring \"ALTERNATIVNYJ\" :short-name \"alternativnyj\" :ascii-compatible-p t :code-space [0 255] :map \"ALTERNATIVNYJ\" :dimension 1 :long-name \"alternativnyj\" :base alternativnyj)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'alternativnyj)", expect);
}

#[test]
fn div_cs_plist_arabic_1_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name arabic-1-column :docstring \"Arabic 1-column\" :short-name \"Arabic 1-col\" :long-name \"Arabic 1-column\" :iso-final-char 51 :emacs-mule-id 165 :supplementary-p t :code-space [33 126] :code-offset 2097408 :dimension 1 :base arabic-1-column preferred-coding-system iso-2022-7bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'arabic-1-column)", expect);
}

#[test]
fn div_cs_plist_arabic_2_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name arabic-2-column :docstring \"Arabic 2-column\" :short-name \"Arabic 2-col\" :long-name \"Arabic 2-column\" :iso-final-char 52 :emacs-mule-id 224 :supplementary-p t :code-space [33 126] :code-offset 2097536 :dimension 1 :base arabic-2-column preferred-coding-system iso-2022-7bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'arabic-2-column)", expect);
}

#[test]
fn div_cs_plist_arabic_digit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name arabic-digit :docstring \"Arabic digit\" :short-name \"Arabic digit\" :iso-final-char 50 :emacs-mule-id 164 :supplementary-p t :code-space [34 42] :code-offset 1536 :dimension 1 :long-name \"Arabic digit\" :base arabic-digit preferred-coding-system iso-2022-7bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'arabic-digit)", expect);
}

#[test]
fn div_cs_plist_arabic_iso8859_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name arabic-iso8859-6 :docstring \"Right-Hand Part of ISO/IEC 8859/6 (Latin/Arabic): ISO-IR-127\" :short-name \"RHP of ISO/IEC 8859/6\" :long-name \"RHP of ISO/IEC 8859/6 (Latin/Arabic)\" :iso-final-char 71 :emacs-mule-id 135 :code-space [32 127] :subset (iso-8859-6 160 255 -128) :dimension 1 :base arabic-iso8859-6 preferred-coding-system iso-2022-7bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'arabic-iso8859-6)", expect);
}

#[test]
fn div_cs_plist_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ascii :dimension 1 :code-space [0 127 0 0 0 0 0 0] :iso-final-char 66 :emacs-mule-id 0 :ascii-compatible-p t :code-offset 0 :docstring \"ASCII (ISO646 IRV)\" :short-name \"ASCII\" :long-name \"ASCII (ISO646 IRV)\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ascii)", expect);
}

#[test]
fn div_cs_plist_assamese_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name assamese-cdac :docstring \"Glyphs of Assamese script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Assamese glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1574400 :dimension 1 :long-name \"CDAC Assamese glyphs\" :base assamese-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'assamese-cdac)", expect);
}

#[test]
fn div_cs_plist_bengali_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name bengali-akruti :docstring \"Glyphs of Bengali script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Bengali glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1576192 :dimension 1 :long-name \"AKRUTI Bengali glyphs\" :base bengali-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'bengali-akruti)", expect);
}

#[test]
fn div_cs_plist_bengali_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name bengali-cdac :docstring \"Glyphs of Bengali script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Bengali glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1573632 :dimension 1 :long-name \"CDAC Bengali glyphs\" :base bengali-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'bengali-cdac)", expect);
}

#[test]
fn div_cs_plist_big5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name big5 :docstring \"Big5 (Chinese traditional)\" :short-name \"Big5\" :code-space [64 254 161 254] :code-offset 1245184 :unify-map \"BIG5\" :dimension 2 :long-name \"Big5\" :base big5)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'big5)", expect);
}

#[test]
fn div_cs_plist_big5_hkscs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name big5-hkscs :docstring \"Big5-HKSCS (Chinese traditional, Hong Kong supplement)\" :short-name \"Big5\" :code-space [64 254 161 254] :code-offset 2605592 :unify-map \"BIG5-HKSCS\" :dimension 2 :long-name \"Big5\" :base big5-hkscs)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'big5-hkscs)", expect);
}

#[test]
fn div_cs_plist_chinese_big5_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-big5-1 :docstring \"Frequently used part (A141-C67E) of Big5 (Chinese traditional)\" :short-name \"Big5 (Level-1)\" :long-name \"Big5 (Level-1) A141-C67F\" :iso-final-char 48 :emacs-mule-id 152 :supplementary-p t :code-space [33 126 33 126] :code-offset 1265664 :unify-map \"BIG5-1\" :dimension 2 :base chinese-big5-1 preferred-coding-system chinese-big5)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-big5-1)", expect);
}

#[test]
fn div_cs_plist_chinese_big5_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-big5-2 :docstring \"Less frequently used part (C940-FEFE) of Big5 (Chinese traditional)\" :short-name \"Big5 (Level-2)\" :long-name \"Big5 (Level-2) C940-FEFE\" :iso-final-char 49 :emacs-mule-id 153 :supplementary-p t :code-space [33 126 33 126] :code-offset 1275904 :unify-map \"BIG5-2\" :dimension 2 :base chinese-big5-2 preferred-coding-system chinese-big5)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-big5-2)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-1 :docstring \"CNS11643 Plane 1 Chinese traditional: ISO-IR-171\" :short-name \"CNS11643-1\" :long-name \"CNS11643-1 (Chinese traditional): ISO-IR-171\" :iso-final-char 71 :emacs-mule-id 149 :code-space [33 126 33 126] :code-offset 1130496 :unify-map \"CNS-1\" :dimension 2 :base chinese-cns11643-1 preferred-coding-system iso-2022-cn)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-1)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_15() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-15 :docstring \"CNS11643 Plane 15 Chinese Traditional\" :short-name \"CNS11643-15\" :long-name \"CNS11643-15 (Chinese traditional)\" :code-space [33 126 33 126] :code-offset 2623546 :unify-map \"CNS-F\" :dimension 2 :base chinese-cns11643-15)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-15)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-2 :docstring \"CNS11643 Plane 2 Chinese traditional: ISO-IR-172\" :short-name \"CNS11643-2\" :long-name \"CNS11643-2 (Chinese traditional): ISO-IR-172\" :iso-final-char 72 :emacs-mule-id 150 :code-space [33 126 33 126] :code-offset 1146880 :unify-map \"CNS-2\" :dimension 2 :base chinese-cns11643-2 preferred-coding-system iso-2022-cn)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-2)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-3 :docstring \"CNS11643 Plane 3 Chinese Traditional: ISO-IR-183\" :short-name \"CNS11643-3\" :long-name \"CNS11643-3 (Chinese traditional): ISO-IR-183\" :iso-final-char 73 :code-space [33 126 33 126] :emacs-mule-id 246 :code-offset 1163264 :unify-map \"CNS-3\" :dimension 2 :base chinese-cns11643-3 preferred-coding-system iso-2022-cn)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-3)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-4 :docstring \"CNS11643 Plane 4 Chinese Traditional: ISO-IR-184\" :short-name \"CNS11643-4\" :long-name \"CNS11643-4 (Chinese traditional): ISO-IR-184\" :iso-final-char 74 :emacs-mule-id 247 :code-space [33 126 33 126] :code-offset 1179648 :unify-map \"CNS-4\" :dimension 2 :base chinese-cns11643-4 preferred-coding-system iso-2022-cn)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-4)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-5 :docstring \"CNS11643 Plane 5 Chinese Traditional: ISO-IR-185\" :short-name \"CNS11643-5\" :long-name \"CNS11643-5 (Chinese traditional): ISO-IR-185\" :iso-final-char 75 :emacs-mule-id 248 :code-space [33 126 33 126] :code-offset 1196032 :unify-map \"CNS-5\" :dimension 2 :base chinese-cns11643-5 preferred-coding-system iso-2022-cn)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-5)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-6 :docstring \"CNS11643 Plane 6 Chinese Traditional: ISO-IR-186\" :short-name \"CNS11643-6\" :long-name \"CNS11643-6 (Chinese traditional): ISO-IR-186\" :iso-final-char 76 :emacs-mule-id 249 :code-space [33 126 33 126] :code-offset 1212416 :unify-map \"CNS-6\" :dimension 2 :base chinese-cns11643-6 preferred-coding-system iso-2022-cn)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-6)", expect);
}

#[test]
fn div_cs_plist_chinese_cns11643_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-cns11643-7 :docstring \"CNS11643 Plane 7 Chinese Traditional: ISO-IR-187\" :short-name \"CNS11643-7\" :long-name \"CNS11643-7 (Chinese traditional): ISO-IR-187\" :iso-final-char 77 :emacs-mule-id 250 :code-space [33 126 33 126] :code-offset 1228800 :unify-map \"CNS-7\" :dimension 2 :base chinese-cns11643-7 preferred-coding-system iso-2022-cn)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-cns11643-7)", expect);
}

#[test]
fn div_cs_plist_chinese_gb2312() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-gb2312 :docstring \"GB2312 Chinese simplified: ISO-IR-58\" :short-name \"GB2312\" :long-name \"GB2312: ISO-IR-58\" :iso-final-char 65 :emacs-mule-id 145 :code-space [33 126 33 126] :code-offset 1114112 :unify-map \"GB2312\" :dimension 2 :base chinese-gb2312 preferred-coding-system chinese-iso-8bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-gb2312)", expect);
}

#[test]
fn div_cs_plist_chinese_gbk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-gbk :docstring \"GBK Chinese simplified.\" :short-name \"GBK\" :code-space [64 254 129 254] :code-offset 1441792 :unify-map \"GBK\" :dimension 2 :long-name \"GBK\" :base chinese-gbk preferred-coding-system chinese-gbk)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-gbk)", expect);
}

#[test]
fn div_cs_plist_chinese_sisheng() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-sisheng :docstring \"SiSheng characters for PinYin/ZhuYin\" :short-name \"SiSheng\" :long-name \"SiSheng (PinYin/ZhuYin)\" :iso-final-char 48 :emacs-mule-id 160 :code-space [33 126] :unify-map \"MULE-sisheng\" :supplementary-p t :code-offset 2097152 :dimension 1 :base chinese-sisheng preferred-coding-system iso-2022-7bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'chinese-sisheng)", expect);
}

#[test]
fn div_cs_plist_control_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name control-1 :docstring \"8-bit control code (0x80..0x9F)\" :short-name \"8-bit control code\" :code-space [128 159] :code-offset 128 :dimension 1 :long-name \"8-bit control code\" :base control-1)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'control-1)", expect);
}

#[test]
fn div_cs_plist_cp00858() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp858 :docstring \"CP858 (Multilingual Latin I + Euro)\" :short-name \"CP858\" :code-space [0 255] :ascii-compatible-p t :map \"CP858\" :dimension 1 :long-name \"CP858\" :base cp858)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp00858)", expect);
}

#[test]
fn div_cs_plist_cp038() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm038 :docstring \"International version of EBCDIC\" :short-name \"IBM038\" :code-space [0 255] :mime-charset ibm038 :map \"IBM038\" :dimension 1 :long-name \"IBM038\" :base ibm038)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp038)", expect);
}

#[test]
fn div_cs_plist_cp1047() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm1047 :docstring \"IBM1047, `EBCDIC Latin 1/Open Systems' used by OS/390 Unix.\" :short-name \"IBM1047\" :code-space [0 255] :mime-charset ibm1047 :map \"IBM1047\" :dimension 1 :long-name \"IBM1047\" :base ibm1047)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1047)", expect);
}

#[test]
fn div_cs_plist_cp1125() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp1125 :docstring \"CP1125\" :short-name \"CP1125\" :code-space [0 255] :ascii-compatible-p t :map \"CP1125\" :dimension 1 :long-name \"CP1125\" :base cp1125)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1125)", expect);
}

#[test]
fn div_cs_plist_cp1250() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1250 :docstring \"WINDOWS-1250 (Central Europe)\" :short-name \"WINDOWS-1250\" :ascii-compatible-p t :code-space [0 255] :map \"CP1250\" :dimension 1 :long-name \"WINDOWS-1250\" :base windows-1250)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1250)", expect);
}

#[test]
fn div_cs_plist_cp1251() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1251 :docstring \"WINDOWS-1251 (Cyrillic)\" :short-name \"WINDOWS-1251\" :ascii-compatible-p t :code-space [0 255] :map \"CP1251\" :dimension 1 :long-name \"WINDOWS-1251\" :base windows-1251)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1251)", expect);
}

#[test]
fn div_cs_plist_cp1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1252 :docstring \"WINDOWS-1252 (Latin I)\" :short-name \"WINDOWS-1252\" :ascii-compatible-p t :code-space [0 255] :map \"CP1252\" :dimension 1 :long-name \"WINDOWS-1252\" :base windows-1252)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1252)", expect);
}

#[test]
fn div_cs_plist_cp1253() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1253 :docstring \"WINDOWS-1253 (Greek)\" :short-name \"WINDOWS-1253\" :ascii-compatible-p t :code-space [0 255] :map \"CP1253\" :dimension 1 :long-name \"WINDOWS-1253\" :base windows-1253)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1253)", expect);
}

#[test]
fn div_cs_plist_cp1254() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1254 :docstring \"WINDOWS-1254 (Turkish)\" :short-name \"WINDOWS-1254\" :ascii-compatible-p t :code-space [0 255] :map \"CP1254\" :dimension 1 :long-name \"WINDOWS-1254\" :base windows-1254)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1254)", expect);
}

#[test]
fn div_cs_plist_cp1255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1255 :docstring \"WINDOWS-1255 (Hebrew)\" :short-name \"WINDOWS-1255\" :ascii-compatible-p t :code-space [0 255] :map \"CP1255\" :dimension 1 :long-name \"WINDOWS-1255\" :base windows-1255)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1255)", expect);
}

#[test]
fn div_cs_plist_cp1256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1256 :docstring \"WINDOWS-1256 (Arabic)\" :short-name \"WINDOWS-1256\" :ascii-compatible-p t :code-space [0 255] :map \"CP1256\" :dimension 1 :long-name \"WINDOWS-1256\" :base windows-1256)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1256)", expect);
}

#[test]
fn div_cs_plist_cp1257() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1257 :docstring \"WINDOWS-1257 (Baltic)\" :short-name \"WINDOWS-1257\" :ascii-compatible-p t :code-space [0 255] :map \"CP1257\" :dimension 1 :long-name \"WINDOWS-1257\" :base windows-1257)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1257)", expect);
}

#[test]
fn div_cs_plist_cp1258() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1258 :docstring \"WINDOWS-1258 (Viet Nam)\" :short-name \"WINDOWS-1258\" :ascii-compatible-p t :code-space [0 255] :map \"CP1258\" :dimension 1 :long-name \"WINDOWS-1258\" :base windows-1258)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp1258)", expect);
}

#[test]
fn div_cs_plist_cp154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ptcp154 :docstring \"ParaType codepage (Asian Cyrillic)\" :short-name \"PT154\" :ascii-compatible-p t :code-space [0 255] :mime-charset pt154 :map \"PTCP154\" :dimension 1 :long-name \"PT154\" :base ptcp154)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp154)", expect);
}

#[test]
fn div_cs_plist_cp437() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp437 :docstring \"CP437 (MS-DOS United States, Australia, New Zealand, South Africa)\" :short-name \"CP437\" :code-space [0 255] :ascii-compatible-p t :map \"IBM437\" :dimension 1 :long-name \"CP437\" :base cp437)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp437)", expect);
}

#[test]
fn div_cs_plist_cp720() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp720 :docstring \"CP720 (Arabic)\" :short-name \"CP720\" :code-space [0 255] :ascii-compatible-p t :map \"CP720\" :dimension 1 :long-name \"CP720\" :base cp720)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp720)", expect);
}

#[test]
fn div_cs_plist_cp737() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp737 :docstring \"CP737 (PC Greek)\" :short-name \"CP737\" :code-space [0 255] :ascii-compatible-p t :map \"CP737\" :dimension 1 :long-name \"CP737\" :base cp737)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp737)", expect);
}

#[test]
fn div_cs_plist_cp775() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp775 :docstring \"CP775 (PC Baltic)\" :short-name \"CP775\" :code-space [0 255] :ascii-compatible-p t :map \"CP775\" :dimension 1 :long-name \"CP775\" :base cp775)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp775)", expect);
}

#[test]
fn div_cs_plist_cp850() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm850 :docstring \"DOS codepage 850 (Latin-1)\" :short-name \"IBM850\" :ascii-compatible-p t :code-space [0 255] :map \"IBM850\" :dimension 1 :long-name \"IBM850\" :base ibm850)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp850)", expect);
}

#[test]
fn div_cs_plist_cp851() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp851 :docstring \"CP851 (Greek)\" :short-name \"CP851\" :code-space [0 255] :ascii-compatible-p t :map \"IBM851\" :dimension 1 :long-name \"CP851\" :base cp851)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp851)", expect);
}

#[test]
fn div_cs_plist_cp852() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp852 :docstring \"CP852 (MS-DOS Latin-2)\" :short-name \"CP852\" :code-space [0 255] :ascii-compatible-p t :map \"IBM852\" :dimension 1 :long-name \"CP852\" :base cp852)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp852)", expect);
}

#[test]
fn div_cs_plist_cp855() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp855 :docstring \"CP855 (IBM Cyrillic)\" :short-name \"CP855\" :code-space [0 255] :ascii-compatible-p t :map \"IBM855\" :dimension 1 :long-name \"CP855\" :base cp855)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp855)", expect);
}

#[test]
fn div_cs_plist_cp857() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp857 :docstring \"CP857 (IBM Turkish)\" :short-name \"CP857\" :code-space [0 255] :ascii-compatible-p t :map \"IBM857\" :dimension 1 :long-name \"CP857\" :base cp857)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp857)", expect);
}

#[test]
fn div_cs_plist_cp858() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp858 :docstring \"CP858 (Multilingual Latin I + Euro)\" :short-name \"CP858\" :code-space [0 255] :ascii-compatible-p t :map \"CP858\" :dimension 1 :long-name \"CP858\" :base cp858)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp858)", expect);
}

#[test]
fn div_cs_plist_cp860() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp860 :docstring \"CP860 (MS-DOS Portuguese)\" :short-name \"CP860\" :code-space [0 255] :ascii-compatible-p t :map \"IBM860\" :dimension 1 :long-name \"CP860\" :base cp860)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp860)", expect);
}

#[test]
fn div_cs_plist_cp861() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp861 :docstring \"CP861 (MS-DOS Icelandic)\" :short-name \"CP861\" :code-space [0 255] :ascii-compatible-p t :map \"IBM861\" :dimension 1 :long-name \"CP861\" :base cp861)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp861)", expect);
}

#[test]
fn div_cs_plist_cp862() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp862 :docstring \"CP862 (PC Hebrew)\" :short-name \"CP862\" :code-space [0 255] :ascii-compatible-p t :map \"IBM862\" :dimension 1 :long-name \"CP862\" :base cp862)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp862)", expect);
}

#[test]
fn div_cs_plist_cp863() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp863 :docstring \"CP863 (MS-DOS Canadian French)\" :short-name \"CP863\" :code-space [0 255] :ascii-compatible-p t :map \"IBM863\" :dimension 1 :long-name \"CP863\" :base cp863)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp863)", expect);
}

#[test]
fn div_cs_plist_cp864() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp864 :docstring \"CP864 (PC Arabic)\" :short-name \"CP864\" :code-space [0 255] :ascii-compatible-p t :map \"IBM864\" :dimension 1 :long-name \"CP864\" :base cp864)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp864)", expect);
}

#[test]
fn div_cs_plist_cp865() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp865 :docstring \"CP865 (MS-DOS Nordic)\" :short-name \"CP865\" :code-space [0 255] :ascii-compatible-p t :map \"IBM865\" :dimension 1 :long-name \"CP865\" :base cp865)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp865)", expect);
}

#[test]
fn div_cs_plist_cp866() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp866 :docstring \"CP866\" :short-name \"cp866\" :ascii-compatible-p t :code-space [0 255] :map \"IBM866\" :dimension 1 :long-name \"cp866\" :base cp866)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp866)", expect);
}

#[test]
fn div_cs_plist_cp866u() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp1125 :docstring \"CP1125\" :short-name \"CP1125\" :code-space [0 255] :ascii-compatible-p t :map \"CP1125\" :dimension 1 :long-name \"CP1125\" :base cp1125)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp866u)", expect);
}

#[test]
fn div_cs_plist_cp869() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp869 :docstring \"CP869 (IBM Modern Greek)\" :short-name \"CP869\" :code-space [0 255] :ascii-compatible-p t :map \"IBM869\" :dimension 1 :long-name \"CP869\" :base cp869)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp869)", expect);
}

#[test]
fn div_cs_plist_cp874() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp874 :docstring \"CP874 (IBM Thai)\" :short-name \"CP874\" :code-space [0 255] :ascii-compatible-p t :map \"IBM874\" :dimension 1 :long-name \"CP874\" :base cp874)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp874)", expect);
}

#[test]
fn div_cs_plist_cp932() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp932 :docstring \"CP932 (Microsoft shift-jis)\" :code-space [0 255 0 254] :short-name \"CP932\" :superset (ascii katakana-sjis cp932-2-byte) :dimension 2 :long-name \"CP932\" :base cp932)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp932)", expect);
}

#[test]
fn div_cs_plist_cp932_2_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp932-2-byte :docstring \"2-byte part of CP932\" :dimension 2 :map \"CP932-2BYTE\" :code-space [64 252 129 252] :supplementary-p t :short-name \"cp932-2-byte\" :long-name \"cp932-2-byte\" :base cp932-2-byte)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp932-2-byte)", expect);
}

#[test]
fn div_cs_plist_cp936() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-gbk :docstring \"GBK Chinese simplified.\" :short-name \"GBK\" :code-space [64 254 129 254] :code-offset 1441792 :unify-map \"GBK\" :dimension 2 :long-name \"GBK\" :base chinese-gbk preferred-coding-system chinese-gbk)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp936)", expect);
}

#[test]
fn div_cs_plist_cp949() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp949 :docstring \"CP949 (Korean)\" :short-name \"CP949\" :long-name \"CP949 (Korean)\" :code-space [0 254 0 253] :superset (ascii cp949-2-byte) :dimension 2 :base cp949)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp949)", expect);
}

#[test]
fn div_cs_plist_cp949_2_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp949-2-byte :docstring \"2-byte part of CP949\" :dimension 2 :map \"CP949-2BYTE\" :code-space [65 254 129 253] :supplementary-p t :short-name \"cp949-2-byte\" :long-name \"cp949-2-byte\" :base cp949-2-byte)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cp949-2-byte)", expect);
}

#[test]
fn div_cs_plist_cyrillic_iso8859_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cyrillic-iso8859-5 :docstring \"Right-Hand Part of ISO/IEC 8859/5 (Latin/Cyrillic): ISO-IR-144\" :short-name \"RHP of ISO/IEC 8859/5\" :long-name \"RHP of ISO/IEC 8859/5 (Latin/Cyrillic)\" :iso-final-char 76 :emacs-mule-id 140 :code-space [32 127] :subset (iso-8859-5 160 255 -128) :dimension 1 :base cyrillic-iso8859-5 preferred-coding-system cyrillic-iso-8bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'cyrillic-iso8859-5)", expect);
}

#[test]
fn div_cs_plist_devanagari_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name devanagari-akruti :docstring \"Glyphs of Devanagari script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Devanagari glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1575936 :dimension 1 :long-name \"AKRUTI Devanagari glyphs\" :base devanagari-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'devanagari-akruti)", expect);
}

#[test]
fn div_cs_plist_devanagari_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name devanagari-cdac :docstring \"Glyphs of Devanagari script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Devanagari glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1573120 :dimension 1 :long-name \"CDAC Devanagari glyphs\" :base devanagari-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'devanagari-cdac)", expect);
}

#[test]
fn div_cs_plist_ebcdic_int() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm038 :docstring \"International version of EBCDIC\" :short-name \"IBM038\" :code-space [0 255] :mime-charset ibm038 :map \"IBM038\" :dimension 1 :long-name \"IBM038\" :base ibm038)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ebcdic-int)", expect);
}

#[test]
fn div_cs_plist_ebcdic_uk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ebcdic-uk :docstring \"UK version of EBCDIC\" :short-name \"EBCDIC-UK\" :code-space [0 255] :mime-charset ebcdic-uk :map \"EBCDICUK\" :dimension 1 :long-name \"EBCDIC-UK\" :base ebcdic-uk)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ebcdic-uk)", expect);
}

#[test]
fn div_cs_plist_ebcdic_us() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ebcdic-us :docstring \"US version of EBCDIC\" :short-name \"EBCDIC-US\" :code-space [0 255] :mime-charset ebcdic-us :map \"EBCDICUS\" :dimension 1 :long-name \"EBCDIC-US\" :base ebcdic-us)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ebcdic-us)", expect);
}

#[test]
fn div_cs_plist_eight_bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name eight-bit :dimension 1 :code-space [128 255 0 0 0 0 0 0] :iso-final-char nil :emacs-mule-id nil :ascii-compatible-p nil :code-offset 4194176 :docstring \"Raw bytes 128-255\" :short-name \"Raw bytes\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'eight-bit)", expect);
}

#[test]
fn div_cs_plist_eight_bit_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name eight-bit-control :docstring \"Raw bytes in the range 0x80..0x9F (usually produced from invalid encodings)\" :short-name \"Raw bytes 0x80..0x9F\" :supplementary-p t :code-space [128 159] :code-offset 4194176 :dimension 1 :long-name \"Raw bytes 0x80..0x9F\" :base eight-bit-control)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'eight-bit-control)", expect);
}

#[test]
fn div_cs_plist_eight_bit_graphic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name eight-bit-graphic :docstring \"Raw bytes in the range 0xA0..0xFF (usually produced from invalid encodings)\" :short-name \"Raw bytes 0xA0..0xFF\" :supplementary-p t :code-space [160 255] :code-offset 4194208 :dimension 1 :long-name \"Raw bytes 0xA0..0xFF\" :base eight-bit-graphic)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'eight-bit-graphic)", expect);
}

#[test]
fn div_cs_plist_emacs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name emacs :dimension 3 :code-space [0 255 0 255 0 63 0 0] :iso-final-char nil :emacs-mule-id nil :ascii-compatible-p t :code-offset 0 :docstring \"Full Emacs charset (excluding eight bit chars)\" :short-name \"Emacs\" :long-name \"Emacs\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'emacs)", expect);
}

#[test]
fn div_cs_plist_ethiopic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ethiopic :docstring \"Ethiopic characters for Amharic and Tigrigna.\" :short-name \"Ethiopic\" :long-name \"Ethiopic characters\" :iso-final-char 51 :emacs-mule-id 245 :supplementary-p t :unify-map \"MULE-ethiopic\" :code-space [33 126 33 126] :code-offset 1703936 :dimension 2 :base ethiopic preferred-coding-system iso-2022-7bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ethiopic)", expect);
}

#[test]
fn div_cs_plist_gb18030() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gb18030 :docstring \"GB18030\" :code-space [0 255 0 254 0 254 0 254] :min-code 0 :max-code (65081 . 65081) :superset (ascii gb18030-2-byte gb18030-4-byte-bmp gb18030-4-byte-smp gb18030-4-byte-ext-1 gb18030-4-byte-ext-2) :dimension 4 :short-name \"gb18030\" :long-name \"gb18030\" :base gb18030)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gb18030)", expect);
}

#[test]
fn div_cs_plist_gb18030_2_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gb18030-2-byte :docstring \"GB18030 2-byte (0x814E..0xFEFE)\" :code-space [64 254 129 254] :supplementary-p t :map \"GB180302\" :dimension 2 :short-name \"gb18030-2-byte\" :long-name \"gb18030-2-byte\" :base gb18030-2-byte preferred-coding-system chinese-gb18030)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gb18030-2-byte)", expect);
}

#[test]
fn div_cs_plist_gb18030_4_byte_bmp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gb18030-4-byte-bmp :docstring \"GB18030 4-byte for BMP (0x81308130-0x8431A439)\" :code-space [48 57 129 254 48 57 129 132] :supplementary-p t :map \"GB180304\" :dimension 4 :short-name \"gb18030-4-byte-bmp\" :long-name \"gb18030-4-byte-bmp\" :base gb18030-4-byte-bmp preferred-coding-system chinese-gb18030)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gb18030-4-byte-bmp)", expect);
}

#[test]
fn div_cs_plist_gb18030_4_byte_ext_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gb18030-4-byte-ext-1 :docstring \"GB18030 4-byte (0x8431A530-0x8F39FE39)\" :code-space [48 57 129 254 48 57 132 143] :min-code (33841 . 42288) :max-code (36665 . 65081) :supplementary-p t :code-offset 2097152 :dimension 4 :short-name \"gb18030-4-byte-ext-1\" :long-name \"gb18030-4-byte-ext-1\" :base gb18030-4-byte-ext-1 preferred-coding-system chinese-gb18030)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gb18030-4-byte-ext-1)", expect);
}

#[test]
fn div_cs_plist_gb18030_4_byte_ext_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gb18030-4-byte-ext-2 :docstring \"GB18030 4-byte (0xE3329A36-0xFE39FE39)\" :code-space [48 57 129 254 48 57 227 254] :min-code (58162 . 39478) :max-code (65081 . 65081) :supplementary-p t :code-offset 2246732 :dimension 4 :short-name \"gb18030-4-byte-ext-2\" :long-name \"gb18030-4-byte-ext-2\" :base gb18030-4-byte-ext-2 preferred-coding-system chinese-gb18030)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gb18030-4-byte-ext-2)", expect);
}

#[test]
fn div_cs_plist_gb18030_4_byte_smp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gb18030-4-byte-smp :docstring \"GB18030 4-byte for SMP (0x90308130-0xE3329A35)\" :code-space [48 57 129 254 48 57 144 227] :min-code (36912 . 33072) :max-code (58162 . 39477) :supplementary-p t :code-offset 65536 :dimension 4 :short-name \"gb18030-4-byte-smp\" :long-name \"gb18030-4-byte-smp\" :base gb18030-4-byte-smp preferred-coding-system chinese-gb18030)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gb18030-4-byte-smp)", expect);
}

#[test]
fn div_cs_plist_georgian_academy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name georgian-academy :docstring \"GEORGIAN-ACADEMY\" :short-name \"GEORGIAN-ACADEMY\" :ascii-compatible-p t :code-space [0 255] :map \"KA-ACADEMY\" :dimension 1 :long-name \"GEORGIAN-ACADEMY\" :base georgian-academy)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'georgian-academy)", expect);
}

#[test]
fn div_cs_plist_georgian_ps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name georgian-ps :docstring \"GEORGIAN-PS\" :short-name \"GEORGIAN-PS\" :ascii-compatible-p t :code-space [0 255] :map \"KA-PS\" :dimension 1 :long-name \"GEORGIAN-PS\" :base georgian-ps)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'georgian-ps)", expect);
}

#[test]
fn div_cs_plist_greek_iso8859_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name greek-iso8859-7 :docstring \"Right-Hand Part of ISO/IEC 8859/7 (Latin/Greek): ISO-IR-126\" :short-name \"RHP of ISO/IEC 8859/7\" :long-name \"RHP of ISO/IEC 8859/7 (Latin/Greek)\" :iso-final-char 70 :emacs-mule-id 134 :code-space [32 127] :subset (iso-8859-7 160 255 -128) :dimension 1 :base greek-iso8859-7 preferred-coding-system greek-iso-8bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'greek-iso8859-7)", expect);
}

#[test]
fn div_cs_plist_gujarati_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gujarati-akruti :docstring \"Glyphs of Gujarati script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Gujarati glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1576704 :dimension 1 :long-name \"AKRUTI Gujarati glyphs\" :base gujarati-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gujarati-akruti)", expect);
}

#[test]
fn div_cs_plist_gujarati_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name gujarati-cdac :docstring \"Glyphs of Gujarati script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Gujarati glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1575424 :dimension 1 :long-name \"CDAC Gujarati glyphs\" :base gujarati-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'gujarati-cdac)", expect);
}

#[test]
fn div_cs_plist_hebrew_iso8859_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name hebrew-iso8859-8 :docstring \"Right-Hand Part of ISO/IEC 8859/8 (Latin/Hebrew): ISO-IR-138\" :short-name \"RHP of ISO/IEC 8859/8\" :long-name \"RHP of ISO/IEC 8859/8 (Latin/Hebrew)\" :iso-final-char 72 :emacs-mule-id 136 :code-space [32 127] :subset (iso-8859-8 160 255 -128) :dimension 1 :base hebrew-iso8859-8 preferred-coding-system hebrew-iso-8bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'hebrew-iso8859-8)", expect);
}

#[test]
fn div_cs_plist_hp_roman8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name hp-roman8 :docstring \"Encoding used by Hewlet-Packard printer software\" :short-name \"HP-ROMAN8\" :ascii-compatible-p t :code-space [0 255] :map \"HP-ROMAN8\" :dimension 1 :long-name \"HP-ROMAN8\" :base hp-roman8)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'hp-roman8)", expect);
}

#[test]
fn div_cs_plist_ibm038() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm038 :docstring \"International version of EBCDIC\" :short-name \"IBM038\" :code-space [0 255] :mime-charset ibm038 :map \"IBM038\" :dimension 1 :long-name \"IBM038\" :base ibm038)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm038)", expect);
}

#[test]
fn div_cs_plist_ibm1047() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm1047 :docstring \"IBM1047, `EBCDIC Latin 1/Open Systems' used by OS/390 Unix.\" :short-name \"IBM1047\" :code-space [0 255] :mime-charset ibm1047 :map \"IBM1047\" :dimension 1 :long-name \"IBM1047\" :base ibm1047)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm1047)", expect);
}

#[test]
fn div_cs_plist_ibm256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm256 :docstring \"Netherlands version of EBCDIC\" :short-name \"IBM256\" :code-space [0 255] :mime-charset ibm256 :map \"IBM256\" :dimension 1 :long-name \"IBM256\" :base ibm256)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm256)", expect);
}

#[test]
fn div_cs_plist_ibm273() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm273 :docstring \"Austrian / German version of EBCDIC\" :short-name \"IBM273\" :code-space [0 255] :mime-charset ibm273 :map \"IBM273\" :dimension 1 :long-name \"IBM273\" :base ibm273)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm273)", expect);
}

#[test]
fn div_cs_plist_ibm274() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm274 :docstring \"Belgian version of EBCDIC\" :short-name \"IBM274\" :code-space [0 255] :mime-charset ibm274 :map \"IBM274\" :dimension 1 :long-name \"IBM274\" :base ibm274)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm274)", expect);
}

#[test]
fn div_cs_plist_ibm275() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm275 :docstring \"Brazilian version of EBCDIC\" :short-name \"IBM275\" :code-space [0 255] :mime-charset ibm275 :map \"IBM275\" :dimension 1 :long-name \"IBM275\" :base ibm275)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm275)", expect);
}

#[test]
fn div_cs_plist_ibm277() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm277 :docstring \"Danish / Norwegian version of EBCDIC\" :short-name \"IBM277\" :code-space [0 255] :mime-charset ibm277 :map \"IBM277\" :dimension 1 :long-name \"IBM277\" :base ibm277)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm277)", expect);
}

#[test]
fn div_cs_plist_ibm278() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm278 :docstring \"Finnish / Swedish version of EBCDIC\" :short-name \"IBM278\" :code-space [0 255] :mime-charset ibm278 :map \"IBM278\" :dimension 1 :long-name \"IBM278\" :base ibm278)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm278)", expect);
}

#[test]
fn div_cs_plist_ibm280() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm280 :docstring \"Italian version of EBCDIC\" :short-name \"IBM280\" :code-space [0 255] :mime-charset ibm270 :map \"IBM280\" :dimension 1 :long-name \"IBM280\" :base ibm280)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm280)", expect);
}

#[test]
fn div_cs_plist_ibm281() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm281 :docstring \"Japanese-E version of EBCDIC\" :short-name \"IBM281\" :code-space [0 255] :mime-charset ibm281 :map \"IBM281\" :dimension 1 :long-name \"IBM281\" :base ibm281)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm281)", expect);
}

#[test]
fn div_cs_plist_ibm284() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm284 :docstring \"Spanish version of EBCDIC\" :short-name \"IBM284\" :code-space [0 255] :mime-charset ibm284 :map \"IBM284\" :dimension 1 :long-name \"IBM284\" :base ibm284)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm284)", expect);
}

#[test]
fn div_cs_plist_ibm285() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm285 :docstring \"UK english version of EBCDIC\" :short-name \"IBM285\" :code-space [0 255] :mime-charset ibm285 :map \"IBM285\" :dimension 1 :long-name \"IBM285\" :base ibm285)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm285)", expect);
}

#[test]
fn div_cs_plist_ibm290() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm290 :docstring \"Japanese katakana version of EBCDIC\" :short-name \"IBM290\" :code-space [0 255] :mime-charset ibm290 :map \"IBM290\" :dimension 1 :long-name \"IBM290\" :base ibm290)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm290)", expect);
}

#[test]
fn div_cs_plist_ibm297() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm297 :docstring \"French version of EBCDIC\" :short-name \"IBM297\" :code-space [0 255] :mime-charset ibm297 :map \"IBM297\" :dimension 1 :long-name \"IBM297\" :base ibm297)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm297)", expect);
}

#[test]
fn div_cs_plist_ibm850() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ibm850 :docstring \"DOS codepage 850 (Latin-1)\" :short-name \"IBM850\" :ascii-compatible-p t :code-space [0 255] :map \"IBM850\" :dimension 1 :long-name \"IBM850\" :base ibm850)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm850)", expect);
}

#[test]
fn div_cs_plist_ibm866() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp866 :docstring \"CP866\" :short-name \"cp866\" :ascii-compatible-p t :code-space [0 255] :map \"IBM866\" :dimension 1 :long-name \"cp866\" :base cp866)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ibm866)", expect);
}

#[test]
fn div_cs_plist_indian_1_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name indian-1-column :docstring \"Indian charset for 1-column width glyphs.\" :short-name \"Indian 1-col\" :long-name \"Indian 1 Column\" :iso-final-char 54 :emacs-mule-id 251 :supplementary-p t :code-space [33 126 33 126] :code-offset 1589248 :dimension 2 :base indian-1-column)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'indian-1-column)", expect);
}

#[test]
fn div_cs_plist_indian_2_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name indian-2-column :docstring \"Indian charset for 2-column width glyphs.\" :short-name \"Indian 2-col\" :long-name \"Indian 2 Column\" :iso-final-char 53 :emacs-mule-id 251 :supplementary-p t :code-space [33 126 33 126] :code-offset 1589248 :dimension 2 :base indian-2-column preferred-coding-system devanagari)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'indian-2-column)", expect);
}

#[test]
fn div_cs_plist_indian_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name indian-glyph :docstring \"Glyphs for Indian characters.\" :short-name \"Indian glyph\" :iso-final-char 52 :emacs-mule-id 240 :supplementary-p t :code-space [32 127 32 127] :code-offset 1573120 :dimension 2 :long-name \"Indian glyph\" :base indian-glyph preferred-coding-system devanagari)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'indian-glyph)", expect);
}

#[test]
fn div_cs_plist_indian_is13194() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name indian-is13194 :docstring \"7-bit representation of IS 13194 (ISCII) for Devanagari\" :short-name \"IS 13194 (DEV)\" :long-name \"Indian IS 13194 (DEV)\" :iso-final-char 53 :emacs-mule-id 225 :supplementary-p t :code-space [33 126] :code-offset 1572864 :unify-map \"MULE-is13194\" :dimension 1 :base indian-is13194 preferred-coding-system devanagari)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'indian-is13194)", expect);
}

#[test]
fn div_cs_plist_ipa() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ipa :docstring \"IPA (International Phonetic Association)\" :short-name \"IPA\" :iso-final-char 48 :emacs-mule-id 161 :unify-map \"MULE-ipa\" :code-space [32 127] :supplementary-p t :code-offset 2097280 :dimension 1 :long-name \"IPA\" :base ipa preferred-coding-system iso-2022-7bit)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ipa)", expect);
}

#[test]
fn div_cs_plist_iso_8859_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-1 :dimension 1 :code-space [0 255 0 0 0 0 0 0] :iso-final-char nil :emacs-mule-id nil :ascii-compatible-p t :code-offset 0 :docstring \"Latin-1 (ISO/IEC 8859-1)\" :short-name \"Latin-1\" :long-name \"Latin-1\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-1)", expect);
}

#[test]
fn div_cs_plist_iso_8859_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-10 :docstring \"ISO/IEC 8859/10\" :short-name \"Latin-6\" :long-name \"ISO/IEC 8859/10\" :ascii-compatible-p t :code-space [0 255] :map \"8859-10\" :dimension 1 :base iso-8859-10)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-10)", expect);
}

#[test]
fn div_cs_plist_iso_8859_11() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-11 :docstring \"ISO/IEC 8859/11\" :short-name \"Latin/Thai\" :long-name \"ISO/IEC 8859/11\" :ascii-compatible-p t :code-space [0 255] :map \"8859-11\" :dimension 1 :base iso-8859-11)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-11)", expect);
}

#[test]
fn div_cs_plist_iso_8859_13() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-13 :docstring \"ISO/IEC 8859/13\" :short-name \"Latin-7\" :long-name \"ISO/IEC 8859/13\" :ascii-compatible-p t :code-space [0 255] :map \"8859-13\" :dimension 1 :base iso-8859-13)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-13)", expect);
}

#[test]
fn div_cs_plist_iso_8859_14() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-14 :docstring \"ISO/IEC 8859/14\" :short-name \"Latin-8\" :long-name \"ISO/IEC 8859/14\" :ascii-compatible-p t :code-space [0 255] :map \"8859-14\" :dimension 1 :base iso-8859-14)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-14)", expect);
}

#[test]
fn div_cs_plist_iso_8859_15() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-15 :docstring \"ISO/IEC 8859/15\" :short-name \"Latin-9\" :long-name \"ISO/IEC 8859/15\" :ascii-compatible-p t :code-space [0 255] :map \"8859-15\" :dimension 1 :base iso-8859-15)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-15)", expect);
}

#[test]
fn div_cs_plist_iso_8859_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-16 :docstring \"ISO/IEC 8859/16\" :short-name \"Latin-10\" :long-name \"ISO/IEC 8859/16\" :ascii-compatible-p t :code-space [0 255] :map \"8859-16\" :dimension 1 :base iso-8859-16)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-16)", expect);
}

#[test]
fn div_cs_plist_iso_8859_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-2 :docstring \"ISO/IEC 8859/2\" :short-name \"Latin-2\" :long-name \"ISO/IEC 8859/2\" :ascii-compatible-p t :code-space [0 255] :map \"8859-2\" :dimension 1 :base iso-8859-2)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-2)", expect);
}

#[test]
fn div_cs_plist_iso_8859_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-3 :docstring \"ISO/IEC 8859/3\" :short-name \"Latin-3\" :long-name \"ISO/IEC 8859/3\" :ascii-compatible-p t :code-space [0 255] :map \"8859-3\" :dimension 1 :base iso-8859-3)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-3)", expect);
}

#[test]
fn div_cs_plist_iso_8859_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-4 :docstring \"ISO/IEC 8859/4\" :short-name \"Latin-4\" :long-name \"ISO/IEC 8859/4\" :ascii-compatible-p t :code-space [0 255] :map \"8859-4\" :dimension 1 :base iso-8859-4)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-4)", expect);
}

#[test]
fn div_cs_plist_iso_8859_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-5 :docstring \"ISO/IEC 8859/5\" :short-name \"Latin/Cyrillic\" :long-name \"ISO/IEC 8859/5\" :ascii-compatible-p t :code-space [0 255] :map \"8859-5\" :dimension 1 :base iso-8859-5)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-5)", expect);
}

#[test]
fn div_cs_plist_iso_8859_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-6 :docstring \"ISO/IEC 8859/6\" :short-name \"Latin/Arabic\" :long-name \"ISO/IEC 8859/6\" :ascii-compatible-p t :code-space [0 255] :map \"8859-6\" :dimension 1 :base iso-8859-6)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-6)", expect);
}

#[test]
fn div_cs_plist_iso_8859_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-7 :docstring \"ISO/IEC 8859/7\" :short-name \"Latin/Greek\" :long-name \"ISO/IEC 8859/7\" :ascii-compatible-p t :code-space [0 255] :map \"8859-7\" :dimension 1 :base iso-8859-7)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-7)", expect);
}

#[test]
fn div_cs_plist_iso_8859_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-8 :docstring \"ISO/IEC 8859/8\" :short-name \"Latin/Hebrew\" :long-name \"ISO/IEC 8859/8\" :ascii-compatible-p t :code-space [0 255] :map \"8859-8\" :dimension 1 :base iso-8859-8)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-8)", expect);
}

#[test]
fn div_cs_plist_iso_8859_9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name iso-8859-9 :docstring \"ISO/IEC 8859/9\" :short-name \"Latin-5\" :long-name \"ISO/IEC 8859/9\" :ascii-compatible-p t :code-space [0 255] :map \"8859-9\" :dimension 1 :base iso-8859-9)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'iso-8859-9)", expect);
}

#[test]
fn div_cs_plist_japanese_jisx0208() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name japanese-jisx0208 :docstring \"JISX0208.1983/1990 Japanese Kanji: ISO-IR-87\" :short-name \"JISX0208\" :long-name \"JISX0208.1983/1990 (Japanese): ISO-IR-87\" :iso-final-char 66 :emacs-mule-id 146 :code-space [33 126 33 126] :code-offset 1310720 :unify-map \"JISX0208\" :dimension 2 :base japanese-jisx0208 preferred-coding-system iso-2022-jp)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'japanese-jisx0208)", expect);
}

#[test]
fn div_cs_plist_japanese_jisx0208_1978() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name japanese-jisx0208-1978 :docstring \"JISX0208.1978 Japanese Kanji (so called \\\"old JIS\\\"): ISO-IR-42\" :short-name \"JISX0208.1978\" :long-name \"JISX0208.1978 (JISC6226.1978): ISO-IR-42\" :iso-final-char 64 :emacs-mule-id 144 :code-space [33 126 33 126] :code-offset 1327104 :unify-map \"JISC6226\" :dimension 2 :base japanese-jisx0208-1978 preferred-coding-system iso-2022-jp)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'japanese-jisx0208-1978)", expect);
}

#[test]
fn div_cs_plist_japanese_jisx0212() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name japanese-jisx0212 :docstring \"JISX0212 Japanese supplement: ISO-IR-159\" :short-name \"JISX0212\" :long-name \"JISX0212 (Japanese): ISO-IR-159\" :iso-final-char 68 :emacs-mule-id 148 :code-space [33 126 33 126] :code-offset 1343488 :unify-map \"JISX0212\" :dimension 2 :base japanese-jisx0212 preferred-coding-system iso-2022-jp)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'japanese-jisx0212)", expect);
}

#[test]
fn div_cs_plist_japanese_jisx0213_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name japanese-jisx0213-1 :docstring \"JISX0213.2000 Plane 1 (Japanese)\" :short-name \"JISX0213-1\" :iso-final-char 79 :emacs-mule-id 151 :unify-map \"JISX2131\" :code-space [33 126 33 126] :code-offset 1359872 :dimension 2 :long-name \"JISX0213-1\" :base japanese-jisx0213-1)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'japanese-jisx0213-1)", expect);
}

#[test]
fn div_cs_plist_japanese_jisx0213_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name japanese-jisx0213-2 :docstring \"JISX0213.2000 Plane 2 (Japanese)\" :short-name \"JISX0213-2\" :iso-final-char 80 :emacs-mule-id 254 :unify-map \"JISX2132\" :code-space [33 126 33 126] :code-offset 1376256 :dimension 2 :long-name \"JISX0213-2\" :base japanese-jisx0213-2)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'japanese-jisx0213-2)", expect);
}

#[test]
fn div_cs_plist_japanese_jisx0213_a() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name japanese-jisx0213-a :docstring \"JISX0213.2004 adds these characters to JISX0213.2000.\" :short-name \"JISX0213A\" :dimension 2 :code-space [33 126 33 126] :supplementary-p t :map \"JISX213A\" :long-name \"JISX0213A\" :base japanese-jisx0213-a)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'japanese-jisx0213-a)", expect);
}

#[test]
fn div_cs_plist_japanese_jisx0213_2004_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name japanese-jisx0213.2004-1 :docstring \"JISX0213.2004 Plane1 (Japanese)\" :short-name \"JISX0213.2004-1\" :dimension 2 :code-space [33 126 33 126] :iso-final-char 81 :superset (japanese-jisx0213-a japanese-jisx0213-1) :long-name \"JISX0213.2004-1\" :base japanese-jisx0213.2004-1)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'japanese-jisx0213.2004-1)", expect);
}

#[test]
fn div_cs_plist_jisx0201() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name jisx0201 :docstring \"JISX0201\" :short-name \"JISX0201\" :code-space [0 223] :map \"JISX0201\" :dimension 1 :long-name \"JISX0201\" :base jisx0201)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'jisx0201)", expect);
}

#[test]
fn div_cs_plist_kannada_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name kannada-akruti :docstring \"Glyphs of Kannada script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Kannada glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1577728 :dimension 1 :long-name \"AKRUTI Kannada glyphs\" :base kannada-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'kannada-akruti)", expect);
}

#[test]
fn div_cs_plist_kannada_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name kannada-cdac :docstring \"Glyphs of Kannada script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Kannada glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1574912 :dimension 1 :long-name \"CDAC Kannada glyphs\" :base kannada-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'kannada-cdac)", expect);
}

#[test]
fn div_cs_plist_katakana_jisx0201() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name katakana-jisx0201 :docstring \"Katakana Part of JISX0201.1976\" :short-name \"JISX0201 Katakana\" :long-name \"Japanese Katakana (JISX0201.1976)\" :iso-final-char 73 :emacs-mule-id 137 :supplementary-p t :code-space [33 126] :subset (jisx0201 161 254 -128) :dimension 1 :base katakana-jisx0201 preferred-coding-system japanese-shift-jis)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'katakana-jisx0201)", expect);
}

#[test]
fn div_cs_plist_katakana_sjis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name katakana-sjis :docstring \"Katakana part of Shift-JIS\" :dimension 1 :code-space [161 223] :subset (jisx0201 161 223 0) :supplementary-p t :short-name \"katakana-sjis\" :long-name \"katakana-sjis\" :base katakana-sjis)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'katakana-sjis)", expect);
}

#[test]
fn div_cs_plist_koi8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name koi8-r :docstring \"KOI8-R\" :short-name \"KOI8-R\" :ascii-compatible-p t :code-space [0 255] :map \"KOI8-R\" :dimension 1 :long-name \"KOI8-R\" :base koi8-r)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'koi8)", expect);
}

#[test]
fn div_cs_plist_koi8_r() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name koi8-r :docstring \"KOI8-R\" :short-name \"KOI8-R\" :ascii-compatible-p t :code-space [0 255] :map \"KOI8-R\" :dimension 1 :long-name \"KOI8-R\" :base koi8-r)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'koi8-r)", expect);
}

#[test]
fn div_cs_plist_koi8_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name koi8-t :docstring \"KOI8-T\" :short-name \"KOI8-T\" :ascii-compatible-p t :code-space [0 255] :map \"KOI8-T\" :dimension 1 :long-name \"KOI8-T\" :base koi8-t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'koi8-t)", expect);
}

#[test]
fn div_cs_plist_koi8_u() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name koi8-u :docstring \"KOI8-U\" :short-name \"KOI8-U\" :ascii-compatible-p t :code-space [0 255] :map \"KOI8-U\" :dimension 1 :long-name \"KOI8-U\" :base koi8-u)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'koi8-u)", expect);
}

#[test]
fn div_cs_plist_korean_ksc5601() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name korean-ksc5601 :docstring \"KSC5601 Korean Hangul and Hanja: ISO-IR-149\" :short-name \"KSC5601\" :long-name \"KSC5601 (Korean): ISO-IR-149\" :iso-final-char 67 :emacs-mule-id 147 :code-space [33 126 33 126] :code-offset 2596756 :unify-map \"KSC5601\" :dimension 2 :base korean-ksc5601 preferred-coding-system iso-2022-kr)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'korean-ksc5601)", expect);
}

#[test]
fn div_cs_plist_lao() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name lao :docstring \"Lao characters (ISO10646 0E81..0EDF)\" :short-name \"Lao\" :iso-final-char 49 :emacs-mule-id 167 :supplementary-p t :code-space [33 126] :code-offset 3713 :dimension 1 :long-name \"Lao\" :base lao preferred-coding-system lao)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'lao)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-1 :docstring \"Right-Hand Part of ISO/IEC 8859/1 (Latin-1): ISO-IR-100\" :short-name \"RHP of Latin-1\" :long-name \"RHP of ISO/IEC 8859/1 (Latin-1): ISO-IR-100\" :iso-final-char 65 :emacs-mule-id 129 :code-space [32 127] :code-offset 160 :dimension 1 :base latin-iso8859-1 preferred-coding-system iso-latin-1)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-1)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-10 :docstring \"Right-Hand Part of ISO/IEC 8859/10 (Latin-6): ISO-IR-157\" :short-name \"RHP of ISO/IEC 8859/10\" :long-name \"RHP of ISO/IEC 8859/10 (Latin-6)\" :iso-final-char 86 :emacs-mule-id nil :code-space [32 127] :subset (iso-8859-10 160 255 -128) :dimension 1 :base latin-iso8859-10)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-10)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_13() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-13 :docstring \"Right-Hand Part of ISO/IEC 8859/13 (Latin-7): ISO-IR-179\" :short-name \"RHP of ISO/IEC 8859/13\" :long-name \"RHP of ISO/IEC 8859/13 (Latin-7)\" :iso-final-char 89 :emacs-mule-id nil :code-space [32 127] :subset (iso-8859-13 160 255 -128) :dimension 1 :base latin-iso8859-13)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-13)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_14() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-14 :docstring \"Right-Hand Part of ISO/IEC 8859/14 (Latin-8): ISO-IR-199\" :short-name \"RHP of ISO/IEC 8859/14\" :long-name \"RHP of ISO/IEC 8859/14 (Latin-8)\" :iso-final-char 95 :emacs-mule-id 143 :code-space [32 127] :subset (iso-8859-14 160 255 -128) :dimension 1 :base latin-iso8859-14 preferred-coding-system iso-latin-8)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-14)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_15() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-15 :docstring \"Right-Hand Part of ISO/IEC 8859/15 (Latin-9): ISO-IR-203\" :short-name \"RHP of ISO/IEC 8859/15\" :long-name \"RHP of ISO/IEC 8859/15 (Latin-9)\" :iso-final-char 98 :emacs-mule-id 142 :code-space [32 127] :subset (iso-8859-15 160 255 -128) :dimension 1 :base latin-iso8859-15 preferred-coding-system iso-latin-9)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-15)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-16 :docstring \"Right-Hand Part of ISO/IEC 8859/16 (Latin-10): ISO-IR-226\" :short-name \"RHP of ISO/IEC 8859/16\" :long-name \"RHP of ISO/IEC 8859/16 (Latin-10)\" :iso-final-char 102 :emacs-mule-id nil :code-space [32 127] :subset (iso-8859-16 160 255 -128) :dimension 1 :base latin-iso8859-16)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-16)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-2 :docstring \"Right-Hand Part of ISO/IEC 8859/2 (Latin-2): ISO-IR-101\" :short-name \"RHP of ISO/IEC 8859/2\" :long-name \"RHP of ISO/IEC 8859/2 (Latin-2)\" :iso-final-char 66 :emacs-mule-id 130 :code-space [32 127] :subset (iso-8859-2 160 255 -128) :dimension 1 :base latin-iso8859-2 preferred-coding-system iso-latin-2)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-2)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-3 :docstring \"Right-Hand Part of ISO/IEC 8859/3 (Latin-3): ISO-IR-109\" :short-name \"RHP of ISO/IEC 8859/3\" :long-name \"RHP of ISO/IEC 8859/3 (Latin-3)\" :iso-final-char 67 :emacs-mule-id 131 :code-space [32 127] :subset (iso-8859-3 160 255 -128) :dimension 1 :base latin-iso8859-3 preferred-coding-system iso-latin-3)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-3)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-4 :docstring \"Right-Hand Part of ISO/IEC 8859/4 (Latin-4): ISO-IR-110\" :short-name \"RHP of ISO/IEC 8859/4\" :long-name \"RHP of ISO/IEC 8859/4 (Latin-4)\" :iso-final-char 68 :emacs-mule-id 132 :code-space [32 127] :subset (iso-8859-4 160 255 -128) :dimension 1 :base latin-iso8859-4 preferred-coding-system iso-latin-4)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-4)", expect);
}

#[test]
fn div_cs_plist_latin_iso8859_9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-iso8859-9 :docstring \"Right-Hand Part of ISO/IEC 8859/9 (Latin-5): ISO-IR-148\" :short-name \"RHP of ISO/IEC 8859/9\" :long-name \"RHP of ISO/IEC 8859/9 (Latin-5)\" :iso-final-char 77 :emacs-mule-id 141 :code-space [32 127] :subset (iso-8859-9 160 255 -128) :dimension 1 :base latin-iso8859-9 preferred-coding-system iso-latin-5)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-iso8859-9)", expect);
}

#[test]
fn div_cs_plist_latin_jisx0201() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name latin-jisx0201 :docstring \"Roman Part of JISX0201.1976\" :short-name \"JISX0201 Roman\" :long-name \"Japanese Roman (JISX0201.1976)\" :iso-final-char 74 :emacs-mule-id 138 :supplementary-p t :code-space [33 126] :subset (jisx0201 33 126 0) :dimension 1 :base latin-jisx0201 preferred-coding-system japanese-shift-jis)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'latin-jisx0201)", expect);
}

#[test]
fn div_cs_plist_mac_roman() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name mac-roman :docstring \"Mac Roman charset\" :short-name \"Mac Roman\" :ascii-compatible-p t :code-space [0 255] :map \"MACINTOSH\" :dimension 1 :long-name \"Mac Roman\" :base mac-roman)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'mac-roman)", expect);
}

#[test]
fn div_cs_plist_malayalam_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name malayalam-akruti :docstring \"Glyphs of Malayalam script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Malayalam glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1577984 :dimension 1 :long-name \"AKRUTI Malayalam glyphs\" :base malayalam-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'malayalam-akruti)", expect);
}

#[test]
fn div_cs_plist_malayalam_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name malayalam-cdac :docstring \"Glyphs of Malayalam script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Malayalam glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1575168 :dimension 1 :long-name \"CDAC Malayalam glyphs\" :base malayalam-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'malayalam-cdac)", expect);
}

#[test]
fn div_cs_plist_mik() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name mik :docstring \"Bulgarian DOS codepage\" :short-name \"MIK\" :ascii-compatible-p t :code-space [0 255] :map \"MIK\" :dimension 1 :long-name \"MIK\" :base mik)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'mik)", expect);
}

#[test]
fn div_cs_plist_mule_lao() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name mule-lao :docstring \"Lao characters (ISO10646 0E81..0EDF)\" :short-name \"Lao\" :code-space [0 255] :supplementary-p t :superset (ascii eight-bit-control (lao . 128)) :dimension 1 :long-name \"Lao\" :base mule-lao)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'mule-lao)", expect);
}

#[test]
fn div_cs_plist_mule_unicode_0100_24ff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name mule-unicode-0100-24ff :docstring \"Unicode characters of the range U+0100..U+24FF.\" :short-name \"Unicode subset\" :long-name \"Unicode subset (U+0100..U+24FF)\" :iso-final-char 49 :emacs-mule-id 244 :supplementary-p t :code-space [32 127 32 127] :code-offset 256 :dimension 2 :base mule-unicode-0100-24ff)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'mule-unicode-0100-24ff)", expect);
}

#[test]
fn div_cs_plist_mule_unicode_2500_33ff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name mule-unicode-2500-33ff :docstring \"Unicode characters of the range U+2500..U+33FF.\" :short-name \"Unicode subset 2\" :long-name \"Unicode subset (U+2500..U+33FF)\" :iso-final-char 50 :emacs-mule-id 242 :supplementary-p t :code-space [32 127 32 71] :code-offset 9472 :dimension 2 :base mule-unicode-2500-33ff)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'mule-unicode-2500-33ff)", expect);
}

#[test]
fn div_cs_plist_mule_unicode_e000_ffff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name mule-unicode-e000-ffff :docstring \"Unicode characters of the range U+E000..U+FFFF.\" :short-name \"Unicode subset 3\" :long-name \"Unicode subset (U+E000+FFFF)\" :iso-final-char 51 :emacs-mule-id 243 :supplementary-p t :code-space [32 127 32 117] :code-offset 57344 :max-code 30015 :dimension 2 :base mule-unicode-e000-ffff)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'mule-unicode-e000-ffff)", expect);
}

#[test]
fn div_cs_plist_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name next :docstring \"NEXT\" :short-name \"NEXT\" :ascii-compatible-p t :code-space [0 255] :map \"NEXTSTEP\" :dimension 1 :long-name \"NEXT\" :base next)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'next)", expect);
}

#[test]
fn div_cs_plist_oriya_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name oriya-akruti :docstring \"Glyphs of Oriya script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Oriya glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1576960 :dimension 1 :long-name \"AKRUTI Oriya glyphs\" :base oriya-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'oriya-akruti)", expect);
}

#[test]
fn div_cs_plist_oriya_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name oriya-cdac :docstring \"Glyphs of Oriya script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Oriya glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1574656 :dimension 1 :long-name \"CDAC Oriya glyphs\" :base oriya-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'oriya-cdac)", expect);
}

#[test]
fn div_cs_plist_pt154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ptcp154 :docstring \"ParaType codepage (Asian Cyrillic)\" :short-name \"PT154\" :ascii-compatible-p t :code-space [0 255] :mime-charset pt154 :map \"PTCP154\" :dimension 1 :long-name \"PT154\" :base ptcp154)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'pt154)", expect);
}

#[test]
fn div_cs_plist_ptcp154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name ptcp154 :docstring \"ParaType codepage (Asian Cyrillic)\" :short-name \"PT154\" :ascii-compatible-p t :code-space [0 255] :mime-charset pt154 :map \"PTCP154\" :dimension 1 :long-name \"PT154\" :base ptcp154)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ptcp154)", expect);
}

#[test]
fn div_cs_plist_punjabi_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name punjabi-akruti :docstring \"Glyphs of Punjabi script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Punjabi glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1576448 :dimension 1 :long-name \"AKRUTI Punjabi glyphs\" :base punjabi-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'punjabi-akruti)", expect);
}

#[test]
fn div_cs_plist_punjabi_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name punjabi-cdac :docstring \"Glyphs of Punjabi script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Punjabi glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1575680 :dimension 1 :long-name \"CDAC Punjabi glyphs\" :base punjabi-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'punjabi-cdac)", expect);
}

#[test]
fn div_cs_plist_ruscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name cp1125 :docstring \"CP1125\" :short-name \"CP1125\" :code-space [0 255] :ascii-compatible-p t :map \"CP1125\" :dimension 1 :long-name \"CP1125\" :base cp1125)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ruscii)", expect);
}

#[test]
fn div_cs_plist_sanskrit_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name sanskrit-cdac :docstring \"Glyphs of Sanskrit script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Sanskrit glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1573376 :dimension 1 :long-name \"CDAC Sanskrit glyphs\" :base sanskrit-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'sanskrit-cdac)", expect);
}

#[test]
fn div_cs_plist_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name symbol :docstring \"Adobe symbol encoding used in PostScript\" :short-name \"ADOBE-SYMBOL\" :code-space [32 255] :map \"symbol\" :dimension 1 :long-name \"ADOBE-SYMBOL\" :base symbol)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'symbol)", expect);
}

#[test]
fn div_cs_plist_tamil_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name tamil-akruti :docstring \"Glyphs of Tamil script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Tamil glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1577216 :dimension 1 :long-name \"AKRUTI Tamil glyphs\" :base tamil-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'tamil-akruti)", expect);
}

#[test]
fn div_cs_plist_tamil_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name tamil-cdac :docstring \"Glyphs of Tamil script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Tamil glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1573888 :dimension 1 :long-name \"CDAC Tamil glyphs\" :base tamil-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'tamil-cdac)", expect);
}

#[test]
fn div_cs_plist_tcvn_5712() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name vscii :docstring \"VSCII1.1 (TCVN-5712 VN1)\" :short-name \"VSCII\" :code-space [0 255] :map \"VSCII\" :dimension 1 :long-name \"VSCII\" :base vscii)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'tcvn-5712)", expect);
}

#[test]
fn div_cs_plist_telugu_akruti() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name telugu-akruti :docstring \"Glyphs of Telugu script for AKRUTI font.  Subset of `indian-glyph'.\" :short-name \"AKRUTI Telugu glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1577472 :dimension 1 :long-name \"AKRUTI Telugu glyphs\" :base telugu-akruti)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'telugu-akruti)", expect);
}

#[test]
fn div_cs_plist_telugu_cdac() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name telugu-cdac :docstring \"Glyphs of Telugu script for CDAC font.  Subset of `indian-glyph'.\" :short-name \"CDAC Telugu glyphs\" :supplementary-p t :code-space [0 255] :code-offset 1574144 :dimension 1 :long-name \"CDAC Telugu glyphs\" :base telugu-cdac)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'telugu-cdac)", expect);
}

#[test]
fn div_cs_plist_thai_iso8859_11() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name thai-iso8859-11 :docstring \"Right-Hand Part of ISO/IEC 8859/11 (Latin/Thai): ISO-IR-166\" :short-name \"RHP of ISO/IEC 8859/11\" :long-name \"RHP of ISO/IEC 8859/11 (Latin/Thai)\" :iso-final-char 84 :emacs-mule-id nil :code-space [32 127] :subset (iso-8859-11 160 255 -128) :dimension 1 :base thai-iso8859-11)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'thai-iso8859-11)", expect);
}

#[test]
fn div_cs_plist_thai_tis620() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name thai-tis620 :docstring \"MULE charset for TIS620.2533\" :short-name \"TIS620.2533\" :iso-final-char 84 :emacs-mule-id 133 :code-space [32 127] :code-offset 3584 :dimension 1 :long-name \"TIS620.2533\" :base thai-tis620 preferred-coding-system thai-tis620)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'thai-tis620)", expect);
}

#[test]
fn div_cs_plist_tibetan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name tibetan :docstring \"Tibetan characters\" :iso-final-char 55 :short-name \"Tibetan 2-col\" :long-name \"Tibetan 2 column\" :iso-final-char 55 :emacs-mule-id 252 :unify-map \"MULE-tibetan\" :supplementary-p t :code-space [33 126 33 37] :code-offset 1638400 :dimension 2 :base tibetan preferred-coding-system tibetan)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'tibetan)", expect);
}

#[test]
fn div_cs_plist_tibetan_1_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name tibetan-1-column :docstring \"Tibetan 1 column glyph\" :short-name \"Tibetan 1-col\" :long-name \"Tibetan 1 column\" :iso-final-char 56 :emacs-mule-id 241 :supplementary-p t :code-space [33 126 33 37] :code-offset 1638400 :dimension 2 :base tibetan-1-column preferred-coding-system tibetan)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'tibetan-1-column)", expect);
}

#[test]
fn div_cs_plist_tis620_2533() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name tis620-2533 :docstring \"TIS620.2533, a.k.a. TIS-620.  Like `thai-iso8859-11', but without NBSP.\" :short-name \"TIS620.2533\" :ascii-compatible-p t :code-space [0 255] :superset (ascii (thai-tis620 . 128)) :dimension 1 :long-name \"TIS620.2533\" :base tis620-2533)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'tis620-2533)", expect);
}

#[test]
fn div_cs_plist_ucs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name unicode :dimension 3 :code-space [0 255 0 255 0 16 0 0] :iso-final-char nil :emacs-mule-id nil :ascii-compatible-p t :code-offset 0 :docstring \"Unicode (ISO10646)\" :short-name \"Unicode\" :long-name \"Unicode (ISO10646)\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'ucs)", expect);
}

#[test]
fn div_cs_plist_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name unicode :dimension 3 :code-space [0 255 0 255 0 16 0 0] :iso-final-char nil :emacs-mule-id nil :ascii-compatible-p t :code-offset 0 :docstring \"Unicode (ISO10646)\" :short-name \"Unicode\" :long-name \"Unicode (ISO10646)\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'unicode)", expect);
}

#[test]
fn div_cs_plist_unicode_bmp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name unicode-bmp :docstring \"Unicode Basic Multilingual Plane (U+0000..U+FFFF)\" :short-name \"Unicode BMP\" :code-space [0 255 0 255] :code-offset 0 :dimension 2 :long-name \"Unicode BMP\" :base unicode-bmp)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'unicode-bmp)", expect);
}

#[test]
fn div_cs_plist_unicode_sip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name unicode-sip :docstring \"Unicode Supplementary Ideographic Plane (U+20000..U+2FFFF)\" :short-name \"Unicode SIP\" :code-space [0 255 0 255] :code-offset 131072 :dimension 2 :long-name \"Unicode SIP\" :base unicode-sip)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'unicode-sip)", expect);
}

#[test]
fn div_cs_plist_unicode_smp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name unicode-smp :docstring \"Unicode Supplementary Multilingual Plane (U+10000..U+1FFFF)\" :short-name \"Unicode SMP \" :code-space [0 255 0 255] :code-offset 65536 :dimension 2 :long-name \"Unicode SMP \" :base unicode-smp)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'unicode-smp)", expect);
}

#[test]
fn div_cs_plist_unicode_ssp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name unicode-ssp :docstring \"Unicode Supplementary Special-purpose Plane (U+E0000..U+EFFFF)\" :short-name \"Unicode SSP\" :code-space [0 255 0 255] :code-offset 917504 :dimension 2 :long-name \"Unicode SSP\" :base unicode-ssp)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'unicode-ssp)", expect);
}

#[test]
fn div_cs_plist_vietnamese_viscii_lower() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name vietnamese-viscii-lower :docstring \"VISCII1.1 lower-case\" :short-name \"VISCII lower\" :long-name \"VISCII lower-case\" :iso-final-char 49 :emacs-mule-id 162 :code-space [32 127] :code-offset 2097664 :supplementary-p t :unify-map \"MULE-lviscii\" :dimension 1 :base vietnamese-viscii-lower preferred-coding-system vietnamese-viscii)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'vietnamese-viscii-lower)", expect);
}

#[test]
fn div_cs_plist_vietnamese_viscii_upper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name vietnamese-viscii-upper :docstring \"VISCII1.1 upper-case\" :short-name \"VISCII upper\" :long-name \"VISCII upper-case\" :iso-final-char 50 :emacs-mule-id 163 :code-space [32 127] :code-offset 2097792 :supplementary-p t :unify-map \"MULE-uviscii\" :dimension 1 :base vietnamese-viscii-upper preferred-coding-system vietnamese-viscii)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'vietnamese-viscii-upper)", expect);
}

#[test]
fn div_cs_plist_viscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name viscii :docstring \"VISCII1.1\" :short-name \"VISCII\" :long-name \"VISCII 1.1\" :code-space [0 255] :map \"VISCII\" :dimension 1 :base viscii)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'viscii)", expect);
}

#[test]
fn div_cs_plist_vscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name vscii :docstring \"VSCII1.1 (TCVN-5712 VN1)\" :short-name \"VSCII\" :code-space [0 255] :map \"VSCII\" :dimension 1 :long-name \"VSCII\" :base vscii)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'vscii)", expect);
}

#[test]
fn div_cs_plist_vscii_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name vscii-2 :docstring \"VSCII-2 (TCVN-5712 VN2)\" :code-space [0 255] :map \"VSCII-2\" :dimension 1 :short-name \"vscii-2\" :long-name \"vscii-2\" :base vscii-2)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'vscii-2)", expect);
}

#[test]
fn div_cs_plist_windows_1250() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1250 :docstring \"WINDOWS-1250 (Central Europe)\" :short-name \"WINDOWS-1250\" :ascii-compatible-p t :code-space [0 255] :map \"CP1250\" :dimension 1 :long-name \"WINDOWS-1250\" :base windows-1250)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1250)", expect);
}

#[test]
fn div_cs_plist_windows_1251() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1251 :docstring \"WINDOWS-1251 (Cyrillic)\" :short-name \"WINDOWS-1251\" :ascii-compatible-p t :code-space [0 255] :map \"CP1251\" :dimension 1 :long-name \"WINDOWS-1251\" :base windows-1251)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1251)", expect);
}

#[test]
fn div_cs_plist_windows_1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1252 :docstring \"WINDOWS-1252 (Latin I)\" :short-name \"WINDOWS-1252\" :ascii-compatible-p t :code-space [0 255] :map \"CP1252\" :dimension 1 :long-name \"WINDOWS-1252\" :base windows-1252)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1252)", expect);
}

#[test]
fn div_cs_plist_windows_1253() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1253 :docstring \"WINDOWS-1253 (Greek)\" :short-name \"WINDOWS-1253\" :ascii-compatible-p t :code-space [0 255] :map \"CP1253\" :dimension 1 :long-name \"WINDOWS-1253\" :base windows-1253)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1253)", expect);
}

#[test]
fn div_cs_plist_windows_1254() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1254 :docstring \"WINDOWS-1254 (Turkish)\" :short-name \"WINDOWS-1254\" :ascii-compatible-p t :code-space [0 255] :map \"CP1254\" :dimension 1 :long-name \"WINDOWS-1254\" :base windows-1254)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1254)", expect);
}

#[test]
fn div_cs_plist_windows_1255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1255 :docstring \"WINDOWS-1255 (Hebrew)\" :short-name \"WINDOWS-1255\" :ascii-compatible-p t :code-space [0 255] :map \"CP1255\" :dimension 1 :long-name \"WINDOWS-1255\" :base windows-1255)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1255)", expect);
}

#[test]
fn div_cs_plist_windows_1256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1256 :docstring \"WINDOWS-1256 (Arabic)\" :short-name \"WINDOWS-1256\" :ascii-compatible-p t :code-space [0 255] :map \"CP1256\" :dimension 1 :long-name \"WINDOWS-1256\" :base windows-1256)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1256)", expect);
}

#[test]
fn div_cs_plist_windows_1257() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1257 :docstring \"WINDOWS-1257 (Baltic)\" :short-name \"WINDOWS-1257\" :ascii-compatible-p t :code-space [0 255] :map \"CP1257\" :dimension 1 :long-name \"WINDOWS-1257\" :base windows-1257)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1257)", expect);
}

#[test]
fn div_cs_plist_windows_1258() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name windows-1258 :docstring \"WINDOWS-1258 (Viet Nam)\" :short-name \"WINDOWS-1258\" :ascii-compatible-p t :code-space [0 255] :map \"CP1258\" :dimension 1 :long-name \"WINDOWS-1258\" :base windows-1258)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-1258)", expect);
}

#[test]
fn div_cs_plist_windows_936() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:name chinese-gbk :docstring \"GBK Chinese simplified.\" :short-name \"GBK\" :code-space [64 254 129 254] :code-offset 1441792 :unify-map \"GBK\" :dimension 2 :long-name \"GBK\" :base chinese-gbk preferred-coding-system chinese-gbk)""#
    ]];
    crate::common::assert_oracle_parity_expect("(charset-plist 'windows-936)", expect);
}
