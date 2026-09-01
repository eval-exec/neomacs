//! Oracle parity for advanced Lisp constructs via binary execution.
//! Tests defmacro, closures, apply, fboundp, symbol-function, macrop.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- defmacro ---

#[test]
fn oracle_defmacro_basic_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defmacro nvm--test-dm (x) (list '+ x 1))
  (nvm--test-dm 5))"#,
        expect,
    );
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_defmacro_returns_lambda_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defmacro nvm--test-dm2 (x) (list 'quote x))
  (nvm--test-dm2 42))"#,
        expect,
    );
    assert_ok_eq("42", &o, &n);
}

// --- closures ---

#[test]
fn oracle_closure_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 15""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defun nvm--mk-adder (n)
    (lambda (x) (+ x n)))
  (funcall (nvm--mk-adder 10) 5))"#,
        expect,
    );
    assert_ok_eq("15", &o, &n);
}

// --- apply ---

#[test]
fn oracle_apply_with_list_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defun nvm--apply-fn (a b c) (list a b c))
  (apply 'nvm--apply-fn '(1 2 3)))"#,
        expect,
    );
    assert_ok_eq("(1 2 3)", &o, &n);
}

#[test]
fn oracle_apply_with_args_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ 1 2 '(3 4))"#, expect);
    assert_ok_eq("10", &o, &n);
}

// --- fboundp ---

#[test]
fn oracle_fboundp_defined_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defun nvm--fb-fn () 42)
  (fboundp 'nvm--fb-fn))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_fboundp_undefined_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(fboundp 'nvm--no-such-function-xyz)"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

// --- symbol-function ---

#[test]
fn oracle_symbol_function_returns_function_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defun nvm--sf-fn (x) x)
  (functionp (symbol-function 'nvm--sf-fn)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- macrop on macro symbol-function ---

#[test]
fn oracle_macrop_on_symbol_function_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defmacro nvm--mac-test (x) (list '1+ x))
  (macrop (symbol-function 'nvm--mac-test)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- funcall with lambda ---

#[test]
fn oracle_funcall_lambda_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 30""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(funcall (lambda (x y) (+ x y)) 10 20)"#,
        expect,
    );
    assert_ok_eq("30", &o, &n);
}
