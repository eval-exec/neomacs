//! UTF-8 / multibyte *legacy coding-system matrix* divergence probes.
//!
//! One focused test per legacy coding system (ISO-8859 family, Windows
//! codepages, KOI8, Mac-Roman, Vietnamese/Thai, and CJK ISO-2022 / EUC /
//! GB18030 / UTF-7 / emacs-mule). Neomacs supports UTF-8/16, latin-1/9, and
//! big5, but most of these legacy codings are unsupported and yield U+FFFD
//! (decode) or nil (encode) instead of the correct characters.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- ISO-8859 single-byte family --------------------------------------------

macro_rules! iso_decode_test {
    ($name:ident, $coding:literal, $bytes:expr) => {
        #[test]
        fn $name() {
            return_if_neovm_enable_oracle_proptest_not_set!();
            let form = concat!(
                "(let ((d (decode-coding-string (unibyte-string ",
                $bytes,
                ") '",
                $coding,
                ")))\n  (list (length d) (append d nil)))"
            );
            crate::common::assert_oracle_parity(form);
        }
    };
}

iso_decode_test!(
    div_utf8_decode_iso8859_2,
    "iso-8859-2",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_3,
    "iso-8859-3",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_4,
    "iso-8859-4",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_5,
    "iso-8859-5",
    "161 176 198 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_6,
    "iso-8859-6",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_8,
    "iso-8859-8",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_9,
    "iso-8859-9",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_10,
    "iso-8859-10",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_13,
    "iso-8859-13",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_14,
    "iso-8859-14",
    "161 198 230 240 253"
);
iso_decode_test!(
    div_utf8_decode_iso8859_16,
    "iso-8859-16",
    "161 198 230 240 253"
);

// --- Windows codepages ------------------------------------------------------

iso_decode_test!(
    div_utf8_decode_windows_1250,
    "windows-1250",
    "140 141 156 159 165"
);
iso_decode_test!(
    div_utf8_decode_windows_1251,
    "windows-1251",
    "192 193 194 195 196"
);
iso_decode_test!(
    div_utf8_decode_windows_1253,
    "windows-1253",
    "193 194 195 196 197"
);
iso_decode_test!(
    div_utf8_decode_windows_1254,
    "windows-1254",
    "199 208 221 222 254"
);
iso_decode_test!(
    div_utf8_decode_windows_1255,
    "windows-1255",
    "224 231 32 241 250"
);
iso_decode_test!(
    div_utf8_decode_windows_1256,
    "windows-1256",
    "199 32 218 225 237"
);
iso_decode_test!(
    div_utf8_decode_windows_1257,
    "windows-1257",
    "193 196 197 198 207"
);

// --- KOI8 / Mac / Vietnamese / Thai -----------------------------------------

iso_decode_test!(div_utf8_decode_koi8_u, "koi8-u", "193 194 195 196 197");
iso_decode_test!(
    div_utf8_decode_mac_roman,
    "mac-roman",
    "129 143 167 200 201 214"
);
iso_decode_test!(div_utf8_decode_viscii, "viscii", "161 178 192 199 252");
iso_decode_test!(div_utf8_decode_tis620, "tis-620", "161 198 209 225 249");

// --- CJK / multibyte legacy (round-trip) ------------------------------------

macro_rules! cjk_roundtrip_test {
    ($name:ident, $coding:literal, $text:literal) => {
        #[test]
        fn $name() {
            return_if_neovm_enable_oracle_proptest_not_set!();
            let form = concat!(
                "(let* ((s ",
                $text,
                ")\n",
                "       (e (encode-coding-string s '",
                $coding,
                "))\n",
                "       (d (decode-coding-string e '",
                $coding,
                ")))\n",
                "  (list (append e nil) (equal s d) (length e)))"
            );
            crate::common::assert_oracle_parity(form);
        }
    };
}

cjk_roundtrip_test!(div_utf8_euc_kr_roundtrip, "euc-kr", "\"안녕하세요\"");
cjk_roundtrip_test!(div_utf8_gb2312_roundtrip, "gb2312", "\"中文测试\"");
cjk_roundtrip_test!(div_utf8_gb18030_roundtrip, "gb18030", "\"中文测试😀\"");
// GB18030 covers all of Unicode through its 2-byte (GBK subset), 4-byte BMP,
// and 4-byte SMP ranges.  Exercise each structural form: pure ASCII (1-byte),
// pure GBK (2-byte), a BMP code point absent from GBK that needs the 4-byte BMP
// table (U+00A4 CURRENCY SIGN), an astral SMP code point (😀 U+1F600), and the
// maximum SMP code point (U+10FFFF), all of which GNU encodes via its
// `:charset-list` charset codec.
cjk_roundtrip_test!(
    div_utf8_gb18030_ascii_roundtrip,
    "gb18030",
    "\"Hello, world!\""
);
cjk_roundtrip_test!(div_utf8_gb18030_gbk_roundtrip, "gb18030", "\"中文测试\"");
cjk_roundtrip_test!(div_utf8_gb18030_bmp_gap_roundtrip, "gb18030", "\"¤£¡¿\"");
cjk_roundtrip_test!(
    div_utf8_gb18030_smp_max_roundtrip,
    "gb18030",
    "(string #x10FFFF)"
);
cjk_roundtrip_test!(div_utf8_gb18030_mixed_roundtrip, "gb18030", "\"AB中¤文😀\"");

// GB18030 attaches `(charset gb18030-2-byte)` / `(charset gb18030-4-byte-bmp)`
// / `(charset gb18030-4-byte-smp)` text properties when decoding, identifying
// which structural form each character came from.  Verify the decoded property
// runs match GNU byte-for-byte.
#[test]
fn div_utf8_gb18030_decode_charset_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((65 214 208 161 232 148 57 252 54) t ((0 nil) (1 gb18030-2-byte) (2 gb18030-2-byte) (3 gb18030-4-byte-smp)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s "A中¤😀")
       (e (encode-coding-string s 'gb18030))
       (d (decode-coding-string e 'gb18030))
       (props nil))
  (let ((i 0) (n (length d)))
    (while (< i n)
      (push (list i (get-text-property i 'charset d)) props)
      (setq i (1+ i))))
  (list (append e nil) (equal s d) (nreverse props)))
"#,
        expect,
    );
}
cjk_roundtrip_test!(
    div_utf8_iso2022_jp_roundtrip,
    "iso-2022-jp",
    "\"こんにちは\""
);
cjk_roundtrip_test!(div_utf8_iso2022_cn_roundtrip, "iso-2022-cn", "\"中文测试\"");
cjk_roundtrip_test!(div_utf8_iso2022_kr_roundtrip, "iso-2022-kr", "\"안녕\"");
cjk_roundtrip_test!(div_utf8_utf7_roundtrip, "utf-7", "\"café世界\"");
cjk_roundtrip_test!(div_utf8_emacs_mule_roundtrip, "emacs-mule", "\"café世界\"");

// --- detect-coding / find-operation-coding-system --------------------------

#[test]
fn div_utf8_find_auto_coding_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((225 262 263 273))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((s (decode-coding-string (unibyte-string 225 198 230 240) 'iso-8859-2)))
  (list (append s nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decode_coding_string_cyrillic_iso8859_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 (1040 1041 1042 1043 1044))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 176 177 178 179 180) 'iso-8859-5)))
  (list (length d) (append d nil)))
"#,
        expect,
    );
}
