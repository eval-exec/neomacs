//! Oracle parity for skip-chars + field operations.
//! GNU src/editfns.c, src/cmds.c, src/textprop.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_skip_chars_forward_moves() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*sc*")) (erase-buffer) (insert "abc123xyz") (goto-char 1) (skip-chars-forward "a-z") (point))"#,
        expect,
    );
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_skip_chars_backward_moves() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*scb*")) (erase-buffer) (insert "abc123xyz") (goto-char 10) (skip-chars-backward "a-z") (point))"#,
        expect,
    );
    assert_ok_eq("7", &o, &n);
}

#[test]
fn oracle_skip_chars_forward_returns_distance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*scd*")) (erase-buffer) (insert "123abc") (goto-char 1) (skip-chars-forward "0-9"))"#,
        expect,
    );
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_field_beginning_moves_to_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*fb*")) (erase-buffer) (insert "hello world") (goto-char 7) (field-beginning) (point))"#,
        expect,
    );
    assert_ok_eq("7", &o, &n);
}

#[test]
fn oracle_field_end_moves_to_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*fe*")) (erase-buffer) (insert "hello world") (goto-char 7) (field-end) (point))"#,
        expect,
    );
    assert_ok_eq("7", &o, &n);
}

#[test]
fn oracle_field_string_returns_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*fs*")) (erase-buffer) (insert "hello world") (goto-char 7) (stringp (field-string)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_delete_field_removes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*df*")) (erase-buffer) (insert "hello world") (goto-char 7) (delete-field) (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"\"", &o, &n);
}

#[test]
fn oracle_constrain_to_field_returns_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ctf*")) (erase-buffer) (insert "hello world") (integerp (constrain-to-field 5 12)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
