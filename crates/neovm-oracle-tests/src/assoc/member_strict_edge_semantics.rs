//! Oracle parity tests for `assoc`, `assq`, `rassoc`, `member`, `memq` —
//! strict edge cases with non-cons elements and dotted lists.
//!
//! GNU src/fns.c: These functions iterate a list, applying `equal` or `eq`
//! to the appropriate part. Non-cons elements cause `wrong-type-argument`.
//! Dotted lists are treated as proper up to the non-nil cdr.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_assoc_finds_by_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (b . 2)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(assoc 'b '((a . 1) (b . 2) (c . 3)))"#,
        expect,
    );
    assert_ok_eq("(b . 2)", &o, &n);
}

#[test]
fn oracle_assoc_returns_nil_for_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(assoc 'x '((a . 1) (b . 2)))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_assq_uses_eq_not_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // "key" as string — assq uses eq, string identity fails
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(assq "key" '(("key" . 1)))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_assoc_uses_equal_for_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"key\" . 1)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(assoc "key" '(("key" . 1)))"#, expect);
    assert_ok_eq("(\"key\" . 1)", &o, &n);
}

#[test]
fn oracle_assoc_non_cons_element_skipped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // GNU: assoc silently skips non-cons elements (not an error).
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(assoc 'a '(a (b . 2)))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_rassoc_finds_by_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (b . 2)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(rassoc 2 '((a . 1) (b . 2) (c . 3)))"#,
        expect,
    );
    assert_ok_eq("(b . 2)", &o, &n);
}

#[test]
fn oracle_member_finds_by_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (b c)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(member 'b '(a b c))"#, expect);
    assert_ok_eq("(b c)", &o, &n);
}

#[test]
fn oracle_member_nil_not_found() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(member 'x '(a b c))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_memq_uses_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // memq uses eq, so distinct string objects won't match
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(memq "hello" '("hello"))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_member_dotted_list_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (c . d)""#]];
    // dotted list: member treats cdr as the next element until non-cons
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(member 'c '(a b c . d))"#, expect);
    assert_ok_eq("(c . d)", &o, &n);
}

#[test]
fn oracle_assoc_empty_alist_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(assoc 'a nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_assoc_nil_key_found() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil . 1)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(assoc nil '((nil . 1) (2 . 3)))"#, expect);
    assert_ok_eq("(nil . 1)", &o, &n);
}

#[test]
fn oracle_rassq_found_by_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a . x)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(rassq 'x '((a . x) (b . y)))"#, expect);
    assert_ok_eq("(a . x)", &o, &n);
}

#[test]
fn oracle_rassq_not_found() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(rassq 'z '((a . x) (b . y)))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_memq_first_match_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b a)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(memq 'a '(a b a))"#, expect);
    assert_ok_eq("(a b a)", &o, &n);
}
