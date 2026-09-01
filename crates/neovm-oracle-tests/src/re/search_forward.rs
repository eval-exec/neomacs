//! Oracle parity tests for `re-search-forward`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_re_search_forward_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 8""#]];
    let (oracle_ret, neovm_ret) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abc xyz") (goto-char 1) (re-search-forward "xyz"))"#,
        expect,
    );
    assert_ok_eq("8", &oracle_ret, &neovm_ret);

    let expect = expect_test::expect![[r#""OK 8""#]];
    let (oracle_point, neovm_point) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (erase-buffer) (insert "abc xyz") (goto-char 1) (re-search-forward "xyz") (point))"#,
        expect,
    );
    assert_ok_eq("8", &oracle_point, &neovm_point);
}

#[test]
fn oracle_prop_re_search_forward_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(re-search-forward 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_re_search_forward_multibyte_match_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn (erase-buffer) (insert "αβc") (goto-char 1) (re-search-forward "c") (list (match-beginning 0) (match-end 0) (point)))"####;
    let expect = expect_test::expect![[r#""OK (3 4 4)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(3 4 4)", &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_re_search_forward_returns_match_end(
        n in 0usize..20usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let haystack = format!("{}abc", "b".repeat(n));
        let form = format!(
            r#"(progn (erase-buffer) (insert "{}") (goto-char 1) (re-search-forward "abc"))"#,
            haystack
        );
        let expected = (n + 4).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
