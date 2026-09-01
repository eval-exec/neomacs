//! Oracle parity for regex replace + match-data operations.
//! GNU src/search.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_regexp_quote_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\\\\.world\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(regexp-quote "hello.world")"#, expect);
    assert_ok_eq("\"hello\\\\.world\"", &o, &n);
}

#[test]
fn oracle_regexp_quote_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(stringp (regexp-quote "a*b+c?d[e]f"))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_match_beginning_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "cd" "abcdef") (match-beginning 0))"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_match_end_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "cd" "abcdef") (match-end 0))"#,
        expect,
    );
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_match_data_has_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "foo" "foobar") (consp (match-data)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_set_match_data_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "xyz" "abcxyzdef") (let ((saved (match-data))) (set-match-data saved) (equal saved (match-data))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_replace_match_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello earth\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*rm2*")) (erase-buffer) (insert "hello world") (goto-char 1) (search-forward "world" nil t) (replace-match "earth" t t) (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"hello earth\"", &o, &n);
}

#[test]
fn oracle_looking_at_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*la*")) (erase-buffer) (insert "hello") (goto-char 1) (looking-at "hel"))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_looking_at_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*la2*")) (erase-buffer) (insert "hello") (goto-char 1) (looking-at "xyz"))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}
