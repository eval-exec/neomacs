//! Oracle parity tests for `match-end`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_match_end_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 4""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "b+" "abbb") (match-end 0))"#,
        expect,
    );
    assert_ok_eq("4", &oracle, &neovm);
}

#[test]
fn oracle_prop_match_end_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump \"x\")""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(match-end "x")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_match_end_uses_character_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "c" "αβc") (match-end 0))"#,
        expect,
    );
    assert_ok_eq("3", &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_match_end_group0_index(
        n in 0usize..20usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let haystack = format!("{}abc", "b".repeat(n));
        let form = format!(
            r#"(progn (string-match "abc" "{}") (match-end 0))"#,
            haystack
        );
        let expected = (n + 3).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
