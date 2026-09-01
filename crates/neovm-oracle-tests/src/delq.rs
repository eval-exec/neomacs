//! Oracle parity tests for `delq`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_delq_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 5 7)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(delq 3 '(1 3 5 3 7))", expect);
    assert_ok_eq("(1 5 7)", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(delq 'x '(x x x))", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK (10 20 30)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(delq 99 '(10 20 30))", expect);
    assert_ok_eq("(10 20 30)", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(delq 5 nil)", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK (b c d e)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(delq 'a '(b c a d a e))", expect);
    assert_ok_eq("(b c d e)", &o, &n);
}

#[test]
fn oracle_prop_delq_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 42)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(delq 1 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_delq_mutates_before_improper_tail_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fdelq walks with FOR_EACH_TAIL, destructively splices
    // matching cons cells during the walk, and only then runs CHECK_LIST_END.
    // Therefore a later improper tail still leaves earlier removals visible.
    let form = r#"
(let ((x (list 'a 'b 'c)))
  (setcdr (cdr x) 'tail)
  (condition-case err
      (delq 'b x)
    (error (list (car err) x))))
"#;

    let expect = expect_test::expect![[r#""OK (wrong-type-argument (a . tail))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delq_reports_current_improper_tail_after_leading_matches_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fdelq updates LIST as leading matches are skipped, then
    // CHECK_LIST_END reports the final improper tail object rather than the
    // original dotted cons.
    let form = r#"
(list
 (condition-case err
     (delq 1 (cons 1 2))
   (error (list (car err) (cdr err))))
 (condition-case err
     (delq 3 (cons 1 2))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp 2)) (wrong-type-argument (listp (1 . 2))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_delq_integer_removal(
        target in -200i64..200i64,
        a in -200i64..200i64,
        b in -200i64..200i64,
        c in -200i64..200i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(delq {} (list {} {} {}))", target, a, b, c);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_eq!(neovm, oracle, "delq parity failed for: {form}");
    }
}
