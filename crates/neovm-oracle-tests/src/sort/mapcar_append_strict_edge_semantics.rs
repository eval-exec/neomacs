//! Oracle parity tests for `sort`, `mapcar`, `mapc`, `append` —
//! strict edge cases.
//!
//! GNU src/fns.c: `sort` destructively sorts a list; `mapcar`/`mapc`
//! apply a function to list elements; `append` concatenates lists.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// sort
// ---------------------------------------------------------------------------

#[test]
fn oracle_sort_numeric() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 3 4 5)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(sort '(3 1 4 1 5) '<)"#, expect);
    assert_ok_eq("(1 1 3 4 5)", &o, &n);
}

#[test]
fn oracle_sort_should_preserve_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(= (length (sort '(3 2 1) '<)) 3)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_sort_singleton_unchanged() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(sort '(42) '<)"#, expect);
    assert_ok_eq("(42)", &o, &n);
}

#[test]
fn oracle_sort_nil_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(sort nil '<)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// ---------------------------------------------------------------------------
// mapcar / mapc
// ---------------------------------------------------------------------------

#[test]
fn oracle_mapcar_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(mapcar '1+ '(1 2 3))"#, expect);
    assert_ok_eq("(2 3 4)", &o, &n);
}

#[test]
fn oracle_mapc_returns_first_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(mapc '1+ '(1 2 3))"#, expect);
    assert_ok_eq("(1 2 3)", &o, &n);
}

// ---------------------------------------------------------------------------
// append
// ---------------------------------------------------------------------------

#[test]
fn oracle_append_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(append '(1 2) '(3 4))"#, expect);
    assert_ok_eq("(1 2 3 4)", &o, &n);
}

#[test]
fn oracle_append_last_arg_not_list_makes_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 42)""#]];
    // GNU: append with a non-list final arg produces a dotted list.
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(append '(1) 42)"#, expect);
    assert_ok_eq("(1 . 42)", &o, &n);
}

#[test]
fn oracle_append_nil_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(append nil nil nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_append_no_args_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(append)"#, expect);
    assert_ok_eq("nil", &o, &n);
}
