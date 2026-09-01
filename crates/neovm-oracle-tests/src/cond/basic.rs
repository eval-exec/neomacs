//! Oracle parity tests for `cond`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_cond_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK yes""#]];
    // first clause matches
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(cond (t 'yes))", expect);
    assert_ok_eq("yes", &o, &n);

    let expect = expect_test::expect![[r#""OK yes""#]];
    // second clause matches
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(cond (nil 'no) (t 'yes))", expect);
    assert_ok_eq("yes", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // no match returns nil
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(cond (nil 'a) (nil 'b))", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // empty cond
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(cond)", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK 3""#]];
    // clause with multiple body forms
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(cond (t 1 2 3))", expect);
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK 42""#]];
    // test value returned when no body
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(cond (42))", expect);
    assert_ok_eq("42", &o, &n);

    let expect = expect_test::expect![[r#""OK three""#]];
    // numeric test
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((x 3)) (cond ((= x 1) 'one) ((= x 2) 'two) ((= x 3) 'three) (t 'other)))",
        expect,
    );
    assert_ok_eq("three", &o, &n);

    let expect = expect_test::expect![[r#""OK 2""#]];
    // side effects only in matching clause
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((v 0)) (cond (nil (setq v 1)) (t (setq v 2))) v)",
        expect,
    );
    assert_ok_eq("2", &o, &n);
}
