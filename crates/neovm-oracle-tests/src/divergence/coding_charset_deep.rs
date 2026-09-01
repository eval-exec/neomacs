//! Divergence tests: coding system conversion, charset mapping deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_coding_system_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t (utf-8 utf-8-with-signature utf-8-auto utf-8-emacs utf-16le utf-16be utf-16-le utf-16le-with-signature utf-16-be utf-16be-with-signature utf-16 iso-2022-7bit iso-2022-7bit-ss2 iso-2022-int-1 iso-2022-7bit-lock iso-2022-cjk iso-2022-7bit-lock-ss2 iso-2022-8bit-ss2 ctext x-ctext compound-text ctext-no-compositions ctext-with-extensions x-ctext-with-extensions compound-text-with-extensions ascii iso-safe us-ascii utf-7 utf-7-imap chinese-iso-7bit iso-2022-cn iso-2022-cn-ext gb2312 cn-gb euc-cn euc-china cn-gb-2312 chinese-iso-8bit hz hz-gb-2312 chinese-hz cp950 cn-big5 big5 chinese-big5 cn-big5-hkscs big5-hkscs chinese-big5-hkscs euc-taiwan euc-tw windows-936 cp936 gbk chinese-gbk gb18030 chinese-gb18030 iso-8859-5 cyrillic-iso-8bit cp878 koi8 koi8-r cyrillic-koi8 koi8-u alternativnyj cyrillic-alternativnyj cp866 koi8-t cp1251 windows-1251 cp866u ruscii cp1125 ibm855 cp855 mik pt154 devanagari in-is13194-devanagari ebcdic-us ebcdic-uk cp1047 ibm1047 cp038 ebcdic-int ibm038 latin-2 iso-8859-2 iso-latin-2 latin-3 iso-8859-3 iso-latin-3 latin-4 iso-8859-4 iso-latin-4 latin-5 iso-8859-9 iso-latin-5 latin-6 iso-8859-10 iso-latin-6 latin-7 iso-8859-13 iso-latin-7 latin-8 iso-8859-14 iso-latin-8 latin-0 latin-9 iso-8859-15 iso-latin-9 cp1250 windows-1250 cp1252 windows-1252 cp1254 windows-1254 cp1257 windows-1257 cp256 ebcdic-int1 ibm256 cp273 ibm273 cp274 ebcdic-be ibm274 cp275 ebcdic-br ibm275 cp277 ebcdic-cp-no ebcdic-cp-dk ibm277 cp278 ebcdic-cp-se ebcdic-cp-fi ibm278 cp280 ebcdic-cp-it ibm280 cp284 ebcdic-cp-es ibm284 cp285 ebcdic-cp-gb ibm285 cp297 ebcdic-cp-fr ibm297 ibm775 cp775 ibm850 cp850 ibm852 cp852 ibm857 cp857 cp858 ibm860 cp860 ibm861 cp861 ibm863 cp863 ibm865 cp865 ibm437 cp437 macintosh mac-roman next roman8 hp-roman8 adobe-standard-encoding latin-10 iso-8859-16 iso-latin-10 iso-8859-7 greek-iso-8bit cp1253 windows-1253 cp737 ibm851 cp851 ibm869 cp869 iso-8859-8-i iso-8859-8-e iso-8859-8 hebrew-iso-8bit cp1255 windows-1255 ibm862 cp862 junet iso-2022-jp iso-2022-jp-2 sjis shift_jis japanese-shift-jis cp932 japanese-cp932 old-jis iso-2022-jp-1978-irv japanese-iso-7bit-1978-irv euc-jp euc-japan euc-japan-1990 japanese-iso-8bit eucjp-ms iso-2022-jp-3 iso-2022-jp-2004 euc-jisx0213 euc-jis-2004 shift_jis-2004 japanese-shift-jis-2004 cp281 ebcdic-jp-e ibm281 cp290 ebcdic-jp-kana ibm290 ks_c_5601-1987 euc-korea euc-kr korean-iso-8bit korean-iso-7bit-lock iso-2022-kr cp949 korean-cp949 lao tis-620 tis620 th-tis620 thai-tis620 ibm874 cp874 iso-8859-11 tibetan tibetan-iso-8bit viscii vietnamese-viscii tcvn-5712 tcvn vietnamese-tcvn vscii vietnamese-vscii viqr vietnamese-viqr cp1258 windows-1258 iso-8859-6 cp1256 windows-1256 georgian-ps georgian-academy) (latin-1 iso-8859-1 iso-latin-1 emacs-mule cp65001 mule-utf-8 utf-8 utf-8-with-signature utf-8-auto utf-8-emacs utf-16le utf-16be utf-16-le utf-16le-with-signature utf-16-be utf-16be-with-signature utf-16 iso-2022-7bit iso-2022-7bit-ss2 iso-2022-int-1 iso-2022-7bit-lock iso-2022-cjk iso-2022-7bit-lock-ss2 iso-2022-8bit-ss2 ctext x-ctext compound-text ctext-no-compositions ctext-with-extensions x-ctext-with-extensions compound-text-with-extensions ascii iso-safe us-ascii utf-7 utf-7-imap chinese-iso-7bit iso-2022-cn iso-2022-cn-ext gb2312 cn-gb euc-cn euc-china cn-gb-2312 chinese-iso-8bit hz hz-gb-2312 chinese-hz cp950 cn-big5 big5 chinese-big5 cn-big5-hkscs big5-hkscs chinese-big5-hkscs euc-taiwan euc-tw windows-936 cp936 gbk chinese-gbk gb18030 chinese-gb18030 iso-8859-5 cyrillic-iso-8bit cp878 koi8 koi8-r cyrillic-koi8 koi8-u alternativnyj cyrillic-alternativnyj cp866 koi8-t cp1251 windows-1251 cp866u ruscii cp1125 ibm855 cp855 mik pt154 devanagari in-is13194-devanagari ebcdic-us ebcdic-uk cp1047 ibm1047 cp038 ebcdic-int ibm038 latin-2 iso-8859-2 iso-latin-2 latin-3 iso-8859-3 iso-latin-3 latin-4 iso-8859-4 iso-latin-4 latin-5 iso-8859-9 iso-latin-5 latin-6 iso-8859-10 iso-latin-6 latin-7 iso-8859-13 iso-latin-7 latin-8 iso-8859-14 iso-latin-8 latin-0 latin-9 iso-8859-15 iso-latin-9 cp1250 windows-1250 cp1252 windows-1252 cp1254 windows-1254 cp1257 windows-1257 cp256 ebcdic-int1 ibm256 cp273 ibm273 cp274 ebcdic-be ibm274 cp275 ebcdic-br ibm275 cp277 ebcdic-cp-no ebcdic-cp-dk ibm277 cp278 ebcdic-cp-se ebcdic-cp-fi ibm278 cp280 ebcdic-cp-it ibm280 cp284 ebcdic-cp-es ibm284 cp285 ebcdic-cp-gb ibm285 cp297 ebcdic-cp-fr ibm297 ibm775 cp775 ibm850 cp850 ibm852 cp852 ibm857 cp857 cp858 ibm860 cp860 ibm861 cp861 ibm863 cp863 ibm865 cp865 ibm437 cp437 macintosh mac-roman next roman8 hp-roman8 adobe-standard-encoding latin-10 iso-8859-16 iso-latin-10 iso-8859-7 greek-iso-8bit cp1253 windows-1253 cp737 ibm851 cp851 ibm869 cp869 iso-8859-8-i iso-8859-8-e iso-8859-8 hebrew-iso-8bit cp1255 windows-1255 ibm862 cp862 junet iso-2022-jp iso-2022-jp-2 sjis shift_jis japanese-shift-jis cp932 japanese-cp932 old-jis iso-2022-jp-1978-irv japanese-iso-7bit-1978-irv euc-jp euc-japan euc-japan-1990 japanese-iso-8bit eucjp-ms iso-2022-jp-3 iso-2022-jp-2004 euc-jisx0213 euc-jis-2004 shift_jis-2004 japanese-shift-jis-2004 cp281 ebcdic-jp-e ibm281 cp290 ebcdic-jp-kana ibm290 ks_c_5601-1987 euc-korea euc-kr korean-iso-8bit korean-iso-7bit-lock iso-2022-kr cp949 korean-cp949 lao tis-620 tis620 th-tis620 thai-tis620 ibm874 cp874 iso-8859-11 tibetan tibetan-iso-8bit viscii vietnamese-viscii tcvn-5712 tcvn vietnamese-tcvn vscii vietnamese-vscii viqr vietnamese-viqr cp1258 windows-1258 iso-8859-6 cp1256 windows-1256 georgian-ps georgian-academy) (binary no-conversion undecided prefer-utf-8 raw-text no-conversion-multibyte latin-1 iso-8859-1 iso-latin-1 emacs-mule cp65001 mule-utf-8 utf-8 utf-8-with-signature utf-8-auto utf-8-emacs utf-16le utf-16be utf-16-le utf-16le-with-signature utf-16-be utf-16be-with-signature utf-16 iso-2022-7bit iso-2022-7bit-ss2 iso-2022-int-1 iso-2022-7bit-lock iso-2022-cjk iso-2022-7bit-lock-ss2 iso-2022-8bit-ss2 ctext x-ctext compound-text ctext-no-compositions ctext-with-extensions x-ctext-with-extensions compound-text-with-extensions ascii iso-safe us-ascii utf-7 utf-7-imap chinese-iso-7bit iso-2022-cn iso-2022-cn-ext gb2312 cn-gb euc-cn euc-china cn-gb-2312 chinese-iso-8bit hz hz-gb-2312 chinese-hz cp950 cn-big5 big5 chinese-big5 cn-big5-hkscs big5-hkscs chinese-big5-hkscs euc-taiwan euc-tw windows-936 cp936 gbk chinese-gbk gb18030 chinese-gb18030 iso-8859-5 cyrillic-iso-8bit cp878 koi8 koi8-r cyrillic-koi8 koi8-u alternativnyj cyrillic-alternativnyj cp866 koi8-t cp1251 windows-1251 cp866u ruscii cp1125 ibm855 cp855 mik pt154 devanagari in-is13194-devanagari ebcdic-us ebcdic-uk cp1047 ibm1047 cp038 ebcdic-int ibm038 latin-2 iso-8859-2 iso-latin-2 latin-3 iso-8859-3 iso-latin-3 latin-4 iso-8859-4 iso-latin-4 latin-5 iso-8859-9 iso-latin-5 latin-6 iso-8859-10 iso-latin-6 latin-7 iso-8859-13 iso-latin-7 latin-8 iso-8859-14 iso-latin-8 latin-0 latin-9 iso-8859-15 iso-latin-9 cp1250 windows-1250 cp1252 windows-1252 cp1254 windows-1254 cp1257 windows-1257 cp256 ebcdic-int1 ibm256 cp273 ibm273 cp274 ebcdic-be ibm274 cp275 ebcdic-br ibm275 cp277 ebcdic-cp-no ebcdic-cp-dk ibm277 cp278 ebcdic-cp-se ebcdic-cp-fi ibm278 cp280 ebcdic-cp-it ibm280 cp284 ebcdic-cp-es ibm284 cp285 ebcdic-cp-gb ibm285 cp297 ebcdic-cp-fr ibm297 ibm775 cp775 ibm850 cp850 ibm852 cp852 ibm857 cp857 cp858 ibm860 cp860 ibm861 cp861 ibm863 cp863 ibm865 cp865 ibm437 cp437 macintosh mac-roman next roman8 hp-roman8 adobe-standard-encoding latin-10 iso-8859-16 iso-latin-10 iso-8859-7 greek-iso-8bit cp1253 windows-1253 cp737 ibm851 cp851 ibm869 cp869 iso-8859-8-i iso-8859-8-e iso-8859-8 hebrew-iso-8bit cp1255 windows-1255 ibm862 cp862 junet iso-2022-jp iso-2022-jp-2 sjis shift_jis japanese-shift-jis cp932 japanese-cp932 old-jis iso-2022-jp-1978-irv japanese-iso-7bit-1978-irv euc-jp euc-japan euc-japan-1990 japanese-iso-8bit eucjp-ms iso-2022-jp-3 iso-2022-jp-2004 euc-jisx0213 euc-jis-2004 shift_jis-2004 japanese-shift-jis-2004 cp281 ebcdic-jp-e ibm281 cp290 ebcdic-jp-kana ibm290 ks_c_5601-1987 euc-korea euc-kr korean-iso-8bit korean-iso-7bit-lock iso-2022-kr cp949 korean-cp949 lao tis-620 tis620 th-tis620 thai-tis620 ibm874 cp874 iso-8859-11 tibetan tibetan-iso-8bit viscii vietnamese-viscii tcvn-5712 tcvn vietnamese-tcvn vscii vietnamese-vscii viqr vietnamese-viqr cp1258 windows-1258 iso-8859-6 cp1256 windows-1256 georgian-ps georgian-academy))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((cs-list (coding-system-list)))
  (list (listp cs-list)
        (member 'utf-8 cs-list)
        (member 'latin-1 cs-list)
        (member 'binary cs-list))) "#,
        expect,
    );
}

#[test]
fn divergence_coding_system_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 iso-latin-1 no-conversion)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (coding-system-base 'utf-8)
  (coding-system-base 'utf-8-dos)
  (coding-system-base 'latin-1)
  (coding-system-base 'binary)) "#,
        expect,
    );
}

#[test]
fn divergence_coding_system_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([utf-8-unix utf-8-dos utf-8-mac] 1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (coding-system-eol-type 'utf-8)
  (coding-system-eol-type 'utf-8-dos)
  (coding-system-eol-type 'utf-8-unix)) "#,
        expect,
    );
}

#[test]
fn divergence_coding_system_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (coding-system-p 'utf-8)
  (coding-system-p 'latin-1)
  (coding-system-p 'binary)
  (coding-system-p 'nonexistent-cs)) "#,
        expect,
    );
}

#[test]
fn divergence_encode_decode_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'encode-coding-region)
  (fboundp 'decode-coding-region)
  (fboundp 'encode-coding-string)
  (fboundp 'decode-coding-string))"#,
        expect,
    );
}

#[test]
fn divergence_charset_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function charset-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((cs-list (charset-list)))
  (list (listp cs-list)
        (member 'ascii cs-list)
        (member 'unicode cs-list))) "#,
        expect,
    );
}

#[test]
fn divergence_charset_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (charsetp 'ascii)
  (charsetp 'unicode)
  (charsetp 'latin)
  (charsetp 'nonexistent)) "#,
        expect,
    );
}

#[test]
fn divergence_decode_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'decode-char)
  (fboundp 'encode-char)
  (characterp ?A)
  (characterp 128)
  (characterp #x4e2d)) "#,
        expect,
    );
}

#[test]
fn divergence_prefer_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'prefer-coding-system)
  (boundp 'buffer-file-coding-system)
  (boundp 'default-buffer-file-coding-system)
  (boundp 'file-name-coding-system))"#,
        expect,
    );
}

#[test]
fn divergence_detection_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'detect-coding-region)
  (fboundp 'detect-coding-with-priority)
  (fboundp 'find-operation-coding-system))"#,
        expect,
    );
}
