//! Oracle parity tests for `string-to-number`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_string_to_number_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle_decimal, neovm_decimal) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "42")"#, expect);
    assert_ok_eq("42", &oracle_decimal, &neovm_decimal);

    let expect = expect_test::expect![[r#""OK 255""#]];
    let (oracle_hex, neovm_hex) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "ff" 16)"#, expect);
    assert_ok_eq("255", &oracle_hex, &neovm_hex);
}

#[test]
fn oracle_prop_string_to_number_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(string-to-number 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_string_to_number_decimal_roundtrip(
        n in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let input = n.to_string();
        let form = format!(r#"(string-to-number "{}")"#, input);
        let expected = n.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
