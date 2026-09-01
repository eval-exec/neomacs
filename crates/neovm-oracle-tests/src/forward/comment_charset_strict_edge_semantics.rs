//! Oracle parity for charset operations.
//! GNU src/charset.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_char_charset_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(symbolp (char-charset ?a))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_char_equal_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(char-equal ?a ?A)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_char_equal_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(char-equal ?a ?b)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_characterp_on_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(characterp ?x)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_characterp_on_non_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(characterp 999999999)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_charsetp_on_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(charsetp 'ascii)"#, expect);
    assert_ok_eq("t", &o, &n);
}
