//! Divergence tests: string manipulation, substring, split, join deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_string_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"foo\" \"bar\" \"baz\") (\"foo\" \"bar\" \"baz\") (\"foo\" \"bar\" \"baz\") (\"foo\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (split-string "foo bar baz" " ")
  (split-string "foo,bar,baz" ",")
  (split-string "foo  bar  baz" " +")
  (split-string "foo" ",")) "#,
        expect,
    );
}

#[test]
fn divergence_string_split_omit_nulls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((\"foo\" \"bar\" \"baz\") (\"foo\" \"bar\") (\"\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (split-string "foo  bar  baz" " " t)
  (split-string "  foo  bar  " " " t)
  (split-string "" " ")) "#,
        expect,
    );
}

#[test]
fn divergence_string_join_mapconcat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"foo, bar, baz\" \"FOO-BAR\" \"a|b|c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (mapconcat 'identity '("foo" "bar" "baz") ", ")
  (mapconcat (lambda (x) (upcase x)) '("foo" "bar") "-")
  (string-join '("a" "b" "c") "|")) "#,
        expect,
    );
}

#[test]
fn divergence_string_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"bar baz bar quux bar\" \"XXX\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'string-replace)
  (string-replace "foo" "bar" "foo baz foo quux foo")
  (string-replace "a" "X" "aaa")) "#,
        expect,
    );
}

#[test]
fn divergence_string_truncate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"Hello\" \"Hello World\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'truncate-string-to-width)
  (truncate-string-to-width "Hello World" 5)
  (truncate-string-to-width "Hello World" 20)) "#,
        expect,
    );
}

#[test]
fn divergence_string_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"hello     \" \"hello-----\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'string-pad)
  (string-pad "hello" 10)
  (string-pad "hello" 10 ?-)
  (string-pad "hello" 3)) "#,
        expect,
    );
}

#[test]
fn divergence_string_trim() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \"hello\" \"hello  \" \"  hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'string-trim)
  (fboundp 'string-trim-left)
  (fboundp 'string-trim-right)
  (string-trim "  hello  ")
  (string-trim-left "  hello  ")
  (string-trim-right "  hello  ")) "#,
        expect,
    );
}

#[test]
fn divergence_string_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t \"hello\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'string-lines)
  (fboundp 'string-chop-newline)
  (string-chop-newline "hello\n")
  (string-chop-newline "hello")) "#,
        expect,
    );
}

#[test]
fn divergence_case_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"HELLO WORLD\" \"hello world\" \"Hello World\" \"Hello World\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (upcase "hello WORLD")
  (downcase "Hello World")
  (capitalize "hello world")
  (upcase-initials "hello world")) "#,
        expect,
    );
}

#[test]
fn divergence_string_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t \"olleh\" (3 2 1) [3 2 1])""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'string-reverse)
  (fboundp 'reverse)
  (reverse "hello")
  (reverse '(1 2 3))
  (reverse [1 2 3])) "#,
        expect,
    );
}
