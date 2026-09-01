//! Oracle parity tests for symbol primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_symbol_name_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(symbol-name 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_intern_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(intern 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_fboundp_car() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(fboundp 'car)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_prop_boundp_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(boundp 'nil)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_prop_symbolp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(symbolp "x")"#, expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(symbolp 'x)", expect);
}

#[test]
fn oracle_prop_bare_colon_keyword_self_evaluates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((x :)) (list (eq x :) (keywordp x) (symbolp x)))",
        expect,
    );
    assert_ok_eq("(t t t)", &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_intern_symbol_name_roundtrip(
        name in proptest::string::string_regex(r"[a-z][a-z0-9-]{0,12}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(r#"(symbol-name (intern {:?}))"#, name);
        let expected = format!("{:?}", name);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_symbolp_interned_symbol(
        name in proptest::string::string_regex(r"[a-z][a-z0-9-]{0,12}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(r#"(symbolp (intern {:?}))"#, name);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq("t", &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_intern_eq_idempotent(
        name in proptest::string::string_regex(r"[a-z][a-z0-9-]{0,12}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(r#"(eq (intern {:?}) (intern {:?}))"#, name, name);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq("t", &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_fboundp_unknown_symbol(
        name in proptest::string::string_regex(r"[a-z][a-z0-9-]{0,10}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let symbol_name = format!("neovm-oracle-unknown-fn-{name}");
        let form = format!(r#"(fboundp (intern {:?}))"#, symbol_name);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq("nil", &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_boundp_unknown_symbol(
        name in proptest::string::string_regex(r"[a-z][a-z0-9-]{0,10}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let symbol_name = format!("neovm-oracle-unknown-var-{name}");
        let form = format!(r#"(boundp (intern {:?}))"#, symbol_name);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq("nil", &oracle, &neovm);
    }
}
