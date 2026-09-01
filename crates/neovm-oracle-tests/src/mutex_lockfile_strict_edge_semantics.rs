//! Oracle parity for make-mutex, mutex-name, mutex-lock, lock-file.
//! GNU src/thread.c, src/filelock.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{
    assert_ok_eq, assert_oracle_parity_with_shared_tempdir_expect, eval_oracle_and_neovm,
};

// --- make-mutex ---

#[test]
fn oracle_make_mutex_returns_mutex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(mutexp (make-mutex "test"))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_make_mutex_no_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(mutexp (make-mutex))"#, expect);
    assert_ok_eq("t", &o, &n);
}

// --- mutex-name ---

#[test]
fn oracle_mutex_name_returns_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"my-mutex\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(mutex-name (make-mutex "my-mutex"))"#,
        expect,
    );
    assert_ok_eq("\"my-mutex\"", &o, &n);
}

#[test]
fn oracle_mutex_name_unnamed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(mutex-name (make-mutex))"#, expect);
    // Default name: nil
    assert_ok_eq("nil", &o, &n);
}

// --- mutex-lock ---

#[test]
fn oracle_mutex_lock_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq m (make-mutex "lock-test")) (mutex-lock m))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

// --- lock-file ---

#[test]
fn oracle_lock_file_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "lock-file" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (lock-file file)
               (unlock-file file)))"#,
        expect,
    );
}
