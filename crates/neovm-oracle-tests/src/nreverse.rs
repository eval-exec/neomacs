//! Oracle parity tests for `nreverse`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_nreverse_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 2 1)""#]];
    let (oracle_list, neovm_list) =
        crate::common::eval_oracle_and_neovm_expect("(nreverse '(1 2 3))", expect);
    assert_ok_eq("(3 2 1)", &oracle_list, &neovm_list);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_nil, neovm_nil) =
        crate::common::eval_oracle_and_neovm_expect("(nreverse nil)", expect);
    assert_ok_eq("nil", &oracle_nil, &neovm_nil);
}

#[test]
fn oracle_prop_nreverse_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(nreverse 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_nreverse_three_element_list(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(nreverse (list {} {} {}))", a, b, c);
        let expected = format!("({} {} {})", c, b, a);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
