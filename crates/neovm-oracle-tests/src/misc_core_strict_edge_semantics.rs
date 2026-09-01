//! Oracle parity for misc core: `char-width`, `string-width`,
//! `compare-strings`, `current-column`, `move-to-column`.
//!
//! GNU src/character.c, src/fns.c, src/indent.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_char_width_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(char-width ?a)"#, expect);
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_string_width_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-width "hello")"#, expect);
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_compare_strings_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "abc" nil nil "abc" nil nil)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_compare_strings_different() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(eq (compare-strings "abc" nil nil "abd" nil nil) t)"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_current_column_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*cc*")) (erase-buffer) (insert "abc") (goto-char 1) (current-column))"#,
        expect,
    );
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_move_to_column_sets_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*mc*")) (erase-buffer) (insert "xyz") (goto-char 1) (move-to-column 2))"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_accessible_keymaps_requires_keymap_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments accessible-keymaps 0)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(listp (accessible-keymaps))"#, expect);
    assert_err_kind(&o, &n, "wrong-number-of-arguments");
}

#[test]
fn oracle_accessible_keymaps_returns_list_for_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let ((m (make-sparse-keymap))) (define-key m [3] (make-sparse-keymap)) (listp (accessible-keymaps m)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_string_width_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-width "")"#, expect);
    assert_ok_eq("0", &o, &n);
}
