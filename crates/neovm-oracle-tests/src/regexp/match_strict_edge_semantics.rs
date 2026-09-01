//! Oracle parity tests for `string-match`, `match-data`, `match-string`
//! and `looking-at` — strict edge cases.
//!
//! GNU src/search.c: regexp matching has subtle behavior around match
//! data lifecycle, zero-length matches, and case-fold semantics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_string_match_finds_at_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match "foo" "foobar")"#, expect);
    assert_ok_eq("0", &oracle, &neovm);
}

#[test]
fn oracle_string_match_finds_later() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match "bar" "foobar")"#, expect);
    assert_ok_eq("3", &oracle, &neovm);
}

#[test]
fn oracle_string_match_no_match_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match "xyz" "hello")"#, expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_match_data_after_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    // match-data returns a list of markers/positions
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (string-match "b[cd]" "abcdef")
  (consp (match-data)))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_match_beginning_after_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (string-match "cde" "abcdef")
  (match-beginning 0))"#,
        expect,
    );
    assert_ok_eq("2", &oracle, &neovm);
}

#[test]
fn oracle_match_end_after_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (string-match "cde" "abcdef")
  (match-end 0))"#,
        expect,
    );
    assert_ok_eq("5", &oracle, &neovm);
}

#[test]
fn oracle_looking_at_matches_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-lookat*"))
  (erase-buffer)
  (insert "hello world")
  (goto-char 1)
  (looking-at "hello"))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_looking_at_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-lookno*"))
  (erase-buffer)
  (insert "hello world")
  (goto-char 1)
  (looking-at "xyz"))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_string_match_wrong_type_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match 42 "foo")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
