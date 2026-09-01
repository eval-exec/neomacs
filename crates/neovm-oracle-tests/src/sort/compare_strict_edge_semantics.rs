//! Oracle parity for sort + compare-strings strict edges.
//! GNU src/fns.c, src/sort.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_sort_nil_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(sort nil '<)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_sort_vector_returns_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(vectorp (sort [3 1 2] '<))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_sort_length_preserved() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(= (length (sort '(3 1 4 2) '<)) 4)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_compare_strings_equal_returns_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "abc" nil nil "abc" nil nil)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_compare_strings_less_returns_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(< (compare-strings "abc" nil nil "abd" nil nil) 0)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_compare_strings_case_insensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(eq (compare-strings "ABC" nil nil "abc" nil nil) t)"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_sort_called_with_custom_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(sort '(3 1 4 2) (lambda (a b) (< a b)))"#,
        expect,
    );
    assert_ok_eq("(1 2 3 4)", &o, &n);
}
