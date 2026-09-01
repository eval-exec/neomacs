//! Oracle parity tests for `sequencep` and sequence-related predicates.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_sequencep_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(sequencep '(1 2 3))", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_sequencep_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(sequencep nil)", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_sequencep_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(sequencep [1 2 3])", expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_sequencep_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(sequencep "hello")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_sequencep_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(sequencep 42)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_sequencep_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(sequencep 'foo)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_sequencep_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(sequencep (make-hash-table))", expect);
    assert_ok_eq("nil", &o, &n);
}
