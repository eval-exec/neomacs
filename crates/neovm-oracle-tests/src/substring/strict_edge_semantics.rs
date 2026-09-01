//! Oracle parity tests for `substring` — strict edge cases.
//!
//! GNU src/fns.c `Fsubstring`: FROM and TO can be negative (counting
//! from end).  nil TO means end-of-string.  Out-of-range indices signal
//! `args-out-of-range`.  These edges are historically bug-prone.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_substring_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"el\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 1 3)"#, expect);
    assert_ok_eq("\"el\"", &o, &n);
}

#[test]
fn oracle_substring_from_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"he\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 0 2)"#, expect);
    assert_ok_eq("\"he\"", &o, &n);
}

#[test]
fn oracle_substring_omit_to() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"llo\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 2)"#, expect);
    assert_ok_eq("\"llo\"", &o, &n);
}

#[test]
fn oracle_substring_nil_to_means_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"llo\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 2 nil)"#, expect);
    assert_ok_eq("\"llo\"", &o, &n);
}

#[test]
fn oracle_substring_negative_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"lo\"""#]];
    // -1 = last char, -2 = second to last, etc.
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" -2)"#, expect);
    assert_ok_eq("\"lo\"", &o, &n);
}

#[test]
fn oracle_substring_negative_to() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hell\"""#]];
    // -1 = last char position (exclusive end)
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 0 -1)"#, expect);
    assert_ok_eq("\"hell\"", &o, &n);
}

#[test]
fn oracle_substring_both_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ll\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" -3 -1)"#, expect);
    assert_ok_eq("\"ll\"", &o, &n);
}

#[test]
fn oracle_substring_from_equals_to_returns_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 2 2)"#, expect);
    assert_ok_eq("\"\"", &o, &n);
}

#[test]
fn oracle_substring_from_equals_length_returns_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "abc" 3 3)"#, expect);
    assert_ok_eq("\"\"", &o, &n);
}

#[test]
fn oracle_substring_from_greater_than_to_is_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range \"abc\" 2 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "abc" 2 1)"#, expect);
    assert_err_kind(&o, &n, "args-out-of-range");
}

#[test]
fn oracle_substring_from_negative_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range \"abc\" -10 nil)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "abc" -10)"#, expect);
    assert_err_kind(&o, &n, "args-out-of-range");
}

#[test]
fn oracle_substring_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp 42)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring 42 0)"#, expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}
