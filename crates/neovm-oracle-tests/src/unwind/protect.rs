//! Oracle parity tests for `unwind-protect`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_unwind_protect_runs_cleanup_on_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((x 0)) (unwind-protect 1 (setq x 2)) x)",
        expect,
    );
    assert_ok_eq("2", &oracle, &neovm);
}

#[test]
fn oracle_prop_unwind_protect_runs_cleanup_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 0)) (condition-case nil (unwind-protect (/ 1 0) (setq x 7)) (error x)))";
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("7", &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_unwind_protect_returns_protected_value(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(unwind-protect {} {})", a, b);
        let expected = a.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
