//! Oracle parity for other-buffer, buffer-local-value, unintern, run-hook-wrapped.
//! GNU src/buffer.c, src/lread.c, src/eval.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- other-buffer ---

#[test]
fn oracle_other_buffer_no_args_returns_live_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(buffer-live-p (other-buffer))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_other_buffer_no_args_returns_bufferp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(bufferp (other-buffer))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_other_buffer_exclude_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(buffer-live-p (other-buffer (current-buffer)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_other_buffer_returns_different_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(not (eq (current-buffer) (other-buffer (current-buffer))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

// --- buffer-local-value ---

#[test]
fn oracle_buffer_local_value_global_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 123""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq blv-test-var 123) (buffer-local-value 'blv-test-var (current-buffer)))"#,
        expect,
    );
    assert_ok_eq("123", &o, &n);
}

#[test]
fn oracle_buffer_local_value_nil_buffer_signals_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"wrong-type-argument\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err (progn (buffer-local-value 'x nil) nil) (error (symbol-name (car err))))"#,
        expect,
    );
    // Both should signal wrong-type-argument
    assert_ok_eq("\"wrong-type-argument\"", &o, &n);
}

// --- unintern ---

#[test]
fn oracle_unintern_nonexistent_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(unintern "nonexistent-sym-xyz-123" obarray)"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_unintern_existing_returns_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (intern "test-sym-to-unintern-42") (unintern "test-sym-to-unintern-42" obarray))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_unintern_with_symbol_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (intern "sym-for-unintern-via-sym") (unintern 'sym-for-unintern-via-sym obarray))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_unintern_removes_from_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (intern "unintern-rm-test") (unintern "unintern-rm-test" obarray) (intern-soft "unintern-rm-test"))"#,
        expect,
    );
    // After unintern, intern-soft should return nil
    assert_ok_eq("nil", &o, &n);
}

// --- run-hook-wrapped ---

#[test]
fn oracle_run_hook_wrapped_empty_hook_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(run-hook-wrapped 'undefined-hook-xyz (lambda (f) f))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}
