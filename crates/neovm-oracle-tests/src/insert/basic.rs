//! Oracle parity tests for `insert`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_insert_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(progn (erase-buffer) (insert \"ab\" 99) (buffer-string))",
        expect,
    );
    assert_ok_eq("\"abc\"", &oracle, &neovm);
}

#[test]
fn oracle_prop_insert_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument char-or-string-p (1 2))""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(insert '(1 2))", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_insert_two_ascii_ints(
        a in b'a'..=b'z',
        b in b'a'..=b'z',
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn (erase-buffer) (insert {} {}) (buffer-string))",
            a, b
        );
        let expected = format!("\"{}{}\"", a as char, b as char);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
