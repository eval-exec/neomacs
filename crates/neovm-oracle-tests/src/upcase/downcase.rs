//! Oracle parity tests for `upcase`, `downcase`, `capitalize`, and related.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_upcase_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"HELLO\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(upcase "hello")"#, expect);
    assert_ok_eq(r#""HELLO""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"HELLO WORLD\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(upcase "Hello World")"#, expect);
    assert_ok_eq(r#""HELLO WORLD""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"ALREADY\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(upcase "ALREADY")"#, expect);
    assert_ok_eq(r#""ALREADY""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(upcase "")"#, expect);
    assert_ok_eq(r#""""#, &o, &n);
}

#[test]
fn oracle_prop_downcase_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(downcase "HELLO")"#, expect);
    assert_ok_eq(r#""hello""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(downcase "Hello World")"#, expect);
    assert_ok_eq(r#""hello world""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"already\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(downcase "already")"#, expect);
    assert_ok_eq(r#""already""#, &o, &n);
}

#[test]
fn oracle_prop_upcase_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 65""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(upcase ?a)", expect);
    assert_ok_eq("65", &o, &n);

    let expect = expect_test::expect![[r#""OK 65""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(upcase ?A)", expect);
    assert_ok_eq("65", &o, &n);
}

#[test]
fn oracle_prop_downcase_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 97""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(downcase ?A)", expect);
    assert_ok_eq("97", &o, &n);

    let expect = expect_test::expect![[r#""OK 97""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(downcase ?a)", expect);
    assert_ok_eq("97", &o, &n);
}

#[test]
fn oracle_prop_capitalize_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"Hello World\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(capitalize "hello world")"#, expect);
    assert_ok_eq(r#""Hello World""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"Hello World\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(capitalize "HELLO WORLD")"#, expect);
    assert_ok_eq(r#""Hello World""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"Hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(capitalize "hello")"#, expect);
    assert_ok_eq(r#""Hello""#, &o, &n);
}

#[test]
fn oracle_prop_upcase_downcase_with_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"ABC123DEF\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(upcase "abc123def")"#, expect);
    assert_ok_eq(r#""ABC123DEF""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"abc123def\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(downcase "ABC123DEF")"#, expect);
    assert_ok_eq(r#""abc123def""#, &o, &n);
}

#[test]
fn oracle_prop_upcase_downcase_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(string-equal (downcase (upcase "hello")) "hello")"####;
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_upcase_initials() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"Hello World\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(upcase-initials "hello world")"#, expect);
    assert_ok_eq(r#""Hello World""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"HELLO WORLD\"""#]];
    // upcase-initials only capitalizes first letter of each word, preserves rest
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(upcase-initials "hELLO wORLD")"#, expect);
    assert_ok_eq(r#""HELLO WORLD""#, &o, &n);
}

#[test]
fn oracle_prop_mapcar_upcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(mapcar 'upcase '("foo" "bar" "baz"))"####;
    let expect = expect_test::expect![[r#""OK (\"FOO\" \"BAR\" \"BAZ\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
