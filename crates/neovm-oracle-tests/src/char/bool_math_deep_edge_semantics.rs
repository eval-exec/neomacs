//! Oracle parity for char-equal, bool-vector, min/max, and bitwise edge cases.
//! GNU src/fns.c, src/data.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- char-equal (case-insensitive) ---

#[test]
fn oracle_char_equal_case_insensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(char-equal ?a ?A)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_char_equal_same_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(char-equal ?a ?a)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_char_equal_different() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(char-equal ?a ?b)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- bool-vector-p ---

#[test]
fn oracle_bool_vector_p_on_bool_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(bool-vector-p (bool-vector t nil))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bool_vector_p_on_regular_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(bool-vector-p [t nil])"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- min / max identity on single arg ---

#[test]
fn oracle_min_single_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(min -5)"#, expect);
    assert_ok_eq("-5", &o, &n);
}

#[test]
fn oracle_max_single_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(max -5)"#, expect);
    assert_ok_eq("-5", &o, &n);
}

#[test]
fn oracle_min_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(min 3 1 4 1 5)"#, expect);
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_max_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(max 3 1 4 1 5)"#, expect);
    assert_ok_eq("5", &o, &n);
}

// --- bitwise operations ---

#[test]
fn oracle_logand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(logand 6 3)"#, expect);
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_logior_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(logior 1 2 4)"#, expect);
    assert_ok_eq("7", &o, &n);
}

#[test]
fn oracle_logxor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(logxor 5 3)"#, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_lognot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(lognot 0)"#, expect);
    assert_ok_eq("-1", &o, &n);
}

// --- string-to-char ---

#[test]
fn oracle_string_to_char_first_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 65""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-to-char "ABC")"#, expect);
    assert_ok_eq("65", &o, &n);
}

#[test]
fn oracle_string_to_char_empty_returns_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-to-char "")"#, expect);
    assert_ok_eq("0", &o, &n);
}
