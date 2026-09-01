//! Oracle parity for type-system: type-of, max-char, bool-vector-p.
//! GNU src/data.c, src/character.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_type_of_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK integer""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(type-of 42)"#, expect);
    assert_ok_eq("integer", &o, &n);
}

#[test]
fn oracle_type_of_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK string""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(type-of "hello")"#, expect);
    assert_ok_eq("string", &o, &n);
}

#[test]
fn oracle_type_of_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK symbol""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(type-of 'sym)"#, expect);
    assert_ok_eq("symbol", &o, &n);
}

#[test]
fn oracle_type_of_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK cons""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(type-of '(a . b))"#, expect);
    assert_ok_eq("cons", &o, &n);
}

#[test]
fn oracle_type_of_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK vector""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(type-of [1 2 3])"#, expect);
    assert_ok_eq("vector", &o, &n);
}

#[test]
fn oracle_type_of_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK float""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(type-of 3.14)"#, expect);
    assert_ok_eq("float", &o, &n);
}

#[test]
fn oracle_max_char_positive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(> (max-char) 0)"#, expect);
    assert_ok_eq("t", &o, &n);
}

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
