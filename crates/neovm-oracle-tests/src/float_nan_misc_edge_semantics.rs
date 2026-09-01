//! Oracle parity for float, isnan, and misc edge cases.
//! GNU src/floatfns.c, src/data.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- float ---

#[test]
fn oracle_float_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42.0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(float 42)"#, expect);
    assert_ok_eq("42.0", &o, &n);
}

#[test]
fn oracle_float_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(float 0)"#, expect);
    assert_ok_eq("0.0", &o, &n);
}

#[test]
fn oracle_float_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -1.0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(float -1)"#, expect);
    assert_ok_eq("-1.0", &o, &n);
}

// --- isnan ---

#[test]
fn oracle_isnan_regular_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(isnan 0.0)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_isnan_on_nan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(isnan (/ 0.0 0.0))"#, expect);
    assert_ok_eq("t", &o, &n);
}

// --- floatp ---

#[test]
fn oracle_floatp_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(floatp 3.14)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_floatp_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(floatp 42)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- integerp ---

#[test]
fn oracle_integerp_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(integerp 3.14)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- numberp ---

#[test]
fn oracle_numberp_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(numberp 3.14)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_numberp_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(numberp 42)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_numberp_string_is_not_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(numberp "42")"#, expect);
    assert_ok_eq("nil", &o, &n);
}
