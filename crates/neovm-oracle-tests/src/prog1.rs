//! Oracle parity tests for `prog1`.
//!
//! Note: `prog2` is a Lisp macro defined in `subr.el` (not a C primitive),
//! so it is not available in the bare `Context::new()` used by oracle tests.
//! It is tested via full neomacs which loads `subr.el`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_prog1_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(prog1 10 20 30)", expect);
    assert_ok_eq("10", &o, &n);

    let expect = expect_test::expect![[r#""OK first""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(prog1 'first)", expect);
    assert_ok_eq("first", &o, &n);

    let expect = expect_test::expect![[r#""OK 0""#]];
    // side effects still happen
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 0)) (prog1 x (setq x 99)) )", expect);
    assert_ok_eq("0", &o, &n);

    let expect = expect_test::expect![[r#""OK 42""#]];
    // prog1 with no body forms
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(prog1 42)", expect);
    assert_ok_eq("42", &o, &n);
}
