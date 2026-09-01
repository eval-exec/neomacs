//! Oracle parity tests for `abs`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_abs_positive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(abs 42)", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_prop_abs_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(abs -42)", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_prop_abs_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(abs 0)", expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_abs_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 3.14""#];
    crate::common::assert_oracle_parity_expect("(abs -3.14)", expect);
    let expect = expect_test::expect![r#""OK 2.5""#];
    crate::common::assert_oracle_parity_expect("(abs 2.5)", expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_abs_proptest(
        n in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(abs {})", n);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
