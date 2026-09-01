//! Oracle parity for deep string comparison edge cases.
//! GNU src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- string= ---

#[test]
fn oracle_string_eq_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string= "" "")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_string_eq_case_sensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string= "a" "A")"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_string_eq_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string= "hello" "hello")"#, expect);
    assert_ok_eq("t", &o, &n);
}

// --- string< ---

#[test]
fn oracle_string_lt_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string< "a" "b")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_string_lt_empty_vs_nonempty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string< "" "a")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_string_lt_nonempty_vs_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string< "a" "")"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_string_lt_same_is_false() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string< "abc" "abc")"#, expect);
    assert_ok_eq("nil", &o, &n);
}
