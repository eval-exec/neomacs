//! Oracle parity tests for predicate primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, eval_oracle_and_neovm};

#[test]
fn oracle_prop_numberp_wrong_arity_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(numberp)";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_prop_null_fixed_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    for (form, expected) in [
        ("(null nil)", "OK t"),
        ("(null 1)", "OK nil"),
        ("(listp nil)", "OK t"),
    ] {
        let (oracle, neovm) = eval_oracle_and_neovm(form);
        assert_eq!(oracle.as_str(), expected);
        assert_eq!(neovm.as_str(), expected);
        assert_eq!(neovm, oracle);
    }
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_numberp_int(
        a in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(numberp {})", a);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_numberp_float(
        a in -100_000.0f64..100_000.0f64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(numberp {})", a);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_integerp_int(
        a in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(integerp {})", a);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_integerp_float(
        a in -100_000.0f64..100_000.0f64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(integerp {})", a);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), "OK nil");
        prop_assert_eq!(neovm.as_str(), "OK nil");
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_floatp_int(
        a in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(floatp {})", a);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), "OK nil");
        prop_assert_eq!(neovm.as_str(), "OK nil");
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_floatp_float(
        a in -100_000.0f64..100_000.0f64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(floatp {})", a);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_consp_atom_listp(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let checks = [
            (format!("(consp (cons {} {}))", a, b), "OK t"),
            (format!("(listp (cons {} {}))", a, b), "OK t"),
            (format!("(atom (cons {} {}))", a, b), "OK nil"),
            (format!("(atom {})", a), "OK t"),
        ];

        for (form, expected) in &checks {
            let (oracle, neovm) = eval_oracle_and_neovm(form);
            prop_assert_eq!(oracle.as_str(), *expected);
            prop_assert_eq!(neovm.as_str(), *expected);
            prop_assert_eq!(neovm.as_str(), oracle.as_str());
        }
    }
}
