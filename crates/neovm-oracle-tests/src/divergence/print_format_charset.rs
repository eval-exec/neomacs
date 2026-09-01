//! Divergence tests: print, format, charset, and coding edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_format_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"00042\" \"hi        \" \"        hi\" \"%\" \"3.14\" \"1.000000e+03\" \"2305843009213693951\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%05d" 42)
  (format "%-10s" "hi")
  (format "%10s" "hi")
  (format "%%")
  (format "%.2f" 3.14159)
  (format "%e" 1000.0)
  (format "%d" most-positive-fixnum))"#,
        expect,
    );
}

#[test]
fn divergence_format_propertize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function format-propertize)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(format-propertize "hello" 'face 'bold)"#,
        expect,
    );
}

#[test]
fn divergence_prin1_vs_princ() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function princ-to-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "hello \"world\""))
  (list (prin1-to-string s)
        (princ-to-string s)))"#,
        expect,
    );
}

#[test]
fn divergence_print_length_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"(1 2 3 ...)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-length 3))
  (prin1-to-string '(1 2 3 4 5 6 7 8 9 10)))"#,
        expect,
    );
}

#[test]
fn divergence_print_level_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"(a (b ...) e)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-level 2))
  (prin1-to-string '(a (b (c (d))) e)))"#,
        expect,
    );
}

#[test]
fn divergence_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK \"#1=(1 2 #1#)\"""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-circle t)
        (x (list 1 2 3)))
  (setcar (nthcdr 2 x) x)
  (prin1-to-string x))"#,
        expect,
    );
}

#[test]
fn divergence_print_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (\"test-0\" \"#:test-0\")""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-gensym t)
        (gs (gensym "test-")))
  (list (symbol-name gs)
        (prin1-to-string gs)))"#,
        expect,
    );
}

#[test]
fn divergence_charset_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 65 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (charsetp 'ascii)
  (charsetp 'unicode)
  (encode-char ?A 'ascii)
  (decode-char 'ascii 65))"#,
        expect,
    );
}

#[test]
fn divergence_char_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (char-width ?A)
              (char-width ?中)
              (char-width ?ā)
              (char-width ? ))"#,
        expect,
    );
}

#[test]
fn divergence_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 4 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list (string-width "hello")
              (string-width "中文")
              (string-width "ābc"))"#,
        expect,
    );
}

#[test]
fn divergence_truncate_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"abcde\" \"abcde\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (truncate-string-to-width "abcdefghij" 5)
  (truncate-string-to-width "abcdefghij" 5 nil ?…))"#,
        expect,
    );
}
