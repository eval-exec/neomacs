//! Oracle parity tests for numeric comparison primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, eval_oracle_and_neovm};

#[test]
fn oracle_prop_compare_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(< 1 "x")"#;
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_num_eq_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(= {} {})", a, b);
        let expected = if a == b { "OK t" } else { "OK nil" };

        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_num_ne_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(/= {} {})", a, b);
        let expected = if a != b { "OK t" } else { "OK nil" };

        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_num_lt_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(< {} {})", a, b);
        let expected = if a < b { "OK t" } else { "OK nil" };

        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_num_le_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(<= {} {})", a, b);
        let expected = if a <= b { "OK t" } else { "OK nil" };

        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_num_gt_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(> {} {})", a, b);
        let expected = if a > b { "OK t" } else { "OK nil" };

        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_num_ge_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(>= {} {})", a, b);
        let expected = if a >= b { "OK t" } else { "OK nil" };

        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_compare_mixed_int_float(
        a in -100_000i64..100_000i64,
        b in -100_000.0f64..100_000.0f64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let forms = [
            format!("(= {} {})", a, b),
            format!("(< {} {})", a, b),
            format!("(<= {} {})", a, b),
            format!("(> {} {})", a, b),
            format!("(>= {} {})", a, b),
            format!("(/= {} {})", a, b),
        ];

        for form in &forms {
            let (oracle, neovm) = eval_oracle_and_neovm(form);
            prop_assert_eq!(neovm.as_str(), oracle.as_str());
        }
    }
}
