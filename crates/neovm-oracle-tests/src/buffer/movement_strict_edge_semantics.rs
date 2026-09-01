//! Oracle parity for buffer movement: `goto-char`, `forward-char`,
//! `forward-line`, `beginning-of-line`, `end-of-line`, `point`,
//! `point-min`, `point-max`.
//!
//! GNU src/editfns.c, src/cmds.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_goto_char_sets_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv*")) (erase-buffer) (insert "0123456789") (goto-char 5) (point))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_goto_char_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv2*")) (erase-buffer) (goto-char 999) (>= (point) 1))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_forward_char_moves_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv3*")) (erase-buffer) (insert "0123456789") (goto-char 1) (forward-char 3) (point))"#,
        expect,
    );
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_forward_line_moves() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 13""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv4*")) (erase-buffer) (insert "line1\nline2\nline3") (goto-char 1) (forward-line 2) (point))"#,
        expect,
    );
    assert_ok_eq("13", &o, &n);
}

#[test]
fn oracle_beginning_of_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv5*")) (erase-buffer) (insert "abc\ndef") (goto-char 7) (beginning-of-line) (point))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_end_of_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv6*")) (erase-buffer) (insert "abc\ndef") (goto-char 1) (end-of-line) (point))"#,
        expect,
    );
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_point_min_is_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv7*")) (erase-buffer) (= (point-min) 1))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_point_max_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mv8*")) (erase-buffer) (insert "12345") (= (point-max) 6))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_forward_char_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump a)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(forward-char 'a)"#, expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}
