//! Deep combo: string manipulation + case conversion + comparison + multibyte.
//! Tests string operations with case folding, locale, and Unicode.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_upcase_downcase_buffer_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (\"HELLO WORLD\" \"hello world\" \"Hello World Foo\" \"Hello World Foo\")""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (upcase \"hello world\")\n\
         (downcase \"HELLO WORLD\")\n\
         (upcase-initials \"hello world foo\")\n\
         (capitalize \"hello world foo\")))", expect);
}

#[test]
fn deficiency_string_equal_case_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (nil t t -1)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (string-equal \"Hello\" \"hello\")\n\
         (string-equal-ignore-case \"Hello\" \"hello\")\n\
         (compare-strings \"Hello\" 0 nil \"hello\" 0 nil t)\n\
         (compare-strings \"Hello\" 0 nil \"hello\" 0 nil nil)))", expect);
}

#[test]
fn deficiency_string_collation_lessp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (string-lessp \"abc\" \"abd\")\n\
         (string-lessp \"abc\" \"abc\")\n\
         (string-version-lessp \"file2\" \"file10\")\n\
         (string-version-lessp \"file10\" \"file2\")))", expect);
}

#[test]
fn deficiency_string_pad_truncate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (\"hi        \" \"hi--------\" \"hello\" \"hello wo\" \"hello w…\")""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (string-pad \"hi\" 10)\n\
         (string-pad \"hi\" 10 ?-)\n\
         (string-pad \"hello\" 3)\n\
         (truncate-string-to-width \"hello world\" 8)\n\
         (truncate-string-to-width \"hello world\" 8 nil nil t)))", expect);
}

#[test]
fn deficiency_string_replace_in_region_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (\"FOO bar FOO baz FOO\" \" bar  baz \" \"X bar X baz X\")""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"foo bar foo baz foo\"))\n\
         (list (string-replace \"foo\" \"FOO\" s)\n\
         (string-replace \"foo\" \"\" s)\n\
         (replace-regexp-in-string \"fo+\" \"X\" s))))", expect);
}

#[test]
fn deficiency_string_split_trim_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK ((\"a\" \"b\" \"c\") (\"a\" \"b\" \"\" \"c\") (\"a\" \"b\" \"c\") \"hello\" \"hello\" \"hello\")""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (split-string \"  a  b  c  \" \"\\\\s-+\" t)\n\
         (split-string \"a,b,,c\" \",\")\n\
         (split-string \"a,b,,c\" \",\" t)\n\
         (string-trim \"  hello  \")\n\
         (string-trim-left \"xxxhello\" \"x+\")\n\
         (string-trim-right \"helloxxx\" \"x+\")))", expect);
}

#[test]
fn deficiency_string_multibyte_case_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (\"CAFÉ\" \"café\" \"Hello Café World\" \"Hello Café World\")""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (upcase \"caf\\u00e9\")\n\
         (downcase \"CAF\\u00c9\")\n\
         (capitalize \"hello caf\\u00e9 world\")\n\
         (upcase-initials \"hello caf\\u00e9 world\")))", expect);
}

#[test]
fn deficiency_string_search_from_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (0 3 3 6 nil nil)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"abcabcabc\"))\n\
         (list (string-search \"abc\" s)\n\
         (string-search \"abc\" s 1)\n\
         (string-search \"abc\" s 3)\n\
         (string-search \"abc\" s 6)\n\
         (string-search \"abc\" s 7)\n\
         (string-search \"xyz\" s))))", expect);
}

#[test]
fn deficiency_string_reverse_and_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (\"olleh\" \"abc\" \"abc\" 4 \"ABC\")""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (reverse \"hello\")\n\
         (string-to-multibyte \"abc\")\n\
         (string-to-unibyte \"abc\")\n\
         (length (string-to-list \"abc\\u00e9\"))\n\
         (apply 'string (string-to-list \"ABC\"))))", expect);
}

#[test]
fn deficiency_string_format_with_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (\"symbol (a b c)\" \"3.140\" \"1010\" \"2305843009213693951\" \"%d=42\")""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (format \"%s %S\" 'symbol '(a b c))\n\
         (format \"%.3f\" 3.14)\n\
         (format \"%b\" 10)\n\
         (format \"%d\" most-positive-fixnum)\n\
         (format \"%%d=%d\" 42)))", expect);
}
