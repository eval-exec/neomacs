//! Oracle parity tests for `copy-sequence`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_copy_sequence_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(copy-sequence '(1 2 3))", expect);
    assert_ok_eq("(1 2 3)", &o, &n);
}

#[test]
fn oracle_prop_copy_sequence_not_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((lst '(1 2 3)))
                  (eq lst (copy-sequence lst)))";
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_copy_sequence_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((lst '(1 2 3)))
                  (equal lst (copy-sequence lst)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_copy_sequence_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(copy-sequence "hello")"#, expect);
    assert_ok_eq(r#""hello""#, &o, &n);
}

#[test]
fn oracle_prop_copy_sequence_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [1 2 3]""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(copy-sequence [1 2 3])", expect);
    assert_ok_eq("[1 2 3]", &o, &n);
}

#[test]
fn oracle_prop_copy_sequence_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(copy-sequence nil)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_copy_sequence_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(copy-sequence "")"#, expect);
    assert_ok_eq(r#""""#, &o, &n);
}

#[test]
fn oracle_prop_copy_sequence_mutation_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mutating the copy should not affect the original
    let form = "(let* ((orig '(1 2 3))
                       (copy (copy-sequence orig)))
                  (setcar copy 99)
                  (list orig copy))";
    let expect = expect_test::expect![[r#""OK ((1 2 3) (99 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
