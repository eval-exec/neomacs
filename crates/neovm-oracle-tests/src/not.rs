//! Oracle parity tests for `not`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_not_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle_t, neovm_t) = crate::common::eval_oracle_and_neovm_expect("(not nil)", expect);
    assert_ok_eq("t", &oracle_t, &neovm_t);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_nil, neovm_nil) = crate::common::eval_oracle_and_neovm_expect("(not 1)", expect);
    assert_ok_eq("nil", &oracle_nil, &neovm_nil);
}

#[test]
fn oracle_prop_not_wrong_arity_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments not 0)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(not)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_not_boolean_behavior(
        cond in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let arg = if cond { "t" } else { "nil" };
        let expected = if cond { "nil" } else { "t" };
        let form = format!("(not {})", arg);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected, &oracle, &neovm);
    }
}
