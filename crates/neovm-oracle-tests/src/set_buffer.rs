//! Oracle parity tests for `set-buffer`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_set_buffer_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((b (get-buffer-create "*neovm-oracle-set-buffer*")))
  (buffer-name (set-buffer b)))"#;
    let expect = expect_test::expect![[r#""OK \"*neovm-oracle-set-buffer*\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("\"*neovm-oracle-set-buffer*\"", &oracle, &neovm);

    let form_current = r#"(let ((b (get-buffer-create "*neovm-oracle-set-buffer-current*")))
  (set-buffer b)
  (buffer-name (current-buffer)))"#;
    let expect = expect_test::expect![[r#""OK \"*neovm-oracle-set-buffer-current*\"""#]];
    let (oracle_current, neovm_current) =
        crate::common::eval_oracle_and_neovm_expect(form_current, expect);
    assert_ok_eq(
        "\"*neovm-oracle-set-buffer-current*\"",
        &oracle_current,
        &neovm_current,
    );
}

#[test]
fn oracle_prop_set_buffer_error_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments set-buffer 0)""#]];
    let (arity_oracle, arity_neovm) =
        crate::common::eval_oracle_and_neovm_expect("(set-buffer)", expect);
    assert_err_kind(&arity_oracle, &arity_neovm, "wrong-number-of-arguments");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    let (type_oracle, type_neovm) =
        crate::common::eval_oracle_and_neovm_expect("(set-buffer 1)", expect);
    assert_err_kind(&type_oracle, &type_neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_set_buffer_roundtrip_name(
        suffix in proptest::string::string_regex(r"[a-z0-9-]{1,10}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let name = format!("*neovm-oracle-set-buffer-{}*", suffix);
        let form = format!(
            "(let ((b (get-buffer-create {:?}))) (set-buffer b) (buffer-name (current-buffer)))",
            name
        );
        let expected = format!("{:?}", name);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
