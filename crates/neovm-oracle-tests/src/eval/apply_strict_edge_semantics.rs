//! Oracle parity for eval/apply/funcall/macroexpand strict edges.
//! GNU src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_eval_self_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_eval_quoted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval '(+ 1 2))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_apply_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ '(1 2 3))"#, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_funcall_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(funcall '+ 1 2 3)"#, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_macroexpand_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (if t 1 2)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(macroexpand '(if t 1 2))"#, expect);
    assert_ok_eq("(if t 1 2)", &o, &n);
}

#[test]
fn oracle_apply_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(apply '+ nil)"#, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_funcall_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 99""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(funcall (lambda () 99))"#, expect);
    assert_ok_eq("99", &o, &n);
}
