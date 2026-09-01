//! Oracle parity tests for `format`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_format_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"a-3\"""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(format "%s-%d" "a" 3)"#, expect);
    assert_ok_eq("\"a-3\"", &oracle, &neovm);
}

#[test]
fn oracle_prop_format_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(format 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_format_two_ints(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(r#"(format "%d:%d" {} {})"#, a, b);
        let expected = format!("\"{}:{}\"", a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
