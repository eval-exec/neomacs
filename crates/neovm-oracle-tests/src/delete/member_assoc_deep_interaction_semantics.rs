//! Oracle parity for delete, delq, member, memq, assoc, assq deep interaction.
//! GNU src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_delete_removes_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"b\")""#]];
    // delete returns the list with matching elements removed
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(delete "a" (list "a" "b" "a"))"#, expect);
    assert_ok_eq("(\"b\")", &o, &n);
}

#[test]
fn oracle_delq_removes_by_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3)""#]];
    // delq is destructive: use the returned value
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(delq 1 (list 1 2 1 3))"#, expect);
    assert_ok_eq("(2 3)", &o, &n);
}

#[test]
fn oracle_member_returns_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (c d e)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(member 'c '(a b c d e))"#, expect);
    assert_ok_eq("(c d e)", &o, &n);
}

#[test]
fn oracle_member_not_found() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(member 'z '(a b c))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_memq_uses_eq_not_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(memq (make-string 2 ?a) (list (make-string 2 ?a)))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_assoc_finds_first_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a . 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(assoc 'a '((a . 1) (b . 2) (a . 3)))"#,
        expect,
    );
    assert_ok_eq("(a . 1)", &o, &n);
}

#[test]
fn oracle_assq_nil_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(assq 'c '((a . 1) (b . 2)))"#, expect);
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
fn oracle_assoc_on_non_cons_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (b . 2)""#]];
    // assoc skips non-cons elements (a), matches cons (b . 2)
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(assoc 'b '(a (b . 2) (c . 3)))"#, expect);
    assert_ok_eq("(b . 2)", &o, &n);
}

#[test]
fn oracle_member_dotted_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (c . d)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(member 'c '(a b c . d))"#, expect);
    assert_ok_eq("(c . d)", &o, &n);
}
