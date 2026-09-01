//! Oracle parity tests for vector primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_aref_wrong_index_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump \"x\")""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(aref [1 2] "x")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_aref_out_of_range_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range [1 2] 10)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(aref [1 2] 10)", expect);
    assert_err_kind(&oracle, &neovm, "args-out-of-range");
}

#[test]
fn oracle_prop_aset_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(aset 1 0 2)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_vector_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(vector {} {} {})", a, b, c);
        let expected = format!("[{} {} {}]", a, b, c);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_aref_operator(
        idx in 0usize..5usize,
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
        d in -100_000i64..100_000i64,
        e in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let values = [a, b, c, d, e];
        let form = format!("(aref [{} {} {} {} {}] {})", a, b, c, d, e, idx);
        let expected = values[idx].to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_aset_operator(
        idx in 0usize..5usize,
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
        d in -100_000i64..100_000i64,
        e in -100_000i64..100_000i64,
        x in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let mut values = [a, b, c, d, e];
        values[idx] = x;
        let form = format!(
            "(let ((v [{} {} {} {} {}])) (aset v {} {}) v)",
            a, b, c, d, e, idx, x
        );
        let expected = format!(
            "[{} {} {} {} {}]",
            values[0], values[1], values[2], values[3], values[4]
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_length_vector_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
        d in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(length [{} {} {} {}])", a, b, c, d);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq("4", &oracle, &neovm);
    }
}
