//! Oracle parity for throw/catch + unwind-protect strict edges.
//! GNU src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_catch_returns_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(catch 'x 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_catch_catches_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK val""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(catch 'tag (throw 'tag 'val))"#, expect);
    assert_ok_eq("val", &o, &n);
}

#[test]
fn oracle_unwind_protect_runs_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (body cleanup)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defvar neovm--test-uwp-log '()) (unwind-protect (progn (setq neovm--test-uwp-log (cons 'body neovm--test-uwp-log)) 42) (setq neovm--test-uwp-log (cons 'cleanup neovm--test-uwp-log))) (nreverse neovm--test-uwp-log))"#,
        expect,
    );
    assert_ok_eq("(body cleanup)", &o, &n);
}

#[test]
fn oracle_unwind_protect_cleanup_after_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK cleaned""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defvar neovm--test-ut-log '()) (catch 'exit (unwind-protect (throw 'exit 'result) (setq neovm--test-ut-log 'cleaned))) neovm--test-ut-log)"#,
        expect,
    );
    assert_ok_eq("cleaned", &o, &n);
}

#[test]
fn oracle_throw_value_is_evaluated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(catch 't (+ 1 2 3))"#, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_nested_catch_inner_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (i after)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(catch 'outer (list (catch 'inner (throw 'inner 'i)) 'after))"#,
        expect,
    );
    assert_ok_eq("(i after)", &o, &n);
}

#[test]
fn oracle_catch_tag_quote_is_caught() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK caught""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(catch 'tag (throw 'tag 'caught))"#, expect);
    assert_ok_eq("caught", &o, &n);
}
