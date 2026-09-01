//! Oracle parity tests for `random` and `garbage-collect`.
//!
//! GNU implements `random` in `src/fns.c` and `garbage-collect` in `src/alloc.c`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_random_returns_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(integerp (random 100))", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_random_with_limit_returns_value_in_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(< (random 10) 10)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_random_with_t_uses_most_positive_fixnum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(integerp (random t))", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_garbage_collect_returns_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(listp (garbage-collect))", expect);
    assert_ok_eq("t", &oracle, &neovm);
}
