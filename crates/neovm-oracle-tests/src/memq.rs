//! Oracle parity tests for `memq`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_memq_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (b c)""#]];
    let (oracle_found, neovm_found) =
        crate::common::eval_oracle_and_neovm_expect("(memq 'b '(a b c))", expect);
    assert_ok_eq("(b c)", &oracle_found, &neovm_found);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_missing, neovm_missing) =
        crate::common::eval_oracle_and_neovm_expect("(memq 'z '(a b c))", expect);
    assert_ok_eq("nil", &oracle_missing, &neovm_missing);
}

#[test]
fn oracle_prop_memq_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(memq 'a 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_memq_float_uses_eq_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // `memq` uses `eq`, so a separately read float literal is not identical.
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(memq 1.0 '(1.0 2.0))", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_memq_reports_improper_tail_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (wrong-type-argument (listp (2 . 3)))""#]];
    // GNU src/fns.c:Fmemq walks with FOR_EACH_TAIL and then calls
    // CHECK_LIST_END, so a missed search through a dotted list reports the
    // original dotted cons as the offending list object.
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(condition-case err (memq 1 (cons 2 3)) (error (list (car err) (cdr err))))",
        expect,
    );
    assert_ok_eq("(wrong-type-argument (listp (2 . 3)))", &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_memq_head_match(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(memq {} (list {} {}))", a, a, b);
        let expected = format!("({} {})", a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
