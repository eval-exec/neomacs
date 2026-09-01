//! Oracle parity for let, let*, setq, defvar, defconst scoping edge cases.
//! GNU src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- let parallel binding ---

#[test]
fn oracle_let_parallel_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(let ((x 1) (y 2)) (+ x y))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_let_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    // Inner let shadows outer, outer preserved after inner ends
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(let ((x 1)) (let ((x 2)) x) x)"#, expect);
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_let_nil_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(let ((x nil)) x)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- let* sequential binding ---

#[test]
fn oracle_let_star_sequential() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 11""#]];
    // let* allows later bindings to reference earlier ones
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(let* ((x 1) (y (+ x 10))) y)"#, expect);
    assert_ok_eq("11", &o, &n);
}

#[test]
fn oracle_let_vs_let_star_parallel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    // In let, bindings are parallel — y can't see x
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let ((x 1)) (let ((x 10) (y x)) y))"#,
        expect,
    );
    // y gets outer x (1), not inner x (10)
    assert_ok_eq("1", &o, &n);
}

// --- setq ---

#[test]
fn oracle_setq_multiple_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq a 1 b 2 c 3) (list a b c))"#,
        expect,
    );
    assert_ok_eq("(1 2 3)", &o, &n);
}

#[test]
fn oracle_setq_returns_last_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(setq zz 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

// --- defvar ---

#[test]
fn oracle_defvar_no_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 100""#]];
    // defvar does not override an existing value
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq dv-test 100) (defvar dv-test 200) dv-test)"#,
        expect,
    );
    assert_ok_eq("100", &o, &n);
}

#[test]
fn oracle_defvar_initializes_unbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defvar dv-test-new 42) dv-test-new)"#,
        expect,
    );
    assert_ok_eq("42", &o, &n);
}

// --- defconst ---

#[test]
fn oracle_defconst_sets_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defconst dc-test 42) dc-test)"#,
        expect,
    );
    assert_ok_eq("42", &o, &n);
}
