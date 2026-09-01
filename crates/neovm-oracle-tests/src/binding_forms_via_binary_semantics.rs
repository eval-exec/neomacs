//! Oracle parity for Lisp binding forms via binary.
//! Requires full bootstrap: if-let, when-let, and-let*, while-let.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_if_let_binds_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(if-let ((x 1)) x)"#, expect);
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_if_let_star_sequential_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(if-let* ((x 1) (y (+ x 2))) y)"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_when_let_binds_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(when-let ((x t)) x)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_when_let_nil_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(when-let ((x nil)) 'never)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_and_let_star_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(and-let* ((x 1) (y 2) (z 3)) z)"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_and_let_star_short_circuit_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(and-let* ((x 1) (y nil) (z 3)) 'never)"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}
