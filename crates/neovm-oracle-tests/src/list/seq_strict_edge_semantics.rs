//! Oracle parity tests for list/sequence operations: `nthcdr`, `nth`,
//! `safe-length`, `take` — strict edge cases.
//!
//! GNU src/fns.c: list navigation and safe length computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_nthcdr_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nthcdr 0 '(a b c))"#, expect);
    assert_ok_eq("(a b c)", &o, &n);
}

#[test]
fn oracle_nthcdr_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (c d e)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nthcdr 2 '(a b c d e))"#, expect);
    assert_ok_eq("(c d e)", &o, &n);
}

#[test]
fn oracle_nthcdr_beyond_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nthcdr 10 '(a b c))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_nthcdr_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nthcdr 5 nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_nth_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK b""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nth 1 '(a b c))"#, expect);
    assert_ok_eq("b", &o, &n);
}

#[test]
fn oracle_nth_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nth 10 '(a b c))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_safe_length_proper_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(safe-length '(a b c d e))"#, expect);
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_safe_length_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(safe-length nil)"#, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_take_from_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(take 3 '(a b c d e))"#, expect);
    assert_ok_eq("(a b c)", &o, &n);
}

#[test]
fn oracle_take_more_than_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(take 10 '(a b))"#, expect);
    assert_ok_eq("(a b)", &o, &n);
}
