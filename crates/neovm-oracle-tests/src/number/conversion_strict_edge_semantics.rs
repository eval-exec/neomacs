//! Oracle parity tests for number conversion: `float`, `truncate`,
//! `floor`, `ceiling`, `round`, `abs` — strict edge cases.
//!
//! GNU src/floatfns.c: numeric type conversion and rounding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_float_from_int() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42.0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(float 42)", expect);
    assert_ok_eq("42.0", &o, &n);
}

#[test]
fn oracle_float_from_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -7.0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(float -7)", expect);
    assert_ok_eq("-7.0", &o, &n);
}

#[test]
fn oracle_truncate_positive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(truncate 3.7)", expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_truncate_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(truncate -3.7)", expect);
    assert_ok_eq("-3", &o, &n);
}

#[test]
fn oracle_floor_positive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(floor 3.7)", expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_floor_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(floor -3.7)", expect);
    assert_ok_eq("-4", &o, &n);
}

#[test]
fn oracle_ceiling_positive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ceiling 3.2)", expect);
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_ceiling_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ceiling -3.2)", expect);
    assert_ok_eq("-3", &o, &n);
}

#[test]
fn oracle_round_up() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(round 3.6)", expect);
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_abs_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(abs -42)", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_float_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument numberp sym)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(float 'sym)", expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}
