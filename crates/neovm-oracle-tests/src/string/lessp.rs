//! Oracle parity tests for `string-lessp`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_string_lessp_wrong_arity_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments string-lessp 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-lessp "a")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_prop_string_lessp_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-lessp "a" 1)"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_string_lessp_alias_smoke() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string< "abc" "abd")"#, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_string_lessp_operator(
        a in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
        b in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let expected = if a < b { "t" } else { "nil" };
        let form = format!("(string-lessp {:?} {:?})", a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected, &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_string_lessp_alias_operator(
        a in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
        b in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let expected = if a < b { "t" } else { "nil" };
        let form = format!("(string< {:?} {:?})", a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected, &oracle, &neovm);
    }
}
