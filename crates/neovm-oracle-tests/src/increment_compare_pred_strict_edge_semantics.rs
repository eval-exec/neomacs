//! Oracle parity for 1+, 1-, <, >, <=, >=, /=, floatp, listp edges.
//! GNU src/data.c, src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_one_plus_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(1+ 41)", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_one_minus_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(1- 43)", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_one_plus_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(1+ -1)", expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_lt_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(< 1 2 3)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_lt_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(< 1 1)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_gt_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(> 3 2 1)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_le_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(<= 1 1 2)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_ge_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(>= 3 3 2)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_not_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(/= 1 2)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_not_equal_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(/= 1 1)", expect);
    assert_ok_eq("nil", &o, &n);
}
