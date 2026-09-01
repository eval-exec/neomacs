//! Oracle parity tests for `buffer-name`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_buffer_name_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(stringp (buffer-name))", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_prop_buffer_name_error_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-name 2)""#]];
    let (arity_oracle, arity_neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-name nil nil)", expect);
    assert_err_kind(&arity_oracle, &arity_neovm, "wrong-number-of-arguments");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument bufferp 1)""#]];
    let (type_oracle, type_neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-name 1)", expect);
    assert_err_kind(&type_oracle, &type_neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_buffer_name_after_set_buffer(
        suffix in proptest::string::string_regex(r"[a-z0-9-]{1,10}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let name = format!("*neovm-oracle-buffer-name-{}*", suffix);
        let form = format!(
            "(let ((b (get-buffer-create {:?}))) (set-buffer b) (buffer-name))",
            name
        );
        let expected = format!("{:?}", name);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
