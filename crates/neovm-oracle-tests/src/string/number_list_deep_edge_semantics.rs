//! Oracle parity for deep edge cases: string-to-number, substring,
//! safe-length, proper-list-p, number-to-string.
//! GNU src/editfns.c, src/fns.c, src/data.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- string-to-number deep edges ---

#[test]
fn oracle_string_to_number_octal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 63""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "077" 8)"#, expect);
    assert_ok_eq("63", &o, &n);
}

#[test]
fn oracle_string_to_number_hex_base_16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 255""#]];
    // With explicit base 16, string content should be hex digits
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "ff" 16)"#, expect);
    assert_ok_eq("255", &o, &n);
}

#[test]
fn oracle_string_to_number_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "1010" 2)"#, expect);
    assert_ok_eq("10", &o, &n);
}

#[test]
fn oracle_string_to_number_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -42""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "  -42  ")"#, expect);
    assert_ok_eq("-42", &o, &n);
}

#[test]
fn oracle_string_to_number_non_numeric_returns_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "abc")"#, expect);
    assert_ok_eq("0", &o, &n);
}

// --- substring deep edges ---

#[test]
fn oracle_substring_mid_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ell\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 1 4)"#, expect);
    assert_ok_eq("\"ell\"", &o, &n);
}

#[test]
fn oracle_substring_from_mid_to_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"llo\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 2)"#, expect);
    assert_ok_eq("\"llo\"", &o, &n);
}

#[test]
fn oracle_substring_negative_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hell\"""#]];
    // 0-indexed start, -1 means up to last character exclusive
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 0 -1)"#, expect);
    assert_ok_eq("\"hell\"", &o, &n);
}

// --- safe-length ---

#[test]
fn oracle_safe_length_proper_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(safe-length '(a b c))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_safe_length_dotted_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(safe-length '(a . b))"#, expect);
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_safe_length_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(safe-length nil)"#, expect);
    assert_ok_eq("0", &o, &n);
}

// --- number-to-string ---

#[test]
fn oracle_number_to_string_positive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"255\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string 255)"#, expect);
    assert_ok_eq("\"255\"", &o, &n);
}

#[test]
fn oracle_number_to_string_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"-10\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string -10)"#, expect);
    assert_ok_eq("\"-10\"", &o, &n);
}

#[test]
fn oracle_number_to_string_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"0\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(number-to-string 0)"#, expect);
    assert_ok_eq("\"0\"", &o, &n);
}
