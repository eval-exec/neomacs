//! Oracle parity for key description + subr metadata.
//! GNU src/keyboard.c, src/data.c, src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_key_description_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(key-description "a")"#, expect);
    assert_ok_eq("\"a\"", &o, &n);
}

#[test]
fn oracle_single_key_description_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(single-key-description ?a)"#, expect);
    assert_ok_eq("\"a\"", &o, &n);
}

#[test]
fn oracle_single_key_description_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"C-a\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(single-key-description ?\C-a)"#, expect);
    assert_ok_eq("\"C-a\"", &o, &n);
}

#[test]
fn oracle_subr_arity_car() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(subr-arity (symbol-function 'car))"#,
        expect,
    );
    assert_ok_eq("(1 . 1)", &o, &n);
}

#[test]
fn oracle_subr_arity_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 . many)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(subr-arity (symbol-function 'list))"#,
        expect,
    );
    assert_ok_eq("(0 . many)", &o, &n);
}

#[test]
fn oracle_interactive_form_non_interactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(interactive-form 'car)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_interactive_form_for_interactive_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(consp (interactive-form (lambda () (interactive) 42)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_accessible_keymaps_returns_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(listp (accessible-keymaps (make-sparse-keymap)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_accessible_keymaps_requires_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments accessible-keymaps 0)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(accessible-keymaps)"#, expect);
    assert_err_kind(&o, &n, "wrong-number-of-arguments");
}
