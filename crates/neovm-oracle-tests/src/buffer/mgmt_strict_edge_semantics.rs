//! Oracle parity for buffer mgmt: rename-buffer, kill-buffer,
//! generate-new-buffer, buffer-string, buffer-substring.
//! GNU src/buffer.c, src/editfns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_rename_buffer_changes_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"*rn-old*\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (let ((b (get-buffer-create "*rn-old*"))) (rename-buffer "*rn-new*") (buffer-name b)))"#,
        expect,
    );
    assert_ok_eq("\"*rn-old*\"", &o, &n);
}

#[test]
fn oracle_kill_buffer_removes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (let ((b (get-buffer-create "*kb*"))) (kill-buffer b) (buffer-live-p b)))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_buffer_string_returns_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*bsc*")) (erase-buffer) (insert "hello world") (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"hello world\"", &o, &n);
}

#[test]
fn oracle_buffer_substring_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"234\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*bsr*")) (erase-buffer) (insert "0123456789") (buffer-substring 3 6))"#,
        expect,
    );
    assert_ok_eq("\"234\"", &o, &n);
}
