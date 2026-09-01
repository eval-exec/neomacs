//! Oracle parity tests for `string-equal`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_string_equal_wrong_arity_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments string-equal 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-equal "a")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_prop_string_equal_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-equal "a" 1)"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_string_equal_alias_smoke() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(string= "abc" "abc")"#, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_string_equal_operator(
        a in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
        b in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let expected = if a == b { "t" } else { "nil" };
        let form = format!("(string-equal {:?} {:?})", a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected, &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_string_equal_alias_operator(
        a in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
        b in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let expected = if a == b { "t" } else { "nil" };
        let form = format!("(string= {:?} {:?})", a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected, &oracle, &neovm);
    }
}
