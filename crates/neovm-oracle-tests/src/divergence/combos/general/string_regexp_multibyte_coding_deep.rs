//! Deep combo: string operations + regexp + multibyte + encode/decode + property access.
//! Tests string/encoding boundary conditions across character sets.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_string_as_multibyte_unibyte_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 3 6 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"\\xc3\\xa9\\xc3\\xa0\\xc3\\xbc\"))\n\
         (let ((multibyte (string-as-multibyte s))\n\
         (unibyte (string-as-unibyte s)))\n\
         (list (length s)\n\
         (length multibyte)\n\
         (length unibyte)\n\
         (string-equal s multibyte)))))",
        expect,
    );
}

#[test]
fn deficiency_string_make_multibyte_with_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"xe2x82xacxe2x82xa4\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pure-ascii \"hello world\")\n\
         (with-high \\\"\\xe2\\x82\\xac\\xe2\\x82\\xa4\\\"))\n\
         (list (multibyte-string-p pure-ascii)\n\
         (multibyte-string-p with-high)\n\
         (length pure-ascii)\n\
         (length with-high)\n\
         (string-bytes pure-ascii)\n\
         (string-bytes with-high))))",
        expect,
    );
}

#[test]
fn deficiency_encode_decode_coding_string_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 6 3 t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((s \"\\u00e9\\u00e0\\u00fc\")\n\
         (encoded (encode-coding-string s 'utf-8))\n\
         (decoded (decode-coding-string encoded 'utf-8)))\n\
         (list (length s)\n\
         (length encoded)\n\
         (length decoded)\n\
         (string-equal s decoded)\n\
         (multibyte-string-p s)\n\
         (multibyte-string-p encoded))))",
        expect,
    );
}

#[test]
fn deficiency_substring_with_multibyte_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"abc\" \"é\" \"édef\" \"ghi\" 11 13)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"abc\\u00e9def\\u00e0ghi\"))\n\
         (list (substring s 0 3)\n\
         (substring s 3 4)\n\
         (substring s 3 7)\n\
         (substring s -3)\n\
         (length s)\n\
         (string-bytes s))))",
        expect,
    );
}

#[test]
fn deficiency_regexp_with_multibyte_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"hello\" 6 10 \"world\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"hello \\u00e9\\u00e0\\u00fc world\"))\n\
         (list (string-match \"[a-z]+\" s)\n\
         (match-string 0 s)\n\
         (string-match \"\\\\cc\" s)\n\
         (string-match \"world\" s)\n\
         (match-string 0 s))))",
        expect,
    );
}

#[test]
fn deficiency_format_with_multibyte_and_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"%s)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((result (format \\\"%s has %d items: %.2f each\\\"\n\
         \\\"caf\\u00e9\\\" 42 3.14)))\n\
         (list result\n\
         (length result)\n\
         (string-match \\\"42\\\" result)\n\
         (string-match \\\"3\\\\.14\\\" result))))",
        expect,
    );
}

#[test]
fn deficiency_split_string_with_regexp_on_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"alpha\\u00e9beta\\u00e0gamma\\u00fcdelta\"))\n\
         (let ((parts (split-string s \\\"[\\u00e9\\u00e0\\u00fc]\\\")))\n\
         (list parts\n\
         (length parts)\n\
         (nth 0 parts)\n\
         (nth 1 parts)\n\
         (nth 2 parts)))))",
        expect,
    );
}

#[test]
fn deficiency_string_properties_with_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold highlight nil 5 \"hel\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s (propertize \"hello\" 'face 'bold 'mouse 'highlight)))\n\
         (list (get-text-property 0 'face s)\n\
         (get-text-property 0 'mouse s)\n\
         (get-text-property 0 'help-echo s)\n\
         (length s)\n\
         (substring-no-properties s 0 3))))",
        expect,
    );
}

#[test]
fn deficiency_concat_with_mix_of_multibyte_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"-\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a \"ascii\")\n\
         (b \"\\u00e9\\u00e0\")\n\
         (c \"more\"))\n\
         (let ((combined (concat a \\\"-\\\" b \\\"-\\\" c)))\n\
         (list combined\n\
         (length combined)\n\
         (multibyte-string-p combined)\n\
         (substring combined 6 8)))))",
        expect,
    );
}

#[test]
fn deficiency_mapcar_over_string_with_aref_codepoints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"A\\u00e9B\\u00e0C\\u00fc\"))\n\
         (let ((codes (mapcar (lambda (i) (aref s i))\n\
         (number-sequence 0 (1- (length s))))))\n\
         (list codes\n\
         (length codes)\n\
         (cl-loop for c in codes\n\
         when (> c 127)\n\
         collect c))))",
        expect,
    );
}
