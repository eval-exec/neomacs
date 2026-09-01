//! Oracle parity tests for `string-to-number` edge cases.
//!
//! GNU src/lread.c: `string-to-number` parses integers, floats, and
//! handles various edge cases like leading whitespace, trailing garbage,
//! hex/octal/binary notation, and large values.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_string_to_number_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "42")"#, expect);
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -99""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "-99")"#, expect);
    assert_ok_eq("-99", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3.14""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "3.14")"#, expect);
    assert_ok_eq("3.14", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_garbage_returns_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "not-a-number")"#, expect);
    assert_ok_eq("0", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_empty_string_returns_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "")"#, expect);
    assert_ok_eq("0", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_leading_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 123""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "  123")"#, expect);
    assert_ok_eq("123", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_trailing_garbage_ignored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 123""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "123abc")"#, expect);
    assert_ok_eq("123", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_negative_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK -0.5""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "-0.5")"#, expect);
    assert_ok_eq("-0.5", &oracle, &neovm);
}

#[test]
fn oracle_string_to_number_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number 42)"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_string_to_number_large_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(> (string-to-number "999999999999999") 0)"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}
