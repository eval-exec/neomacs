//! Oracle parity tests for buffer text operations: `search-forward`,
//! `search-backward`, `re-search-forward`, `replace-match`, `insert`,
//! `buffer-substring` — strict edges.
//!
//! GNU src/search.c, src/editfns.c: buffer text manipulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_buffer_substring_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hell\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-bs*"))
  (erase-buffer)
  (insert "hello world")
  (buffer-substring 1 5))"#,
        expect,
    );
    assert_ok_eq("\"hell\"", &o, &n);
}

#[test]
fn oracle_buffer_substring_no_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"test\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-bsp*"))
  (erase-buffer)
  (insert "test")
  (buffer-substring-no-properties 1 5))"#,
        expect,
    );
    assert_ok_eq("\"test\"", &o, &n);
}

#[test]
fn oracle_search_forward_finds_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-sf*"))
  (erase-buffer)
  (insert "abc def ghi")
  (goto-char 1)
  (numberp (search-forward "def" nil t)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_search_backward_finds_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-sb*"))
  (erase-buffer)
  (insert "abc def ghi")
  (goto-char 100)
  (numberp (search-backward "def" nil t)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_re_search_forward_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-rs*"))
  (erase-buffer)
  (insert "abc123def")
  (goto-char 1)
  (numberp (re-search-forward "[0-9]+" nil t)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_re_search_backward_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-rsb*"))
  (erase-buffer)
  (insert "abc123def")
  (goto-char 100)
  (numberp (re-search-backward "[0-9]+" nil t)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_point_after_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 9""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-ps*"))
  (erase-buffer)
  (insert "beforeXXafter")
  (goto-char 1)
  (search-forward "XX" nil t)
  (point))"#,
        expect,
    );
    assert_ok_eq("9", &o, &n);
}

#[test]
fn oracle_replace_match_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello earth\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-rm*"))
  (erase-buffer)
  (insert "hello world")
  (goto-char 1)
  (search-forward "world" nil t)
  (replace-match "earth")
  (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"hello earth\"", &o, &n);
}
