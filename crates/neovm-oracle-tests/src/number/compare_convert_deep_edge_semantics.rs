//! Oracle parity for number comparison and conversion deep edge cases.
//! GNU src/data.c, src/floatfns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- = with multiple args and types ---

#[test]
fn oracle_eq_integer_equals_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(= 1 1.0)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_eq_three_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(= 5 5 5)"#, expect);
    assert_ok_eq("t", &o, &n);
}

// --- /= (not equal) ---

#[test]
fn oracle_neq_two_different() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(/= 1 2)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_neq_all_different() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(/= 1 2)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_neq_same_is_false() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(/= 5 5)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- < and > with multiple args ---

#[test]
fn oracle_lt_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(< 1 2 3)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_gt_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(> 3 2 1)"#, expect);
    assert_ok_eq("t", &o, &n);
}

// --- <= and >= ---

#[test]
fn oracle_le_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(<= 1 1 2)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_ge_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(>= 3 3 2)"#, expect);
    assert_ok_eq("t", &o, &n);
}

// --- 1+ / 1- ---

#[test]
fn oracle_inc_dec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 42 0 -1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list (1+ 41) (1- 43) (1+ -1) (1- 0))"#,
        expect,
    );
    assert_ok_eq("(42 42 0 -1)", &o, &n);
}

// --- abs ---

#[test]
fn oracle_abs_positive_and_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 0 3.5)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list (abs -5) (abs 5) (abs 0) (abs -3.5))"#,
        expect,
    );
    assert_ok_eq("(5 5 0 3.5)", &o, &n);
}
