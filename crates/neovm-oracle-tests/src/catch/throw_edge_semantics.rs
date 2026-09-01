//! Oracle parity tests for `catch`/`throw` edge cases.
//!
//! GNU src/eval.c: `catch`/`throw` implement non-local exits with tag
//! matching. Edges around nested catches, tag identity, and error interaction
//! are historically bug-prone.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_catch_returns_body_when_no_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(catch 'tag 42)", expect);
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_catch_catches_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK thrown-value""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(catch 'my-tag
         (throw 'my-tag 'thrown-value))"#,
        expect,
    );
    assert_ok_eq("thrown-value", &oracle, &neovm);
}

#[test]
fn oracle_catch_nested_inner_caught_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (before inner-caught after)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(catch 'outer
         (list 'before
               (catch 'inner
                 (throw 'inner 'inner-caught))
               'after))"#,
        expect,
    );
    assert_ok_eq("(before inner-caught after)", &oracle, &neovm);
}

#[test]
fn oracle_catch_throw_passes_through_to_outer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK from-inner""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(catch 'outer
         (catch 'inner
           (throw 'outer 'from-inner))
         'never-reached)"#,
        expect,
    );
    assert_ok_eq("from-inner", &oracle, &neovm);
}

#[test]
fn oracle_catch_throw_value_is_evaluated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(catch 'tag
         (throw 'tag (+ 1 2 3)))"#,
        expect,
    );
    assert_ok_eq("6", &oracle, &neovm);
}

#[test]
fn oracle_catch_unwind_protect_runs_before_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (cleanup body)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defvar neovm--test-ct-uwp-log '())
  (catch 'exit
    (unwind-protect
        (progn
          (setq neovm--test-ct-uwp-log (cons 'body neovm--test-ct-uwp-log))
          (throw 'exit 'result))
      (setq neovm--test-ct-uwp-log (cons 'cleanup neovm--test-ct-uwp-log))))
  neovm--test-ct-uwp-log)"#,
        expect,
    );
    assert_ok_eq("(cleanup body)", &oracle, &neovm);
}

#[test]
fn oracle_catch_tag_must_match_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 42)""#]];
    // GNU: catch tags are compared with eq.
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list
   (catch 'x (throw 'x 1))
   (catch 'y 42))"#,
        expect,
    );
    assert_ok_eq("(1 42)", &oracle, &neovm);
}

#[test]
fn oracle_throw_without_catch_is_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK uncaught-throw-signaled""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (throw 'no-such-tag 42)
       (error 'uncaught-throw-signaled))"#,
        expect,
    );
    assert_ok_eq("uncaught-throw-signaled", &oracle, &neovm);
}
