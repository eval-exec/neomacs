//! Oracle parity for apply, funcall deep edge cases.
//! GNU src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- apply with spread ---

#[test]
fn oracle_apply_with_spread_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 15""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ 1 2 '(3 4 5))"#, expect);
    assert_ok_eq("15", &o, &n);
}

#[test]
fn oracle_apply_with_empty_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ '())"#, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_apply_with_single_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(apply 'length '((a b c)))"#, expect);
    assert_ok_eq("3", &o, &n);
}

// --- funcall ---

#[test]
fn oracle_funcall_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(funcall '+ 1 2 3)"#, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_funcall_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 30""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(funcall (lambda (x y) (+ x y)) 10 20)"#,
        expect,
    );
    assert_ok_eq("30", &o, &n);
}

// --- apply + funcall interaction ---

#[test]
fn oracle_apply_funcall_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(apply 'funcall (list '+ 1 2 3))"#, expect);
    assert_ok_eq("6", &o, &n);
}

// --- mapcar with various functions ---

#[test]
fn oracle_mapcar_with_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(mapcar '1+ '(1 2 3))"#, expect);
    assert_ok_eq("(2 3 4)", &o, &n);
}

#[test]
fn oracle_mapcar_with_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 6)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(mapcar (lambda (x) (* x 2)) '(1 2 3))"#,
        expect,
    );
    assert_ok_eq("(2 4 6)", &o, &n);
}

// --- mapc returns original list ---

#[test]
fn oracle_mapc_returns_original_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq lst '(1 2 3)) (eq lst (mapc 'ignore lst)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
