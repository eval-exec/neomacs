//! Oracle parity for case conversion + string/char ops.
//! GNU src/casefiddle.c, src/editfns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_downcase_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(downcase "HELLO")"#, expect);
    assert_ok_eq("\"hello\"", &o, &n);
}

#[test]
fn oracle_upcase_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"HELLO\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(upcase "hello")"#, expect);
    assert_ok_eq("\"HELLO\"", &o, &n);
}

#[test]
fn oracle_capitalize_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Hello World\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(capitalize "hello world")"#, expect);
    assert_ok_eq("\"Hello World\"", &o, &n);
}

#[test]
fn oracle_upcase_initials_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Hello World\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(upcase-initials "hello world")"#, expect);
    assert_ok_eq("\"Hello World\"", &o, &n);
}

#[test]
fn oracle_downcase_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(downcase "HeLLo WoRLD")"#, expect);
    assert_ok_eq("\"hello world\"", &o, &n);
}

#[test]
fn oracle_string_to_char_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 97""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-to-char "a")"#, expect);
    assert_ok_eq("97", &o, &n);
}

#[test]
fn oracle_char_to_string_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 65""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-to-char (char-to-string 65))"#,
        expect,
    );
    assert_ok_eq("65", &o, &n);
}
