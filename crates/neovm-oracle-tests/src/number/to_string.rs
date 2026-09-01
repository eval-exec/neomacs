//! Oracle parity tests for `number-to-string`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_number_to_string_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(number-to-string 42)", expect);
    assert_ok_eq(r#""42""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"0\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(number-to-string 0)", expect);
    assert_ok_eq(r#""0""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"-100\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(number-to-string -100)", expect);
    assert_ok_eq(r#""-100""#, &o, &n);
}

#[test]
fn oracle_prop_number_to_string_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"3.14\"""#]];
    crate::common::assert_oracle_parity_expect("(number-to-string 3.14)", expect);
    let expect = expect_test::expect![[r#""OK \"0.0\"""#]];
    crate::common::assert_oracle_parity_expect("(number-to-string 0.0)", expect);
    let expect = expect_test::expect![[r#""OK \"-2.5\"""#]];
    crate::common::assert_oracle_parity_expect("(number-to-string -2.5)", expect);
    let expect = expect_test::expect![[r#""OK \"10000000000.0\"""#]];
    crate::common::assert_oracle_parity_expect("(number-to-string 1.0e10)", expect);
}

#[test]
fn oracle_prop_number_to_string_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(number-to-string "hello")"####;
    let (oracle, neovm) = eval_oracle_and_neovm(form);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_number_to_string_large_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"1000000\"""#]];
    crate::common::assert_oracle_parity_expect("(number-to-string 1000000)", expect);
    let expect = expect_test::expect![[r#""OK \"-999999\"""#]];
    crate::common::assert_oracle_parity_expect("(number-to-string -999999)", expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_number_to_string_roundtrip(
        n in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(string-to-number (number-to-string {}))", n);
        let expected = format!("OK {}", n);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), expected.as_str());
        prop_assert_eq!(oracle.as_str(), expected.as_str());
    }
}
