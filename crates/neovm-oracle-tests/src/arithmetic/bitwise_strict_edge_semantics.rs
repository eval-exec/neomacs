//! Oracle parity tests for bitwise arithmetic: `logand`, `logior`,
//! `logxor`, `lognot`, `ash` — strict edge cases.
//!
//! GNU src/data.c: bitwise operations on integers.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_logand_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(logand 7 3)", expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_logand_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(logand 42 0)", expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_logior_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(logior 1 2 4)", expect);
    assert_ok_eq("7", &o, &n);
}

#[test]
fn oracle_logxor_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(logxor 7 3)", expect);
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_logxor_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(logxor)", expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_lognot_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(lognot 0)", expect);
    assert_ok_eq("-1", &o, &n);
}

#[test]
fn oracle_lognot_negative_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(lognot -1)", expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_ash_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ash 1 3)", expect);
    assert_ok_eq("8", &o, &n);
}

#[test]
fn oracle_ash_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ash 8 -2)", expect);
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_ash_zero_shift() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ash 42 0)", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_mod_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(% 10 3)", expect);
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_mod_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(% -10 3)", expect);
    assert_ok_eq("-1", &o, &n);
}
