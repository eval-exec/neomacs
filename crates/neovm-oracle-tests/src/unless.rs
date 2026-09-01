//! Oracle parity tests for `unless`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_unless_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_t, neovm_t) = crate::common::eval_oracle_and_neovm_expect("(unless t 7)", expect);
    assert_ok_eq("nil", &oracle_t, &neovm_t);

    let expect = expect_test::expect![[r#""OK 7""#]];
    let (oracle_nil, neovm_nil) =
        crate::common::eval_oracle_and_neovm_expect("(unless nil 7)", expect);
    assert_ok_eq("7", &oracle_nil, &neovm_nil);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_unless_branching(
        cond in any::<bool>(),
        a in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let cond_form = if cond { "t" } else { "nil" };
        let form = format!("(unless {} {})", cond_form, a);
        let expected = if cond { "nil".to_string() } else { a.to_string() };
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
