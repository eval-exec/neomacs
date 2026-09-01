//! Oracle parity tests for `last` and `butlast`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_last_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5)""#]];
    // `last` returns the last cons cell
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(last '(1 2 3 4 5))", expect);
    assert_ok_eq("(5)", &o, &n);
}

#[test]
fn oracle_prop_last_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(last '(42))", expect);
    assert_ok_eq("(42)", &o, &n);
}

#[test]
fn oracle_prop_last_with_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 5)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(last '(1 2 3 4 5) 2)", expect);
    assert_ok_eq("(4 5)", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(last '(1 2 3 4 5) 0)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_last_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 . 3)""#]];
    // last on a dotted list
    crate::common::assert_oracle_parity_expect("(last '(1 2 . 3))", expect);
}

#[test]
fn oracle_last_circular_list_uses_safe_length_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((cycle (list 'a 'b 'c))
       (_ (setcdr (last cycle) cycle))
       (detected-length (safe-length cycle)))
  (list detected-length
        (eq (last cycle) (nthcdr (1- detected-length) cycle))
        (car (last cycle))
        (car (last cycle 2))
        (eq (last cycle detected-length) cycle)
        (eq (last cycle (1+ detected-length)) cycle)))
"#;

    let expect = expect_test::expect![[r#""OK (5 t b a t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_butlast_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(butlast '(1 2 3 4 5))", expect);
    assert_ok_eq("(1 2 3 4)", &o, &n);
}

#[test]
fn oracle_prop_butlast_with_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(butlast '(1 2 3 4 5) 2)", expect);
    assert_ok_eq("(1 2 3)", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(butlast '(1 2 3 4 5) 5)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_butlast_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(butlast nil)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_butlast_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(butlast '(42))", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_last_butlast_complement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // butlast + last should reconstruct the original list (by append)
    let form = "(let ((lst '(1 2 3 4 5)))
                  (equal lst (append (butlast lst) (last lst))))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}
