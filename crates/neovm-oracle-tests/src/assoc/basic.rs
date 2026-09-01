//! Oracle parity tests for `assoc`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_assoc_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"b\" . 2)""#]];
    let (oracle_found, neovm_found) = crate::common::eval_oracle_and_neovm_expect(
        r#"(assoc "b" '(("a" . 1) ("b" . 2)))"#,
        expect,
    );
    assert_ok_eq("(\"b\" . 2)", &oracle_found, &neovm_found);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_missing, neovm_missing) = crate::common::eval_oracle_and_neovm_expect(
        r#"(assoc "z" '(("a" . 1) ("b" . 2)))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle_missing, &neovm_missing);
}

#[test]
fn oracle_prop_assoc_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(assoc "a" 1)"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_assoc_equal_key(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(r#"(assoc "k" (list (cons "x" {}) (cons (concat "k") {})))"#, a, b);
        let expected = format!("(\"k\" . {})", b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
