//! Oracle parity for line-number-at-pos, count-lines, bolp, eolp, indent-to.
//! GNU src/editfns.c, src/indent.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_line_number_at_pos_first_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ln*")) (erase-buffer) (insert "abc\ndef\nghi") (line-number-at-pos 2))"#,
        expect,
    );
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_line_number_at_pos_second_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ln2*")) (erase-buffer) (insert "abc\ndef\nghi") (line-number-at-pos 6))"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_line_number_at_pos_no_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*ln3*")) (erase-buffer) (goto-char 1) (line-number-at-pos))"#,
        expect,
    );
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_count_lines_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    // GNU defines `count-lines` in lisp/simple.el, so exercise the dumped
    // runtime rather than the bare-core evaluator.
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*cl*")) (erase-buffer) (insert "a\nb\nc\nd") (= (count-lines (point-min) (point-max)) 4))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bolp_at_beginning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*bl*")) (erase-buffer) (insert "abc\ndef") (goto-char 1) (bolp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_eolp_at_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*el*")) (erase-buffer) (insert "abc\ndef") (goto-char 4) (eolp))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_line_beginning_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*lbp*")) (erase-buffer) (insert "abc\ndef") (goto-char 7) (line-beginning-position))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_line_end_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*lep*")) (erase-buffer) (insert "abc\ndef") (goto-char 5) (line-end-position))"#,
        expect,
    );
    assert_ok_eq("8", &o, &n);
}
