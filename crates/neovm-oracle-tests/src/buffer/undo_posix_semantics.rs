//! Oracle parity tests for buffer undo, local-value, posix regex, ntake.
//!
//! Covers: `buffer-enable-undo`, `buffer-local-value`,
//! `posix-looking-at`, `posix-string-match`, `ntake`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_buffer_enable_undo_no_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-undo*"))
  (buffer-enable-undo)
  t)"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_buffer_local_value_returns_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 77""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (set (make-local-variable 'neovm--test-blv) 77)
  (let ((buf (current-buffer)))
    (buffer-local-value 'neovm--test-blv buf)))"#,
        expect,
    );
    assert_ok_eq("77", &oracle, &neovm);
}

#[test]
fn oracle_posix_looking_at_matches_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-posix*"))
  (erase-buffer)
  (insert "hello world")
  (goto-char 1)
  (posix-looking-at "hello"))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_posix_string_match_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(posix-string-match "foo" "foobar")"#,
        expect,
    );
    assert_ok_eq("0", &oracle, &neovm);
}

#[test]
fn oracle_ntake_takes_from_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(ntake 2 '(a b c d e))"#, expect);
    assert_ok_eq("(a b)", &oracle, &neovm);
}
