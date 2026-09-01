//! Strict combo oracle probes, batch 177: charset operations. decode-char /
//! encode-char round-trips for iso-8859-1 / ascii / unicode, char-charset over
//! latin / CJK / combining, charsetp, charset-priority-list, and coding-system-
//! charset-list.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_charset_decode_encode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (decode-char 'iso-8859-1 233)
      (encode-char ?é 'iso-8859-1)
      (decode-char 'ascii 65)
      (encode-char ?A 'ascii)
      (decode-char 'unicode 0x00e9)
      (encode-char ?é 'unicode)
      (decode-char 'eight-bit 200)
      (encode-char (decode-char 'iso-8859-1 241) 'iso-8859-1)
      (decode-char 'iso-8859-1 65))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable 0x00e9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_charset_p_charset_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (char-charset ?a)
      (char-charset ?é)
      (char-charset ?日)
      (char-charset ? )
      (char-charset 127)
      (charsetp 'ascii)
      (charsetp 'unicode)
      (charsetp 'iso-8859-1)
      (charsetp 'not-a-charset)
      (charsetp 42)
      (consp (charset-priority-list))
      (memq 'ascii (charset-priority-list))
      (memq 'unicode (charset-priority-list)))
"##;
    let expect = expect_test::expect![[
        r#""OK (ascii unicode-bmp unicode-bmp ascii ascii t t t nil nil t (ascii latin-iso8859-1 control-1 iso-8859-2 latin-iso8859-2 iso-8859-3 latin-iso8859-3 iso-8859-4 latin-iso8859-4 iso-8859-5 cyrillic-iso8859-5 iso-8859-6 arabic-iso8859-6 iso-8859-7 greek-iso8859-7 iso-8859-8 hebrew-iso8859-8 iso-8859-9 latin-iso8859-9 iso-8859-10 latin-iso8859-10 iso-8859-11 thai-iso8859-11 iso-8859-13 latin-iso8859-13 iso-8859-14 latin-iso8859-14 iso-8859-15 latin-iso8859-15 iso-8859-16 latin-iso8859-16 thai-tis620 tis620-2533 jisx0201 chinese-gb2312 chinese-gbk chinese-cns11643-1 chinese-cns11643-2 chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7 big5 japanese-jisx0208 japanese-jisx0208-1978 japanese-jisx0212 japanese-jisx0213-1 japanese-jisx0213-2 japanese-jisx0213.2004-1 cp932 korean-ksc5601 big5-hkscs cp949 viscii vscii vscii-2 koi8-r alternativnyj cp866 koi8-u koi8-t georgian-ps georgian-academy windows-1250 windows-1251 windows-1252 windows-1253 windows-1254 windows-1255 windows-1256 windows-1257 windows-1258 next cp1125 cp437 cp720 cp737 cp775 cp851 cp852 cp855 cp857 cp858 cp860 cp861 cp862 cp863 cp864 cp865 cp869 cp874 unicode-smp unicode-sip unicode-ssp mac-roman ebcdic-us ebcdic-uk ibm038 ibm256 ibm273 ibm274 ibm275 ibm277 ibm278 ibm280 ibm281 ibm284 ibm285 ibm290 ibm297 ibm1047 hp-roman8 adobe-standard-encoding symbol ibm850 mik ptcp154 gb18030 chinese-cns11643-15 emacs eight-bit eight-bit-control eight-bit-graphic latin-jisx0201 katakana-jisx0201 chinese-big5-1 chinese-big5-2 japanese-jisx0213-a katakana-sjis cp932-2-byte cp949-2-byte chinese-sisheng ipa vietnamese-viscii-lower vietnamese-viscii-upper arabic-digit arabic-1-column arabic-2-column lao mule-lao indian-is13194 devanagari-cdac sanskrit-cdac bengali-cdac tamil-cdac telugu-cdac assamese-cdac oriya-cdac kannada-cdac malayalam-cdac gujarati-cdac punjabi-cdac devanagari-akruti bengali-akruti punjabi-akruti gujarati-akruti oriya-akruti tamil-akruti telugu-akruti kannada-akruti malayalam-akruti indian-glyph indian-1-column indian-2-column tibetan tibetan-1-column mule-unicode-2500-33ff mule-unicode-e000-ffff mule-unicode-0100-24ff ethiopic gb18030-2-byte gb18030-4-byte-bmp gb18030-4-byte-smp gb18030-4-byte-ext-1 gb18030-4-byte-ext-2) (unicode iso-8859-1 ascii latin-iso8859-1 control-1 iso-8859-2 latin-iso8859-2 iso-8859-3 latin-iso8859-3 iso-8859-4 latin-iso8859-4 iso-8859-5 cyrillic-iso8859-5 iso-8859-6 arabic-iso8859-6 iso-8859-7 greek-iso8859-7 iso-8859-8 hebrew-iso8859-8 iso-8859-9 latin-iso8859-9 iso-8859-10 latin-iso8859-10 iso-8859-11 thai-iso8859-11 iso-8859-13 latin-iso8859-13 iso-8859-14 latin-iso8859-14 iso-8859-15 latin-iso8859-15 iso-8859-16 latin-iso8859-16 thai-tis620 tis620-2533 jisx0201 chinese-gb2312 chinese-gbk chinese-cns11643-1 chinese-cns11643-2 chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7 big5 japanese-jisx0208 japanese-jisx0208-1978 japanese-jisx0212 japanese-jisx0213-1 japanese-jisx0213-2 japanese-jisx0213.2004-1 cp932 korean-ksc5601 big5-hkscs cp949 viscii vscii vscii-2 koi8-r alternativnyj cp866 koi8-u koi8-t georgian-ps georgian-academy windows-1250 windows-1251 windows-1252 windows-1253 windows-1254 windows-1255 windows-1256 windows-1257 windows-1258 next cp1125 cp437 cp720 cp737 cp775 cp851 cp852 cp855 cp857 cp858 cp860 cp861 cp862 cp863 cp864 cp865 cp869 cp874 unicode-smp unicode-sip unicode-ssp mac-roman ebcdic-us ebcdic-uk ibm038 ibm256 ibm273 ibm274 ibm275 ibm277 ibm278 ibm280 ibm281 ibm284 ibm285 ibm290 ibm297 ibm1047 hp-roman8 adobe-standard-encoding symbol ibm850 mik ptcp154 gb18030 chinese-cns11643-15 emacs eight-bit eight-bit-control eight-bit-graphic latin-jisx0201 katakana-jisx0201 chinese-big5-1 chinese-big5-2 japanese-jisx0213-a katakana-sjis cp932-2-byte cp949-2-byte chinese-sisheng ipa vietnamese-viscii-lower vietnamese-viscii-upper arabic-digit arabic-1-column arabic-2-column lao mule-lao indian-is13194 devanagari-cdac sanskrit-cdac bengali-cdac tamil-cdac telugu-cdac assamese-cdac oriya-cdac kannada-cdac malayalam-cdac gujarati-cdac punjabi-cdac devanagari-akruti bengali-akruti punjabi-akruti gujarati-akruti oriya-akruti tamil-akruti telugu-akruti kannada-akruti malayalam-akruti indian-glyph indian-1-column indian-2-column tibetan tibetan-1-column mule-unicode-2500-33ff mule-unicode-e000-ffff mule-unicode-0100-24ff ethiopic gb18030-2-byte gb18030-4-byte-bmp gb18030-4-byte-smp gb18030-4-byte-ext-1 gb18030-4-byte-ext-2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_charset_list_dimension_plane() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (charset-dimension 'ascii)
      (charset-dimension 'iso-8859-1)
      (charset-dimension 'unicode)
      (charset-plist 'ascii)
      (charset-plist 'iso-8859-1)
      (consp (charset-list))
      (memq 'ascii (charset-list))
      (memq 'unicode (charset-list))
      (charset-id 'ascii)
      (charset-id 'unicode)
      (list-charset-chars 'ascii))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function charset-list)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
