//! Oracle parity for list/sequence deep interaction edge cases.
//! GNU src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- nconc ---

#[test]
fn oracle_nconc_nil_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nconc nil '(a b))"#, expect);
    assert_ok_eq("(a b)", &o, &n);
}

#[test]
fn oracle_nconc_nil_second() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nconc '(a) nil)"#, expect);
    assert_ok_eq("(a)", &o, &n);
}

#[test]
fn oracle_nconc_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nconc)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_nconc_destructive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3 4) (3 4) t)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq a '(1 2)) (setq b '(3 4)) (setq c (nconc a b)) (list a b (eq a c)))"#,
        expect,
    );
    // a is modified to (1 2 3 4); c is eq to a
    assert_ok_eq("((1 2 3 4) (3 4) t)", &o, &n);
}

// --- ntake ---

#[test]
fn oracle_ntake_less_than_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(ntake 3 '(a b c d e))"#, expect);
    assert_ok_eq("(a b c)", &o, &n);
}

#[test]
fn oracle_ntake_more_than_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(ntake 10 '(a b))"#, expect);
    assert_ok_eq("(a b)", &o, &n);
}

#[test]
fn oracle_ntake_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(ntake 0 '(a b))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

// --- reverse ---

#[test]
fn oracle_reverse_preserves_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 2 1) (1 2 3))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq orig '(1 2 3)) (setq rev (reverse orig)) (list rev orig))"#,
        expect,
    );
    assert_ok_eq("((3 2 1) (1 2 3))", &o, &n);
}

// --- nreverse ---

#[test]
fn oracle_nreverse_destructive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 2 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq lst (list 1 2 3)) (setq r (nreverse lst)) r)"#,
        expect,
    );
    assert_ok_eq("(3 2 1)", &o, &n);
}

// --- delq / delete interaction ---

#[test]
fn oracle_delq_removes_first_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq lst (list 1 2 3 2 4)) (delq 2 lst) lst)"#,
        expect,
    );
    assert_ok_eq("(1 3 4)", &o, &n);
}
