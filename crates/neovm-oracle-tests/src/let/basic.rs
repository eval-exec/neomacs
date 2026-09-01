//! Oracle parity tests for `let`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_let_scoping_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 1)) (let ((x 2)) x))", expect);
    assert_ok_eq("2", &oracle, &neovm);
}

#[test]
fn oracle_prop_let_parallel_binding_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 1) (y (+ x 2))) y)", expect);
    assert_err_kind(&oracle, &neovm, "void-variable");
}

#[test]
fn oracle_prop_let_duplicate_binding_last_wins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 1) (x 2)) x)", expect);
    assert_ok_eq("2", &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_let_returns_bound_value(
        a in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(let ((x {})) x)", a);
        let expected = a.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
