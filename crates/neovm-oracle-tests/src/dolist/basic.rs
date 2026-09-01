//! Oracle parity tests for `dolist`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_dolist_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sum 0))
                  (dolist (x '(1 2 3 4 5))
                    (setq sum (+ sum x)))
                  sum)";
    let expect = expect_test::expect![[r#""OK 15""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("15", &o, &n);
}

#[test]
fn oracle_prop_dolist_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((count 0))
                  (dolist (x nil)
                    (setq count (1+ count)))
                  count)";
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_dolist_with_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sum 0))
                  (dolist (x '(10 20 30) sum)
                    (setq sum (+ sum x))))";
    let expect = expect_test::expect![[r#""OK 60""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("60", &o, &n);
}

#[test]
fn oracle_prop_dolist_collect_reversed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((result nil))
                  (dolist (x '(a b c d))
                    (setq result (cons x result)))
                  result)";
    let expect = expect_test::expect![[r#""OK (d c b a)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(d c b a)", &o, &n);
}

#[test]
fn oracle_prop_dolist_filter_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common pattern: filter + collect with dolist
    let form = "(let ((evens nil))
                  (dolist (x '(1 2 3 4 5 6 7 8))
                    (when (= 0 (% x 2))
                      (setq evens (cons x evens))))
                  (nreverse evens))";
    let expect = expect_test::expect![[r#""OK (2 4 6 8)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(2 4 6 8)", &o, &n);
}

#[test]
fn oracle_prop_dolist_map_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Map + collect
    let form = "(let ((result nil))
                  (dolist (x '(1 2 3 4 5))
                    (setq result (cons (* x x) result)))
                  (nreverse result))";
    let expect = expect_test::expect![[r#""OK (1 4 9 16 25)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 4 9 16 25)", &o, &n);
}

#[test]
fn oracle_prop_dolist_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested dolist
    let form = "(let ((pairs nil))
                  (dolist (x '(a b))
                    (dolist (y '(1 2))
                      (setq pairs (cons (cons x y) pairs))))
                  (nreverse pairs))";
    let expect = expect_test::expect![[r#""OK ((a . 1) (a . 2) (b . 1) (b . 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_dolist_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((results nil))
                  (dolist (x '(1 0 3 0 5))
                    (setq results
                          (cons (condition-case nil
                                    (/ 10 x)
                                  (arith-error 'inf))
                                results)))
                  (nreverse results))";
    let expect = expect_test::expect![[r#""OK (10 inf 3 inf 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_dolist_returns_nil_by_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(dolist (x '(1 2 3)))", expect);
    assert_ok_eq("nil", &o, &n);
}
