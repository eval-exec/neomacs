//! Oracle parity for looking-at, pos-bol, pos-eol, line-end-position edges.
//! GNU src/search.c, src/editfns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_looking_at_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*la3*")) (erase-buffer) (insert "hello") (goto-char 1) (looking-at "hel"))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_looking_at_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*la4*")) (erase-buffer) (insert "hello") (goto-char 1) (looking-at "xyz"))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_looking_at_with_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*la5*")) (erase-buffer) (insert "hello123") (goto-char 1) (looking-at "[a-z]+"))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_pos_bol_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*pbm*")) (erase-buffer) (insert "abc\ndef\nghi") (goto-char 7) (pos-bol))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_pos_eol_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*pem*")) (erase-buffer) (insert "abc\ndef\nghi") (goto-char 6) (pos-eol))"#,
        expect,
    );
    assert_ok_eq("8", &o, &n);
}

#[test]
fn oracle_line_end_position_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*lepm*")) (erase-buffer) (insert "abc\ndef\nghi") (goto-char 1) (line-end-position))"#,
        expect,
    );
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_bolp_after_hard_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*bhn*")) (erase-buffer) (insert "abc\ndef") (goto-char 5) (bolp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
