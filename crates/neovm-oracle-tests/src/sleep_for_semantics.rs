//! Oracle parity tests for `sleep-for`.
//!
//! GNU implements `sleep-for` in `src/dispnew.c` — pauses for a given
//! number of seconds (and optional milliseconds).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_sleep_for_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(sleep-for 0)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_sleep_for_with_milliseconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(sleep-for 0 50)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_sleep_for_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument numberp a)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(sleep-for 'a)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_sleep_for_wrong_number_of_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments sleep-for 0)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(sleep-for)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}
