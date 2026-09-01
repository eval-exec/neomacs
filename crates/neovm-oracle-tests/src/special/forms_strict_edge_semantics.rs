//! Oracle parity for special form semantics — strict edge cases.
//! GNU src/eval.c: `and`, `or`, `if`, `cond`, `while`, `progn`,
//! `setq`, `let`, `let*`, `quote`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_and_short_circuit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(and nil (/ 1 0))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_and_last_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(and t 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_or_short_circuit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(or t (/ 1 0))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_or_last_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(or nil nil 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_if_then() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK yes""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(if t 'yes 'no)"#, expect);
    assert_ok_eq("yes", &o, &n);
}

#[test]
fn oracle_if_else() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK no""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(if nil 'yes 'no)"#, expect);
    assert_ok_eq("no", &o, &n);
}

#[test]
fn oracle_while_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defvar neovm--test-wl 0) (let ((i 0)) (while (< i 5) (setq neovm--test-wl (1+ neovm--test-wl)) (setq i (1+ i))) neovm--test-wl))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_progn_returns_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(progn 1 2 3)"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_let_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(let ((a 1) (b 2)) (+ a b))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_setq_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 43""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defvar neovm--test-sqm nil) (setq neovm--test-sqm 42 neovm--test-sqm (1+ neovm--test-sqm)))"#,
        expect,
    );
    assert_ok_eq("43", &o, &n);
}
