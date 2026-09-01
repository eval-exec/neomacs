//! Oracle parity tests for character/position operations.
//!
//! GNU src/editfns.c, src/buffer.c: `char-after`, `char-before`,
//! `following-char`, `preceding-char`, `bobp`, `eobp`, `bolp`, `eolp`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_char_after_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 98""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct*")) (erase-buffer) (insert "abc") (goto-char 2) (char-after))"#,
        expect,
    );
    assert_ok_eq("98", &o, &n);
}

#[test]
fn oracle_char_after_at_point_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 120""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct2*")) (erase-buffer) (insert "xyz") (goto-char 1) (char-after))"#,
        expect,
    );
    assert_ok_eq("120", &o, &n);
}

#[test]
fn oracle_char_before_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 98""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct3*")) (erase-buffer) (insert "abc") (goto-char 3) (char-before))"#,
        expect,
    );
    assert_ok_eq("98", &o, &n);
}

#[test]
fn oracle_following_char_returns_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct4*")) (erase-buffer) (insert "abc") (goto-char 1) (characterp (following-char)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_preceding_char_returns_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct5*")) (erase-buffer) (insert "abc") (goto-char 3) (characterp (preceding-char)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bobp_at_point_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct6*")) (erase-buffer) (goto-char 1) (bobp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_eobp_at_point_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct7*")) (erase-buffer) (goto-char (point-max)) (eobp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bolp_at_line_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct8*")) (erase-buffer) (insert "line1\nline2") (goto-char 1) (bolp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_eolp_at_end_of_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ct9*")) (erase-buffer) (insert "abc\ndef") (goto-char 4) (eolp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bolp_after_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*bpl*")) (erase-buffer) (insert "abc\ndef") (goto-char 5) (bolp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_char_after_no_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 98""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*cta*")) (erase-buffer) (insert "abc") (goto-char 2) (char-after))"#,
        expect,
    );
    assert_ok_eq("98", &o, &n);
}
