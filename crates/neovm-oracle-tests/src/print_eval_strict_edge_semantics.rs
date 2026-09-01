//! Oracle parity tests for print/eval: `prin1-to-string`, `eval`,
//! `identity`, `number-to-string`, `string-to-number`.
//!
//! GNU src/print.c, src/eval.c, src/editfns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prin1_to_string_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(prin1-to-string 42)"#, expect);
    assert_ok_eq("\"42\"", &o, &n);
}

#[test]
fn oracle_prin1_to_string_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\"hello\\\"\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(prin1-to-string "hello")"#, expect);
    assert_ok_eq("\"\\\"hello\\\"\"", &o, &n);
}

#[test]
fn oracle_prin1_to_string_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(prin1-to-string 'hello)"#, expect);
    assert_ok_eq("\"hello\"", &o, &n);
}

#[test]
fn oracle_eval_self_evaluating() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_eval_quoted_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval '(+ 1 2))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_identity_returns_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 nil sym)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list (identity 42) (identity nil) (identity 'sym))"#,
        expect,
    );
    assert_ok_eq("(42 nil sym)", &o, &n);
}

#[test]
fn oracle_number_to_string_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string 42)"#, expect);
    assert_ok_eq("\"42\"", &o, &n);
}

#[test]
fn oracle_number_to_string_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"-99\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string -99)"#, expect);
    assert_ok_eq("\"-99\"", &o, &n);
}

#[test]
fn oracle_string_to_number_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-to-number (number-to-string 42))"#,
        expect,
    );
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_number_to_string_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument numberp sym)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string 'sym)"#, expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}
