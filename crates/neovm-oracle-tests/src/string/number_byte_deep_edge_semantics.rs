//! Oracle parity for string-to-number, byte ops, format deep edges.
//! GNU src/editfns.c, src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_string_to_number_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 15""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "1111" 2)"#, expect);
    assert_ok_eq("15", &o, &n);
}

#[test]
fn oracle_string_to_number_octal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 63""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "77" 8)"#, expect);
    assert_ok_eq("63", &o, &n);
}

#[test]
fn oracle_string_to_number_hex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 255""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "ff" 16)"#, expect);
    assert_ok_eq("255", &o, &n);
}

#[test]
fn oracle_string_to_number_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-to-number "")"#, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_byte_to_string_A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(byte-to-string 65)"#, expect);
    assert_ok_eq("\"A\"", &o, &n);
}

#[test]
fn oracle_string_to_char_returns_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 88""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-to-char "XYZ")"#, expect);
    assert_ok_eq("88", &o, &n);
}

#[test]
fn oracle_format_percent_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"%\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%%")"#, expect);
    assert_ok_eq("\"%\"", &o, &n);
}

#[test]
fn oracle_format_char_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "%c" 65)"#, expect);
    assert_ok_eq("\"A\"", &o, &n);
}

#[test]
fn oracle_format_multiple_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"42 items\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(format "%d %s" 42 "items")"#, expect);
    assert_ok_eq("\"42 items\"", &o, &n);
}

#[test]
fn oracle_format_S_vs_s() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"foo\" \"bar\")""#]];
    // %S prints with prin1 (readable), %s prints without quotes
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list (format "%S" 'foo) (format "%s" "bar"))"#,
        expect,
    );
    assert_ok_eq("(\"foo\" \"bar\")", &o, &n);
}
