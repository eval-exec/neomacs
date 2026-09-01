//! Oracle parity tests for number predicates: `natnump`, `integerp`,
//! `floatp`, `numberp` — strict edge cases.
//!
//! GNU src/data.c: type predicates have subtle behavior around
//! bignums, float/integer boundaries, and non-numeric inputs.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_natnump_positive_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(natnump 42)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_natnump_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(natnump 0)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_natnump_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(natnump -1)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_natnump_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(natnump 3.14)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_floatp_integer_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(floatp 42)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_floatp_float_returns_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(floatp 3.14)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_integerp_float_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(integerp 3.14)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_integerp_large_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(integerp 999999999999999)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_numberp_on_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(numberp 'sym)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}
