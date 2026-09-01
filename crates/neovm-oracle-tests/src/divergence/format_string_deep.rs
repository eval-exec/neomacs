//! Divergence tests: format edge cases, propertize, and string conversion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_number_to_string_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"0\" \"0\" \"2305843009213693951\" \"-2305843009213693952\" \"1.5\" \"-15000000000.0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (number-to-string 0)
  (number-to-string -0)
  (number-to-string most-positive-fixnum)
  (number-to-string most-negative-fixnum)
  (number-to-string 1.5)
  (number-to-string -1.5e10))"#,
        expect,
    );
}

#[test]
fn divergence_string_to_number_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 0 100000.0 0 42 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-to-number "42")
  (string-to-number "0xff")
  (string-to-number "1e5")
  (string-to-number "hello")
  (string-to-number "42abc")
  (string-to-number ""))"#,
        expect,
    );
}

#[test]
fn divergence_format_percent_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"0\" \"-1\" \"+42\" \" 42\" \"ff\" \"10\" \"1010\" \"nil\" \"(a b c)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%d" 0)
  (format "%d" -1)
  (format "%+d" 42)
  (format "% d" 42)
  (format "%x" 255)
  (format "%o" 8)
  (format "%b" 10)
  (format "%s" nil)
  (format "%S" '(a b c)))"#,
        expect,
    );
}

#[test]
fn divergence_format_float_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"0.000000\" \"-0.000000\" \"4\" \"0.0001\" \"100000\" \"0.000000e+00\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%f" 0.0)
  (format "%f" -0.0)
  (format "%.0f" 3.7)
  (format "%g" 0.0001)
  (format "%g" 100000.0)
  (format "%e" 0.0))"#,
        expect,
    );
}

#[test]
fn divergence_format_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"        42\" \"42        \" \"0000000042\" \"   hi\" \"hi   \")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%10d" 42)
  (format "%-10d" 42)
  (format "%010d" 42)
  (format "%5s" "hi")
  (format "%-5s" "hi"))"#,
        expect,
    );
}

#[test]
fn divergence_char_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"A\" \"中\" 72 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (char-to-string ?A)
  (char-to-string ?中)
  (string-to-char "Hello")
  (string-to-char ""))"#,
        expect,
    );
}

#[test]
fn divergence_concat_vs_mapconcat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"abc\" \"\" \"a-b-c\" \"A B\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (concat "a" "b" "c")
  (concat)
  (mapconcat #'identity '("a" "b" "c") "-")
  (mapconcat (lambda (x) (upcase x)) '("a" "b") " "))"#,
        expect,
    );
}

#[test]
fn divergence_string_equals_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function string-version-compare)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string= "" "")
  (string= "abc" "abc")
  (string= "abc" "ABC")
  (string-equal "abc" "abc")
  (string< "" "a")
  (string> "b" "a")
  (string-version-compare "1.2" "1.10"))"#,
        expect,
    );
}

#[test]
fn divergence_string_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function string-reverse)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-reverse "abc")
  (string-reverse "")
  (string-reverse "a"))"#,
        expect,
    );
}

#[test]
fn divergence_string_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"xxxxx\" \"hello\" \"hello\" \"hello\" \"hello\" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-pad "" 5 ?x)
  (string-chop-newline "hello\n")
  (string-chop-newline "hello")
  (string-trim "  hello  ")
  (string-trim-left "  hello")
  (string-trim-right "hello  "))"#,
        expect,
    );
}
