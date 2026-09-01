//! UTF-8 / multibyte *coding-system coverage* divergence probes.
//!
//! Probes whether Neomacs implements the broader coding-system taxonomy that
//! GNU ships (latin-9, windows-1252, iso-8859-7, big5, gbk, shift_jis, euc-jp,
//! koi8-r) via encode/decode round-trips, plus the `coding-system-plist`
//! `:signature` property — the root cause of the `utf-8-with-signature` BOM
//! divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- :signature plist (BOM root cause) --------------------------------------

#[test]
fn div_utf8_coding_system_plist_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (plist-get (coding-system-plist 'utf-8) :signature)
      (plist-get (coding-system-plist 'utf-8-with-signature) :signature)
      (plist-get (coding-system-plist 'utf-16) :signature)
      (plist-get (coding-system-plist 'utf-16le) :signature)
      (plist-get (coding-system-plist 'latin-1) :signature))
"#,
        expect,
    );
}

#[test]
fn div_utf8_coding_system_category_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:ascii-compatible-p nil :category coding-category-utf-8-sig :name utf-8-with-signature :docstring \"UTF-8 (with signature (BOM))\" :coding-type utf-8 :mnemonic 85 :charset-list (unicode) :bom t) coding-category-utf-8-sig utf-8-with-signature nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (coding-system-plist 'utf-8-with-signature)
      (plist-get (coding-system-plist 'utf-8-with-signature) :category)
      (plist-get (coding-system-plist 'utf-8-with-signature) :name)
      (plist-get (coding-system-plist 'utf-8-with-signature) :eol-type))
"#,
        expect,
    );
}

// --- 8-bit coding systems ---------------------------------------------------

#[test]
fn div_utf8_latin9_euro_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((164) t 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((euro (string #x20AC))
       (e (encode-coding-string euro 'latin-9))
       (d (decode-coding-string e 'latin-9)))
  (list (append e nil) (equal euro d) (length e) (string-bytes e)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_windows1252_smart_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((146) (147))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((q (encode-coding-string (string #x2019) 'windows-1252))
      (lz (encode-coding-string (string #x201C) 'windows-1252)))
  (list (append q nil) (append lz nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_iso8859_7_greek_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((931 932 933) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((g (decode-coding-string (unibyte-string 211 212 213) 'iso-8859-7)))
  (list (append g nil) (length g)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_koi8_r_cyrillic_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1072 1073 1094) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((c (decode-coding-string (unibyte-string 193 194 195) 'koi8-r)))
  (list (append c nil) (length c)))
"#,
        expect,
    );
}

// --- CJK coding systems -----------------------------------------------------

#[test]
fn div_utf8_big5_cjk_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((164 164 164 229 180 250 184 213) t 8 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s "中文測試")
       (e (encode-coding-string s 'big5))
       (d (decode-coding-string e 'big5)))
  (list (append e nil) (equal s d) (length e) (string-bytes e)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_gbk_cjk_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((214 208 206 196 178 226 202 212) t 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s "中文测试")
       (e (encode-coding-string s 'gbk))
       (d (decode-coding-string e 'gbk)))
  (list (append e nil) (equal s d) (length e)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_shiftjis_japanese_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((130 177 130 241 130 201 130 191 130 205) t 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s "こんにちは")
       (e (encode-coding-string s 'shift_jis))
       (d (decode-coding-string e 'shift_jis)))
  (list (append e nil) (equal s d) (length e)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_eucjp_japanese_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((198 252 203 220 184 236 165 198 165 185 165 200) t 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s "日本語テスト")
       (e (encode-coding-string s 'euc-jp))
       (d (decode-coding-string e 'euc-jp)))
  (list (append e nil) (equal s d) (length e)))
"#,
        expect,
    );
}

// --- coding-system coverage existence ---------------------------------------

#[test]
fn div_utf8_coding_system_existence_broad() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (coding-system-p 'latin-9)
      (coding-system-p 'iso-8859-7)
      (coding-system-p 'windows-1252)
      (coding-system-p 'cp1251)
      (coding-system-p 'big5)
      (coding-system-p 'chinese-big5)
      (coding-system-p 'gbk)
      (coding-system-p 'shift_jis)
      (coding-system-p 'sjis)
      (coding-system-p 'euc-jp)
      (coding-system-p 'koi8-r)
      (coding-system-p 'utf-8-emacs))
"#,
        expect,
    );
}

// --- detect-coding-string ---------------------------------------------------

#[test]
fn div_utf8_detect_coding_string_bom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 iso-latin-1 emacs-mule in-is13194-devanagari utf-8-auto utf-8-with-signature japanese-shift-jis chinese-big5 iso-2022-8bit-ss2) (no-conversion) (no-conversion))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (detect-coding-string (unibyte-string 239 187 191 97 98 99))
      (detect-coding-string (unibyte-string 254 255 0 97))
      (detect-coding-string (unibyte-string 255 254 97 0)))
"#,
        expect,
    );
}
