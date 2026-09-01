//! Per-coding-system *coding-system-plist* matrix (all GNU coding systems).
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cod_plist_adobe_standard_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name adobe-standard-encoding :docstring \"Adobe `standard' encoding for PostScript\" :coding-type charset :mnemonic 42 :charset-list (adobe-standard-encoding) :mime-charset adobe-standard-encoding)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'adobe-standard-encoding)",
        expect,
    );
}

#[test]
fn div_cod_plist_chinese_big5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-big5 :name chinese-big5 :docstring \"BIG5 8-bit encoding for Chinese (MIME:Big5)\" :coding-type big5 :mnemonic 66 :charset-list (ascii big5) :mime-charset big5)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'chinese-big5)", expect);
}

#[test]
fn div_cod_plist_chinese_big5_hkscs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name chinese-big5-hkscs :docstring \"BIG5-HKSCS 8-bit encoding for Chinese, Hong Kong supplement (MIME:Big5-HKSCS)\" :coding-type charset :mnemonic 66 :charset-list (ascii big5-hkscs) :mime-charset big5-hkscs)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'chinese-big5-hkscs)", expect);
}

#[test]
fn div_cod_plist_chinese_gb18030() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name chinese-gb18030 :docstring \"GB18030 encoding for Chinese (MIME:GB18030).\" :coding-type charset :mnemonic 99 :charset-list (ascii gb18030-2-byte gb18030-4-byte-bmp gb18030-4-byte-smp gb18030-4-byte-ext-1 gb18030-4-byte-ext-2) :mime-charset gb18030)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'chinese-gb18030)", expect);
}

#[test]
fn div_cod_plist_chinese_gbk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name chinese-gbk :docstring \"GBK encoding for Chinese (MIME:GBK).\" :coding-type charset :mnemonic 99 :charset-list (ascii chinese-gbk) :mime-charset gbk)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'chinese-gbk)", expect);
}

#[test]
fn div_cod_plist_chinese_hz() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-8 :name chinese-hz :docstring \"Hz/ZW 7-bit encoding for Chinese GB2312 (MIME:HZ-GB-2312).\" :coding-type utf-8 :mnemonic 122 :charset-list (ascii chinese-gb2312) :mime-charset hz-gb-2312 :post-read-conversion post-read-decode-hz :pre-write-conversion pre-write-encode-hz)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'chinese-hz)", expect);
}

#[test]
fn div_cod_plist_chinese_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-2 :name chinese-iso-8bit :docstring \"ISO 2022 based EUC encoding for Chinese GB2312 (MIME:GB2312).\" :coding-type iso-2022 :mnemonic 99 :charset-list (ascii chinese-gb2312) :designation [ascii chinese-gb2312 nil nil] :mime-charset gb2312)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'chinese-iso-8bit)", expect);
}

#[test]
fn div_cod_plist_compound_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-8-else :name compound-text :docstring \"Compound text based generic encoding.\\nThis coding system is an extension of X's \\\"Compound Text Encoding\\\".\\nIt encodes many characters using the normal ISO-2022 designation sequences,\\nbut it doesn't support extended segments of CTEXT.\" :coding-type iso-2022 :mnemonic 120 :charset-list iso-2022 :designation [(ascii 94) (latin-iso8859-1 katakana-jisx0201 96) nil nil] :flags (ascii-at-eol ascii-at-cntl long-form designation locking-shift single-shift composition) :mime-charset x-ctext)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'compound-text)", expect);
}

#[test]
fn div_cod_plist_compound_text_with_extensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-8-else :name compound-text-with-extensions :docstring \"Compound text encoding with ICCCM Extended Segment extensions.\\n\\nSee the variables `ctext-standard-encodings' and\\n`ctext-non-standard-encodings-alist' for the detail about how\\nextended segments are handled.\\n\\nThis coding system should be used only for X selections.  It is inappropriate\\nfor decoding and encoding files, process I/O, etc.\" :coding-type iso-2022 :mnemonic 120 :charset-list iso-2022 :designation [(ascii 94) (latin-iso8859-1 katakana-jisx0201 96) nil nil] :flags (ascii-at-eol ascii-at-cntl long-form designation locking-shift single-shift) :post-read-conversion ctext-post-read-conversion :pre-write-conversion ctext-pre-write-conversion :mime-charset x-ctext)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'compound-text-with-extensions)",
        expect,
    );
}

#[test]
fn div_cod_plist_cp1125() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp1125 :docstring \"cp1125 8-bit encoding for Cyrillic\" :coding-type charset :mnemonic 42 :charset-list (cp1125))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp1125)", expect);
}

#[test]
fn div_cod_plist_cp437() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp437 :docstring \"DOS codepage 437\" :coding-type charset :mnemonic 68 :charset-list (cp437) :mime-charset cp437)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp437)", expect);
}

#[test]
fn div_cod_plist_cp737() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp737 :docstring \"Codepage 737 (PC Greek)\" :coding-type charset :mnemonic 68 :charset-list (cp737) :mime-charset cp737)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp737)", expect);
}

#[test]
fn div_cod_plist_cp775() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp775 :docstring \"DOS codepage 775 (PC Baltic, MS-DOS Baltic Rim)\" :coding-type charset :mnemonic 68 :charset-list (cp775) :mime-charset cp775)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp775)", expect);
}

#[test]
fn div_cod_plist_cp850() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp850 :docstring \"DOS codepage 850 (Western European)\" :coding-type charset :mnemonic 68 :charset-list (cp850) :mime-charset cp850)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp850)", expect);
}

#[test]
fn div_cod_plist_cp851() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp851 :docstring \"DOS codepage 851 (Greek)\" :coding-type charset :mnemonic 68 :charset-list (cp851) :mime-charset cp851)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp851)", expect);
}

#[test]
fn div_cod_plist_cp852() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp852 :docstring \"DOS codepage 852 (Slavic)\" :coding-type charset :mnemonic 68 :charset-list (cp852) :mime-charset cp852)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp852)", expect);
}

#[test]
fn div_cod_plist_cp855() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp855 :docstring \"DOS codepage 855 (Russian)\" :coding-type charset :mnemonic 68 :charset-list (cp855) :mime-charset cp855)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp855)", expect);
}

#[test]
fn div_cod_plist_cp857() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp857 :docstring \"DOS codepage 857 (Turkish)\" :coding-type charset :mnemonic 68 :charset-list (cp857) :mime-charset cp857)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp857)", expect);
}

#[test]
fn div_cod_plist_cp858() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp858 :docstring \"Codepage 858 (Multilingual Latin I + Euro)\" :coding-type charset :mnemonic 68 :charset-list (cp858) :mime-charset cp858)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp858)", expect);
}

#[test]
fn div_cod_plist_cp860() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp860 :docstring \"DOS codepage 860 (Portuguese)\" :coding-type charset :mnemonic 68 :charset-list (cp860) :mime-charset cp860)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp860)", expect);
}

#[test]
fn div_cod_plist_cp861() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp861 :docstring \"DOS codepage 861 (Icelandic)\" :coding-type charset :mnemonic 68 :charset-list (cp861) :mime-charset cp861)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp861)", expect);
}

#[test]
fn div_cod_plist_cp862() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp862 :docstring \"DOS codepage 862 (Hebrew)\" :coding-type charset :mnemonic 68 :charset-list (cp862) :mime-charset cp862)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp862)", expect);
}

#[test]
fn div_cod_plist_cp863() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp863 :docstring \"DOS codepage 863 (French Canadian)\" :coding-type charset :mnemonic 68 :charset-list (cp863) :mime-charset cp863)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp863)", expect);
}

#[test]
fn div_cod_plist_cp865() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp865 :docstring \"DOS codepage 865 (Norwegian/Danish)\" :coding-type charset :mnemonic 68 :charset-list (cp865) :mime-charset cp865)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp865)", expect);
}

#[test]
fn div_cod_plist_cp866() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp866 :docstring \"CP866 encoding for Cyrillic.\" :coding-type charset :mnemonic 42 :charset-list (ibm866) :mime-charset cp866)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp866)", expect);
}

#[test]
fn div_cod_plist_cp869() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp869 :docstring \"DOS codepage 869 (Greek)\" :coding-type charset :mnemonic 68 :charset-list (cp869) :mime-charset cp869)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp869)", expect);
}

#[test]
fn div_cod_plist_cp874() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cp874 :docstring \"DOS codepage 874 (Thai)\" :coding-type charset :mnemonic 68 :charset-list (cp874) :mime-charset cp874)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cp874)", expect);
}

#[test]
fn div_cod_plist_ctext_no_compositions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-8-else :name ctext-no-compositions :docstring \"Compound text based generic encoding.\\n\\nLike `compound-text', but does not produce escape sequences for compositions.\" :coding-type iso-2022 :mnemonic 120 :charset-list iso-2022 :designation [(ascii 94) (latin-iso8859-1 katakana-jisx0201 96) nil nil] :flags (ascii-at-eol ascii-at-cntl designation locking-shift single-shift))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'ctext-no-compositions)",
        expect,
    );
}

#[test]
fn div_cod_plist_cyrillic_alternativnyj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cyrillic-alternativnyj :docstring \"ALTERNATIVNYJ 8-bit encoding for Cyrillic.\" :coding-type charset :mnemonic 65 :charset-list (alternativnyj))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'cyrillic-alternativnyj)",
        expect,
    );
}

#[test]
fn div_cod_plist_cyrillic_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cyrillic-iso-8bit :docstring \"ISO 2022 based 8-bit encoding for Cyrillic script (MIME:ISO-8859-5).\" :coding-type charset :mnemonic 53 :charset-list (iso-8859-5) :mime-charset iso-8859-5)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cyrillic-iso-8bit)", expect);
}

#[test]
fn div_cod_plist_cyrillic_koi8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name cyrillic-koi8 :docstring \"KOI8 8-bit encoding for Cyrillic (MIME: KOI8-R).\" :coding-type charset :mnemonic 82 :charset-list (koi8) :mime-charset koi8-r)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'cyrillic-koi8)", expect);
}

#[test]
fn div_cod_plist_ebcdic_uk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ebcdic-uk :docstring \"UK version of EBCDIC\" :coding-type charset :charset-list (ebcdic-uk) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ebcdic-uk)", expect);
}

#[test]
fn div_cod_plist_ebcdic_us() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ebcdic-us :docstring \"US version of EBCDIC\" :coding-type charset :charset-list (ebcdic-us) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ebcdic-us)", expect);
}

#[test]
fn div_cod_plist_emacs_mule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-emacs-mule :name emacs-mule :docstring \"Emacs 21 internal format used in buffer and string.\" :coding-type emacs-mule :charset-list emacs-mule :mnemonic 77)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'emacs-mule)", expect);
}

#[test]
fn div_cod_plist_euc_jis_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-2 :name euc-jis-2004 :docstring \"ISO 2022 based EUC encoding for JIS X 0213 (MIME:EUC-JIS-2004).\" :coding-type iso-2022 :mnemonic 69 :designation [ascii japanese-jisx0213.2004-1 katakana-jisx0201 japanese-jisx0213-2] :flags (short ascii-at-eol ascii-at-cntl single-shift) :charset-list (ascii latin-jisx0201 japanese-jisx0213.2004-1 japanese-jisx0213-1 katakana-jisx0201 japanese-jisx0213-2) :mime-charset euc-jis-2004)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'euc-jis-2004)", expect);
}

#[test]
fn div_cod_plist_euc_tw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-2 :name euc-tw :docstring \"ISO 2022 based EUC encoding for Chinese CNS11643.\" :coding-type iso-2022 :mnemonic 90 :charset-list (ascii chinese-cns11643-1 chinese-cns11643-2 chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7) :designation [ascii chinese-cns11643-1 (chinese-cns11643-1 chinese-cns11643-2 chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7) nil] :mime-charset euc-tw)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'euc-tw)", expect);
}

#[test]
fn div_cod_plist_eucjp_ms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-2 :name eucjp-ms :docstring \"eucJP-ms (like EUC-JP but with CP932 extension).\\neucJP-ms is defined in <http://www.opengroup.or.jp/jvc/cde/appendix.html>.\" :coding-type iso-2022 :mnemonic 69 :designation [ascii japanese-jisx0208 katakana-jisx0201 japanese-jisx0212] :flags (short ascii-at-eol ascii-at-cntl single-shift) :charset-list (ascii latin-jisx0201 japanese-jisx0208 katakana-jisx0201 japanese-jisx0212) :decode-translation-table eucjp-ms-decode :encode-translation-table eucjp-ms-encode)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'eucjp-ms)", expect);
}

#[test]
fn div_cod_plist_georgian_academy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name georgian-academy :docstring \"Georgian Academy encoding\" :coding-type charset :mnemonic 71 :charset-list (georgian-academy))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'georgian-academy)", expect);
}

#[test]
fn div_cod_plist_georgian_ps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name georgian-ps :docstring \"Georgian PS encoding\" :coding-type charset :mnemonic 71 :charset-list (georgian-ps))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'georgian-ps)", expect);
}

#[test]
fn div_cod_plist_greek_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name greek-iso-8bit :docstring \"ISO 2022 based 8-bit encoding for Greek (MIME:ISO-8859-7).\" :coding-type charset :mnemonic 55 :charset-list (iso-8859-7) :mime-charset iso-8859-7)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'greek-iso-8bit)", expect);
}

#[test]
fn div_cod_plist_hebrew_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name hebrew-iso-8bit :docstring \"ISO 2022 based 8-bit encoding for Hebrew (MIME:ISO-8859-8).\" :coding-type charset :mnemonic 56 :charset-list (iso-8859-8) :mime-charset iso-8859-8)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'hebrew-iso-8bit)", expect);
}

#[test]
fn div_cod_plist_hp_roman8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name hp-roman8 :docstring \"Hewlet-Packard roman-8 encoding (MIME:ROMAN-8)\" :coding-type charset :mnemonic 42 :charset-list (hp-roman8) :mime-charset hp-roman8)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'hp-roman8)", expect);
}

#[test]
fn div_cod_plist_ibm038() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm038 :docstring \"International version of EBCDIC\" :coding-type charset :charset-list (ibm038) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm038)", expect);
}

#[test]
fn div_cod_plist_ibm1047() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm1047 :docstring \"A version of EBCDIC used in OS/390 Unix\" :coding-type charset :charset-list (ibm1047) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm1047)", expect);
}

#[test]
fn div_cod_plist_ibm256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm256 :docstring \"Netherlands version of EBCDIC\" :coding-type charset :charset-list (ibm256) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm256)", expect);
}

#[test]
fn div_cod_plist_ibm273() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm273 :docstring \"Austrian / German version of EBCDIC\" :coding-type charset :charset-list (ibm273) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm273)", expect);
}

#[test]
fn div_cod_plist_ibm274() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm274 :docstring \"Belgian version of EBCDIC\" :coding-type charset :charset-list (ibm274) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm274)", expect);
}

#[test]
fn div_cod_plist_ibm275() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm275 :docstring \"Brazilian version of EBCDIC\" :coding-type charset :charset-list (ibm275) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm275)", expect);
}

#[test]
fn div_cod_plist_ibm277() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm277 :docstring \"Danish / Norwegian version of EBCDIC\" :coding-type charset :charset-list (ibm277) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm277)", expect);
}

#[test]
fn div_cod_plist_ibm278() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm278 :docstring \"Finnish / Swedish version of EBCDIC\" :coding-type charset :charset-list (ibm278) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm278)", expect);
}

#[test]
fn div_cod_plist_ibm280() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm280 :docstring \"Italian version of EBCDIC\" :coding-type charset :charset-list (ibm280) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm280)", expect);
}

#[test]
fn div_cod_plist_ibm281() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm281 :docstring \"Japanese-E version of EBCDIC\" :coding-type charset :charset-list (ibm281) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm281)", expect);
}

#[test]
fn div_cod_plist_ibm284() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm284 :docstring \"Spanish version of EBCDIC\" :coding-type charset :charset-list (ibm284) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm284)", expect);
}

#[test]
fn div_cod_plist_ibm285() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm285 :docstring \"UK English version of EBCDIC\" :coding-type charset :charset-list (ibm285) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm285)", expect);
}

#[test]
fn div_cod_plist_ibm290() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm290 :docstring \"Japanese katakana version of EBCDIC\" :coding-type charset :charset-list (ibm290) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm290)", expect);
}

#[test]
fn div_cod_plist_ibm297() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name ibm297 :docstring \"French version of EBCDIC\" :coding-type charset :charset-list (ibm297) :mnemonic 42)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'ibm297)", expect);
}

#[test]
fn div_cod_plist_in_is13194_devanagari() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-1 :name in-is13194-devanagari :docstring \"8-bit encoding for ASCII (MSB=0) and IS13194-Devanagari (MSB=1).\" :coding-type iso-2022 :mnemonic 68 :designation [ascii indian-is13194 nil nil] :charset-list (ascii indian-is13194) :post-read-conversion in-is13194-post-read-conversion :pre-write-conversion in-is13194-pre-write-conversion)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'in-is13194-devanagari)",
        expect,
    );
}

#[test]
fn div_cod_plist_iso_2022_7bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7 :name iso-2022-7bit :docstring \"ISO 2022 based 7-bit encoding using only G0.\" :coding-type iso-2022 :mnemonic 74 :charset-list iso-2022 :designation [(ascii t) nil nil nil] :flags (short ascii-at-eol ascii-at-cntl 7-bit designation composition))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-7bit)", expect);
}

#[test]
fn div_cod_plist_iso_2022_7bit_lock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-else :name iso-2022-7bit-lock :docstring \"ISO-2022 coding system using Locking-Shift for 96-charset.\" :coding-type iso-2022 :mnemonic 38 :charset-list iso-2022 :designation [(ascii 94) (nil 96) nil nil] :flags (ascii-at-eol ascii-at-cntl 7-bit designation locking-shift composition))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-7bit-lock)", expect);
}

#[test]
fn div_cod_plist_iso_2022_7bit_lock_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-else :name iso-2022-7bit-lock-ss2 :docstring \"Mixture of ISO-2022-JP, ISO-2022-KR, and ISO-2022-CN.\" :coding-type iso-2022 :mnemonic 105 :charset-list (ascii japanese-jisx0208 japanese-jisx0208-1978 latin-jisx0201 korean-ksc5601 chinese-gb2312 chinese-cns11643-1 chinese-cns11643-2 chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7) :designation [(ascii 94) (nil korean-ksc5601 chinese-gb2312 chinese-cns11643-1 96) (nil chinese-cns11643-2) (nil chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7)] :flags (short ascii-at-eol ascii-at-cntl 7-bit locking-shift single-shift init-bol))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'iso-2022-7bit-lock-ss2)",
        expect,
    );
}

#[test]
fn div_cod_plist_iso_2022_7bit_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-else :name iso-2022-7bit-ss2 :docstring \"ISO 2022 based 7-bit encoding using SS2 for 96-charset.\" :coding-type iso-2022 :mnemonic 36 :charset-list iso-2022 :designation [(ascii 94) nil (nil 96) nil] :flags (short ascii-at-eol ascii-at-cntl 7-bit designation single-shift composition))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-7bit-ss2)", expect);
}

#[test]
fn div_cod_plist_iso_2022_8bit_ss2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-8-else :name iso-2022-8bit-ss2 :docstring \"ISO 2022 based 8-bit encoding using SS2 for 96-charset.\" :coding-type iso-2022 :mnemonic 64 :charset-list iso-2022 :designation [(ascii 94) nil (nil 96) nil] :flags (ascii-at-eol ascii-at-cntl designation single-shift composition))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-8bit-ss2)", expect);
}

#[test]
fn div_cod_plist_iso_2022_cn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-else :name iso-2022-cn :docstring \"ISO 2022 based 7bit encoding for Chinese GB and CNS (MIME:ISO-2022-CN).\" :coding-type iso-2022 :mnemonic 67 :charset-list (ascii chinese-gb2312 chinese-cns11643-1 chinese-cns11643-2) :designation [ascii (nil chinese-gb2312 chinese-cns11643-1) (nil chinese-cns11643-2) nil] :flags (ascii-at-eol ascii-at-cntl 7-bit designation locking-shift single-shift init-at-bol) :mime-charset iso-2022-cn :suitable-for-keyboard t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-cn)", expect);
}

#[test]
fn div_cod_plist_iso_2022_cn_ext() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-else :name iso-2022-cn-ext :docstring \"ISO 2022 based 7bit encoding for Chinese GB and CNS (MIME:ISO-2022-CN-EXT).\" :coding-type iso-2022 :mnemonic 67 :charset-list (ascii chinese-gb2312 chinese-cns11643-1 chinese-cns11643-2 chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7) :designation [ascii (nil chinese-gb2312 chinese-cns11643-1) (nil chinese-cns11643-2) (nil chinese-cns11643-3 chinese-cns11643-4 chinese-cns11643-5 chinese-cns11643-6 chinese-cns11643-7)] :flags (ascii-at-eol ascii-at-cntl 7-bit designation locking-shift single-shift init-at-bol) :mime-charset iso-2022-cn-ext :suitable-for-keyboard t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-cn-ext)", expect);
}

#[test]
fn div_cod_plist_iso_2022_jp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-tight :name iso-2022-jp :docstring \"ISO 2022 based 7bit encoding for Japanese (MIME:ISO-2022-JP).\" :coding-type iso-2022 :mnemonic 74 :designation [(ascii japanese-jisx0208-1978 japanese-jisx0208 latin-jisx0201) nil nil nil] :flags (short ascii-at-eol ascii-at-cntl 7-bit designation) :charset-list (ascii japanese-jisx0208 japanese-jisx0208-1978 latin-jisx0201) :mime-charset iso-2022-jp :suitable-for-keyboard t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-jp)", expect);
}

#[test]
fn div_cod_plist_iso_2022_jp_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-else :name iso-2022-jp-2 :docstring \"ISO 2022 based 7bit encoding for CJK, Latin-1, Greek (MIME:ISO-2022-JP-2).\" :coding-type iso-2022 :mnemonic 74 :designation [(ascii japanese-jisx0208-1978 japanese-jisx0208 latin-jisx0201 japanese-jisx0212 chinese-gb2312 korean-ksc5601) nil (nil latin-iso8859-1 greek-iso8859-7) nil] :flags (short ascii-at-eol ascii-at-cntl 7-bit designation single-shift init-at-bol) :charset-list (ascii japanese-jisx0208 japanese-jisx0212 latin-jisx0201 japanese-jisx0208-1978 chinese-gb2312 korean-ksc5601 latin-iso8859-1 greek-iso8859-7) :mime-charset iso-2022-jp-2 :suitable-for-keyboard t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-jp-2)", expect);
}

#[test]
fn div_cod_plist_iso_2022_jp_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-tight :name iso-2022-jp-2004 :docstring \"ISO 2022 based 7bit encoding for JIS X 0213:2004 (MIME:ISO-2022-JP-2004).\" :coding-type iso-2022 :mnemonic 74 :designation [(ascii japanese-jisx0208 japanese-jisx0213.2004-1 japanese-jisx0213-1 japanese-jisx0213-2) nil nil nil] :flags (short ascii-at-eol ascii-at-cntl 7-bit designation) :charset-list (ascii japanese-jisx0208 japanese-jisx0213.2004-1 japanese-jisx0213-1 japanese-jisx0213-2) :mime-charset iso-2022-jp-2004 :suitable-for-keyboard t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-jp-2004)", expect);
}

#[test]
fn div_cod_plist_iso_2022_kr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-else :name iso-2022-kr :docstring \"ISO 2022 based 7-bit encoding for Korean KSC5601 (MIME:ISO-2022-KR).\" :coding-type iso-2022 :mnemonic 107 :designation [ascii (nil korean-ksc5601) nil nil] :flags (ascii-at-eol ascii-at-cntl 7-bit designation locking-shift designation-bol) :charset-list (ascii korean-ksc5601) :mime-charset iso-2022-kr :suitable-for-keyboard t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-2022-kr)", expect);
}

#[test]
fn div_cod_plist_iso_8859_11() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-8859-11 :docstring \"ISO/IEC 8859/11 (Latin/Thai)\\nThis is the same as `thai-tis620' with the addition of no-break-space.\" :coding-type charset :mnemonic 42 :mime-charset iso-8859-11 :charset-list (iso-8859-11))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-8859-11)", expect);
}

#[test]
fn div_cod_plist_iso_8859_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-8859-6 :docstring \"ISO-8859-6 based encoding (MIME:ISO-8859-6).\" :coding-type charset :mnemonic 54 :charset-list (iso-8859-6) :mime-charset iso-8859-6)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-8859-6)", expect);
}

#[test]
fn div_cod_plist_iso_latin_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-1 :docstring \"ISO 2022 based 8-bit encoding for Latin-1 (MIME:ISO-8859-1).\" :coding-type charset :mnemonic 49 :charset-list (iso-8859-1) :mime-charset iso-8859-1)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-1)", expect);
}

#[test]
fn div_cod_plist_iso_latin_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-10 :docstring \"ISO 2022 based 8-bit encoding for Latin-10.\" :coding-type charset :mnemonic 42 :charset-list (iso-8859-16) :mime-charset iso-8859-16)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-10)", expect);
}

#[test]
fn div_cod_plist_iso_latin_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-2 :docstring \"ISO 2022 based 8-bit encoding for Latin-2 (MIME:ISO-8859-2).\" :coding-type charset :mnemonic 50 :charset-list (iso-8859-2) :mime-charset iso-8859-2)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-2)", expect);
}

#[test]
fn div_cod_plist_iso_latin_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-3 :docstring \"ISO 2022 based 8-bit encoding for Latin-3 (MIME:ISO-8859-3).\" :coding-type charset :mnemonic 51 :charset-list (iso-8859-3) :mime-charset iso-8859-3)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-3)", expect);
}

#[test]
fn div_cod_plist_iso_latin_4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-4 :docstring \"ISO 2022 based 8-bit encoding for Latin-4 (MIME:ISO-8859-4).\" :coding-type charset :mnemonic 52 :charset-list (iso-8859-4) :mime-charset iso-8859-4)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-4)", expect);
}

#[test]
fn div_cod_plist_iso_latin_5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-5 :docstring \"ISO 2022 based 8-bit encoding for Latin-5 (MIME:ISO-8859-9).\" :coding-type charset :mnemonic 57 :charset-list (iso-8859-9) :mime-charset iso-8859-9)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-5)", expect);
}

#[test]
fn div_cod_plist_iso_latin_6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-6 :docstring \"ISO 2022 based 8-bit encoding for Latin-6 (MIME:ISO-8859-10).\" :coding-type charset :mnemonic 57 :charset-list (iso-8859-10) :mime-charset iso-8859-10)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-6)", expect);
}

#[test]
fn div_cod_plist_iso_latin_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-7 :docstring \"ISO 2022 based 8-bit encoding for Latin-7 (MIME:ISO-8859-13).\" :coding-type charset :mnemonic 57 :charset-list (iso-8859-13) :mime-charset iso-8859-13)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-7)", expect);
}

#[test]
fn div_cod_plist_iso_latin_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-8 :docstring \"ISO 2022 based 8-bit encoding for Latin-8 (MIME:ISO-8859-14).\" :coding-type charset :mnemonic 87 :charset-list (iso-8859-14) :mime-charset iso-8859-14)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-8)", expect);
}

#[test]
fn div_cod_plist_iso_latin_9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name iso-latin-9 :docstring \"ISO 2022 based 8-bit encoding for Latin-9 (MIME:ISO-8859-15).\" :coding-type charset :mnemonic 48 :charset-list (iso-8859-15) :mime-charset iso-8859-15)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'iso-latin-9)", expect);
}

#[test]
fn div_cod_plist_japanese_cp932() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name japanese-cp932 :docstring \"CP932 (Microsoft shift-jis)\" :coding-type charset :mnemonic 83 :charset-list (ascii katakana-sjis cp932-2-byte))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'japanese-cp932)", expect);
}

#[test]
fn div_cod_plist_japanese_iso_7bit_1978_irv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-iso-7-tight :name japanese-iso-7bit-1978-irv :docstring \"ISO 2022 based 7-bit encoding for Japanese JISX0208-1978 and JISX0201-Roman.\" :coding-type iso-2022 :mnemonic 106 :designation [(latin-jisx0201 japanese-jisx0208-1978 japanese-jisx0208 japanese-jisx0212 katakana-jisx0201) nil nil nil] :flags (short ascii-at-eol ascii-at-cntl 7-bit designation use-roman use-oldjis) :charset-list (ascii latin-jisx0201 japanese-jisx0208-1978 japanese-jisx0208 japanese-jisx0212))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'japanese-iso-7bit-1978-irv)",
        expect,
    );
}

#[test]
fn div_cod_plist_japanese_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-2 :name japanese-iso-8bit :docstring \"ISO 2022 based EUC encoding for Japanese (MIME:EUC-JP).\" :coding-type iso-2022 :mnemonic 69 :designation [ascii japanese-jisx0208 katakana-jisx0201 japanese-jisx0212] :flags (short ascii-at-eol ascii-at-cntl single-shift) :charset-list (ascii latin-jisx0201 japanese-jisx0208 katakana-jisx0201 japanese-jisx0212 japanese-jisx0208-1978) :mime-charset euc-jp)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'japanese-iso-8bit)", expect);
}

#[test]
fn div_cod_plist_japanese_shift_jis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-sjis :name japanese-shift-jis :docstring \"Shift-JIS 8-bit encoding for Japanese (MIME:SHIFT_JIS)\" :coding-type shift-jis :mnemonic 83 :charset-list (ascii katakana-jisx0201 japanese-jisx0208) :mime-charset shift_jis)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'japanese-shift-jis)", expect);
}

#[test]
fn div_cod_plist_japanese_shift_jis_2004() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-sjis :name japanese-shift-jis-2004 :docstring \"Shift_JIS 8-bit encoding for Japanese (MIME:SHIFT_JIS-2004)\" :coding-type shift-jis :mnemonic 83 :charset-list (ascii katakana-jisx0201 japanese-jisx0213.2004-1 japanese-jisx0213-2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'japanese-shift-jis-2004)",
        expect,
    );
}

#[test]
fn div_cod_plist_koi8_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name koi8-t :docstring \"KOI8-T 8-bit encoding for Cyrillic\" :coding-type charset :mnemonic 42 :charset-list (koi8-t) :mime-charset koi8-t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'koi8-t)", expect);
}

#[test]
fn div_cod_plist_koi8_u() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name koi8-u :docstring \"KOI8-U 8-bit encoding for Cyrillic (MIME: KOI8-U)\" :coding-type charset :mnemonic 1059 :charset-list (koi8-u) :mime-charset koi8-u)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'koi8-u)", expect);
}

#[test]
fn div_cod_plist_korean_cp949() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name korean-cp949 :docstring \"CP949 (Microsoft Unified Hangul Code)\" :coding-type charset :mnemonic 75 :charset-list (ascii cp949))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'korean-cp949)", expect);
}

#[test]
fn div_cod_plist_korean_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-2 :name korean-iso-8bit :docstring \"ISO 2022 based EUC encoding for Korean KSC5601 (MIME:EUC-KR).\" :coding-type iso-2022 :mnemonic 75 :designation [ascii korean-ksc5601 nil nil] :charset-list (ascii korean-ksc5601) :mime-charset euc-kr)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'korean-iso-8bit)", expect);
}

#[test]
fn div_cod_plist_lao() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name lao :docstring \"8-bit encoding for ASCII (MSB=0) and LAO (MSB=1).\" :coding-type charset :mnemonic 76 :charset-list (lao))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'lao)", expect);
}

#[test]
fn div_cod_plist_mac_roman() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name mac-roman :docstring \"Mac Roman Encoding (MIME:MACINTOSH).\" :coding-type charset :mnemonic 77 :charset-list (mac-roman) :mime-charset macintosh)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'mac-roman)", expect);
}

#[test]
fn div_cod_plist_mik() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name mik :docstring \"Bulgarian DOS codepage\" :coding-type charset :mnemonic 68 :charset-list (mik))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'mik)", expect);
}

#[test]
fn div_cod_plist_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name next :docstring \"NeXTstep encoding\" :coding-type charset :mnemonic 42 :charset-list (next) :mime-charset next)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'next)", expect);
}

#[test]
fn div_cod_plist_no_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-raw-text :name no-conversion :mnemonic 61 :coding-type raw-text :ascii-compatible-p t :default-char 0 :for-unibyte t :docstring \"Do no conversion.\\n\\nWhen you visit a file with this coding, the file is read into a\\nunibyte buffer as is, thus each byte of a file is treated as a\\ncharacter.\" :eol-type unix)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'no-conversion)", expect);
}

#[test]
fn div_cod_plist_no_conversion_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-raw-text :name no-conversion-multibyte :docstring \"Like `no-conversion' but don't read a file into a unibyte buffer.\" :coding-type raw-text :eol-type unix :mnemonic 61)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'no-conversion-multibyte)",
        expect,
    );
}

#[test]
fn div_cod_plist_prefer_utf_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-undecided :name prefer-utf-8 :docstring \"Like `undecided' but prefer UTF-8 when appropriate.\\nOn decoding, if the source contains 8-bit codes and they all\\nare valid UTF-8 sequences, detect the source as UTF-8 encoding\\nregardless of the coding priority.\\nOn encoding, if the source contains non-ASCII characters, encode them\\nby UTF-8.\" :coding-type undecided :mnemonic 45 :charset-list (emacs) :prefer-utf-8 t :inhibit-null-byte-detection 0 :inhibit-iso-escape-detection 0)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'prefer-utf-8)", expect);
}

#[test]
fn div_cod_plist_pt154() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name pt154 :docstring \"ParaType Asian Cyrillic codepage\" :coding-type charset :mnemonic 68 :charset-list (pt154))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'pt154)", expect);
}

#[test]
fn div_cod_plist_raw_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-raw-text :name raw-text :docstring \"Raw text, which means text contains random 8-bit codes.\\nEncoding text with this coding system produces the actual byte\\nsequence of the text in buffers and strings.  An exception is made for\\ncharacters from the `eight-bit' character set.  Each of them is encoded\\ninto a single byte.\\n\\nWhen you visit a file with this coding, the file is read into a\\nunibyte buffer as is (except for EOL format), thus each byte of a file\\nis treated as a character.\" :coding-type raw-text :for-unibyte t :mnemonic 116)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'raw-text)", expect);
}

#[test]
fn div_cod_plist_thai_tis620() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name thai-tis620 :docstring \"8-bit encoding for ASCII (MSB=0) and Thai TIS620 (MSB=1).\" :coding-type charset :mnemonic 84 :charset-list (tis620-2533))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'thai-tis620)", expect);
}

#[test]
fn div_cod_plist_tibetan_iso_8bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-iso-8-2 :name tibetan-iso-8bit :docstring \"8-bit encoding for ASCII (MSB=0) and TIBETAN (MSB=1).\" :coding-type iso-2022 :mnemonic 81 :designation [ascii tibetan nil nil] :charset-list (ascii tibetan))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'tibetan-iso-8bit)", expect);
}

#[test]
fn div_cod_plist_undecided() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-undecided :name undecided :mnemonic 45 :coding-type undecided :ascii-compatible-p t :charset-list (ascii) :for-unibyte nil :docstring \"No conversion on encoding, automatic conversion on decoding.\" :eol-type nil)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'undecided)", expect);
}

#[test]
fn div_cod_plist_us_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name us-ascii :docstring \"Encode ASCII as-is and encode non-ASCII characters to `?'.\" :coding-type charset :mnemonic 45 :charset-list (ascii) :default-char 63 :mime-charset us-ascii)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'us-ascii)", expect);
}

#[test]
fn div_cod_plist_utf_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-16-auto :name utf-16 :docstring \"UTF-16 (detect endian on decoding, use big endian on encoding with BOM).\" :coding-type utf-16 :mnemonic 85 :charset-list (unicode) :bom (utf-16le-with-signature . utf-16be-with-signature) :endian big :mime-text-unsuitable t :mime-charset utf-16)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-16)", expect);
}

#[test]
fn div_cod_plist_utf_16be() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-16-be-nosig :name utf-16be :docstring \"UTF-16BE (big endian, no signature (BOM)).\" :coding-type utf-16 :mnemonic 85 :charset-list (unicode) :endian big :mime-text-unsuitable t :mime-charset utf-16be)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-16be)", expect);
}

#[test]
fn div_cod_plist_utf_16be_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-16-be :name utf-16be-with-signature :docstring \"UTF-16 (big endian, with signature (BOM)).\" :coding-type utf-16 :mnemonic 85 :charset-list (unicode) :bom t :endian big :mime-text-unsuitable t :mime-charset utf-16)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'utf-16be-with-signature)",
        expect,
    );
}

#[test]
fn div_cod_plist_utf_16le() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-16-le-nosig :name utf-16le :docstring \"UTF-16LE (little endian, no signature (BOM)).\" :coding-type utf-16 :mnemonic 85 :charset-list (unicode) :endian little :mime-text-unsuitable t :mime-charset utf-16le)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-16le)", expect);
}

#[test]
fn div_cod_plist_utf_16le_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-16-le :name utf-16le-with-signature :docstring \"UTF-16 (little endian, with signature (BOM)).\" :coding-type utf-16 :mnemonic 85 :charset-list (unicode) :bom t :endian little :mime-text-unsuitable t :mime-charset utf-16)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'utf-16le-with-signature)",
        expect,
    );
}

#[test]
fn div_cod_plist_utf_7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-8 :name utf-7 :docstring \"UTF-7 encoding of Unicode (RFC 2152).\" :coding-type utf-8 :mnemonic 117 :mime-charset utf-7 :charset-list (unicode) :pre-write-conversion utf-7-pre-write-conversion :post-read-conversion utf-7-post-read-conversion)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-7)", expect);
}

#[test]
fn div_cod_plist_utf_7_imap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-8 :name utf-7-imap :docstring \"UTF-7 encoding of Unicode, IMAP version (RFC 2060)\" :coding-type utf-8 :mnemonic 117 :charset-list (unicode) :pre-write-conversion utf-7-imap-pre-write-conversion :post-read-conversion utf-7-imap-post-read-conversion)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-7-imap)", expect);
}

#[test]
fn div_cod_plist_utf_8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-utf-8 :name utf-8 :docstring \"UTF-8 (no signature (BOM))\" :coding-type utf-8 :mnemonic 85 :charset-list (unicode) :mime-charset utf-8)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-8)", expect);
}

#[test]
fn div_cod_plist_utf_8_auto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-8-auto :name utf-8-auto :docstring \"UTF-8 (auto-detect signature (BOM))\" :coding-type utf-8 :mnemonic 85 :charset-list (unicode) :bom (utf-8-with-signature . utf-8))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-8-auto)", expect);
}

#[test]
fn div_cod_plist_utf_8_emacs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-utf-8 :name utf-8-emacs :docstring \"Support for all Emacs characters (including non-Unicode characters).\" :coding-type utf-8 :mnemonic 85 :charset-list (emacs))""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'utf-8-emacs)", expect);
}

#[test]
fn div_cod_plist_utf_8_with_signature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-utf-8-sig :name utf-8-with-signature :docstring \"UTF-8 (with signature (BOM))\" :coding-type utf-8 :mnemonic 85 :charset-list (unicode) :bom t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(coding-system-plist 'utf-8-with-signature)",
        expect,
    );
}

#[test]
fn div_cod_plist_vietnamese_viqr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-utf-8 :name vietnamese-viqr :docstring \"Vietnamese latin transcription (VIQR).\" :coding-type utf-8 :mnemonic 113 :charset-list (ascii viscii) :post-read-conversion viqr-post-read-conversion :pre-write-conversion viqr-pre-write-conversion)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'vietnamese-viqr)", expect);
}

#[test]
fn div_cod_plist_vietnamese_viscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name vietnamese-viscii :docstring \"8-bit encoding for Vietnamese VISCII 1.1 (MIME:VISCII).\" :coding-type charset :mnemonic 86 :charset-list (viscii) :mime-charset viscii :suitable-for-file-name t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'vietnamese-viscii)", expect);
}

#[test]
fn div_cod_plist_vietnamese_vscii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p nil :category coding-category-charset :name vietnamese-vscii :docstring \"8-bit encoding for Vietnamese VSCII-1 (TCVN-5712).\" :coding-type charset :mnemonic 118 :charset-list (vscii) :suitable-for-file-name t)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'vietnamese-vscii)", expect);
}

#[test]
fn div_cod_plist_windows_1250() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1250 :docstring \"windows-1250 (Central European) encoding (MIME: WINDOWS-1250)\" :coding-type charset :mnemonic 42 :charset-list (windows-1250) :mime-charset windows-1250)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1250)", expect);
}

#[test]
fn div_cod_plist_windows_1251() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1251 :docstring \"windows-1251 8-bit encoding for Cyrillic (MIME: WINDOWS-1251)\" :coding-type charset :mnemonic 98 :charset-list (windows-1251) :mime-charset windows-1251)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1251)", expect);
}

#[test]
fn div_cod_plist_windows_1252() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1252 :docstring \"windows-1252 (Western European) encoding (MIME: WINDOWS-1252)\" :coding-type charset :mnemonic 42 :charset-list (windows-1252) :mime-charset windows-1252)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1252)", expect);
}

#[test]
fn div_cod_plist_windows_1253() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1253 :docstring \"windows-1253 encoding for Greek\" :coding-type charset :mnemonic 103 :charset-list (windows-1253) :mime-charset windows-1253)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1253)", expect);
}

#[test]
fn div_cod_plist_windows_1254() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1254 :docstring \"windows-1254 (Turkish) encoding (MIME: WINDOWS-1254)\" :coding-type charset :mnemonic 42 :charset-list (windows-1254) :mime-charset windows-1254)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1254)", expect);
}

#[test]
fn div_cod_plist_windows_1255() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1255 :docstring \"windows-1255 (Hebrew) encoding (MIME: WINDOWS-1255)\" :coding-type charset :mnemonic 104 :charset-list (windows-1255) :mime-charset windows-1255)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1255)", expect);
}

#[test]
fn div_cod_plist_windows_1256() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1256 :docstring \"windows-1256 (Arabic) encoding (MIME: WINDOWS-1256)\" :coding-type charset :mnemonic 65 :charset-list (windows-1256) :mime-charset windows-1256)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1256)", expect);
}

#[test]
fn div_cod_plist_windows_1257() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1257 :docstring \"windows-1257 (Baltic) encoding (MIME: WINDOWS-1257)\" :coding-type charset :mnemonic 42 :charset-list (windows-1257) :mime-charset windows-1257)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1257)", expect);
}

#[test]
fn div_cod_plist_windows_1258() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ascii-compatible-p t :category coding-category-charset :name windows-1258 :docstring \"windows-1258 encoding for Vietnamese (MIME: WINDOWS-1258)\" :coding-type charset :mnemonic 42 :charset-list (windows-1258) :mime-charset windows-1258)""#
    ]];
    crate::common::assert_oracle_parity_expect("(coding-system-plist 'windows-1258)", expect);
}
