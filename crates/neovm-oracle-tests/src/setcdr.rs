//! Oracle parity tests for `setcdr`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_setcdr_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 . 9)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((x (cons 1 2))) (setcdr x 9) x)",
        expect,
    );
    assert_ok_eq("(1 . 9)", &oracle, &neovm);
}

#[test]
fn oracle_prop_setcdr_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument consp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(setcdr 1 2)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_setcdr_updates_tail(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(let ((x (cons {} {}))) (setcdr x {}) x)", a, b, c);
        let expected = format!("({} . {})", a, c);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
