//! Oracle parity tests for `max`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_max_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 9""#]];
    let (oracle_int, neovm_int) =
        crate::common::eval_oracle_and_neovm_expect("(max 1 9 -3)", expect);
    assert_ok_eq("9", &oracle_int, &neovm_int);

    let expect = expect_test::expect![[r#""OK 2.5""#]];
    let (oracle_mixed, neovm_mixed) =
        crate::common::eval_oracle_and_neovm_expect("(max 1 2.5)", expect);
    assert_ok_eq("2.5", &oracle_mixed, &neovm_mixed);
}

#[test]
fn oracle_prop_max_error_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments max 0)""#]];
    let (arity_oracle, arity_neovm) = crate::common::eval_oracle_and_neovm_expect("(max)", expect);
    assert_err_kind(&arity_oracle, &arity_neovm, "wrong-number-of-arguments");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"x\")""#]];
    let (type_oracle, type_neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(max 1 "x")"#, expect);
    assert_err_kind(&type_oracle, &type_neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_max_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(max {} {})", a, b);
        let expected = format!("OK {}", std::cmp::max(a, b));
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected.as_str());
        prop_assert_eq!(neovm.as_str(), expected.as_str());
        prop_assert_eq!(neovm, oracle);
    }
}
