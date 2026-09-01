//! Oracle parity for string compare: `string-lessp`, `string-version-lessp`.
//! GNU src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_string_lessp_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-lessp "abc" "abd")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_string_lessp_false() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-lessp "zzz" "aaa")"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_string_lessp_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-lessp "abc" "abc")"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_string_lessp_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-lessp "" "a")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_string_lessp_empty_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-lessp "" "")"#, expect);
    assert_ok_eq("nil", &o, &n);
}
