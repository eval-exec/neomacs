//! Oracle parity for search, string-match, match-data interaction via binary.
//! Tests multi-group match-string, posix-string-match, replace-match, looking-at.
//! GNU src/search.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- string-match with multiple groups ---

#[test]
fn oracle_string_match_three_groups_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello world 42\" \"hello\" \"world\" \"42\")""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (string-match "\\([a-z]+\\) \\([a-z]+\\) \\([0-9]+\\)" "hello world 42")
  (list (match-string 0 "hello world 42")
        (match-string 1 "hello world 42")
        (match-string 2 "hello world 42")
        (match-string 3 "hello world 42")))"#,
        expect,
    );
    assert_ok_eq("(\"hello world 42\" \"hello\" \"world\" \"42\")", &o, &n);
}

// --- posix-string-match vs string-match ---

#[test]
fn oracle_posix_string_match_vs_string_match_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list (posix-string-match "[a-z]+" "hello")
                (string-match "[a-z]+" "hello"))"#,
        expect,
    );
    assert_ok_eq("(0 0)", &o, &n);
}

// --- looking-at with regex alternation ---

#[test]
fn oracle_looking_at_alternation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*laa*")) (erase-buffer) (insert "foobar") (goto-char 1) (looking-at "foo"))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_looking_at_alternation_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*lab*")) (erase-buffer) (insert "foobar") (goto-char 1) (looking-at "bar\\|baz"))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

// --- replace-match literal string ---

#[test]
fn oracle_replace_match_literal_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"X world\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (set-buffer (get-buffer-create "*rml*"))
  (erase-buffer)
  (insert "hello world")
  (goto-char 1)
  (re-search-forward "[a-z]+" nil t)
  (replace-match "X" t)
  (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"X world\"", &o, &n);
}

// --- match-data structure ---

#[test]
fn oracle_match_data_is_marker_list_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (set-buffer (get-buffer-create "*md*"))
  (erase-buffer)
  (insert "hello world")
  (goto-char 1)
  (re-search-forward "[a-z]+" nil t)
  (consp (match-data)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- search-forward / search-backward with bound ---

#[test]
fn oracle_search_forward_with_bound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*sf*")) (erase-buffer) (insert "abc def ghi") (goto-char 1) (search-forward "def" nil t))"#,
        expect,
    );
    assert_ok_eq("8", &o, &n);
}

#[test]
fn oracle_search_backward_finds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*sb*")) (erase-buffer) (insert "abc def ghi") (goto-char 11) (search-backward "def" nil t))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}
