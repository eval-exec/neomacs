//! Oracle parity for match-data + re-search with subexpression groups.
//! GNU src/search.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_string_match_subexpr_begin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "a\\(b\\)c" "abc") (match-beginning 1))"#,
        expect,
    );
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_string_match_subexpr_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "a\\(b\\)c" "abc") (match-end 1))"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_re_search_forward_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*rsg*")) (erase-buffer) (insert "before123after") (goto-char 1) (re-search-forward "\\([0-9]+\\)" nil t) (match-beginning 1))"#,
        expect,
    );
    assert_ok_eq("7", &o, &n);
}

#[test]
fn oracle_match_data_integrity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "foo" "foobar") (consp (match-data)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_match_data_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "a\\(b\\)c\\(d\\)e" "abcde") (match-data) t)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_set_match_data_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "xyz" "abcxyzdef") (let ((saved (match-data))) (set-match-data saved) (match-beginning 0)))"#,
        expect,
    );
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_string_match_no_match_does_not_affect_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "found" "found-here") (string-match "notfound" "xyz") (match-data) t)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
