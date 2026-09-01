//! Oracle parity for subr-arity, special-form-p, function introspection via binary.
//! GNU src/data.c, src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_subr_arity_car_1_1_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(subr-arity (symbol-function 'car))"#,
        expect,
    );
    assert_ok_eq("(1 . 1)", &o, &n);
}

#[test]
fn oracle_subr_arity_plus_variadic_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 . many)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(subr-arity (symbol-function '+))"#, expect);
    assert_ok_eq("(0 . many)", &o, &n);
}

#[test]
fn oracle_special_form_p_if_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(special-form-p 'if)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_special_form_p_car_is_nil_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(special-form-p 'car)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_functionp_subr_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(functionp (symbol-function 'car))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_macrop_on_macro_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defmacro nvm--fi-macro (x) (list '1+ x)) (macrop (symbol-function 'nvm--fi-macro)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
