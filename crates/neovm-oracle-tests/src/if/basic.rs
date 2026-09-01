//! Oracle parity tests for `if`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_if_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle_t, neovm_t) = crate::common::eval_oracle_and_neovm_expect("(if t 1 2)", expect);
    assert_ok_eq("1", &oracle_t, &neovm_t);

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle_nil, neovm_nil) =
        crate::common::eval_oracle_and_neovm_expect("(if nil 1 2)", expect);
    assert_ok_eq("2", &oracle_nil, &neovm_nil);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_if_branching(
        cond in any::<bool>(),
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let cond_form = if cond { "t" } else { "nil" };
        let form = format!("(if {} {} {})", cond_form, a, b);
        let expected = if cond { a } else { b }.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
