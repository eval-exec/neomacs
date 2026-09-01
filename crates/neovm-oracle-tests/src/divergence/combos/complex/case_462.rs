/// Batch 462: thread/mutex/condvar/atomic/locking deep probes.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx462_make_mutex_condvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:mutex \"test-mutex\") void-function t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (make-mutex "test-mutex") (error (car e)))
      (condition-case e (make-condvar "test-cv") (error (car e)))
      (fboundp 'mutex-lock)
      (fboundp 'condition-wait))"##,
        expect,
    );
}

#[test]
fn div_cx462_thread_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:thread nil) nil wrong-number-of-arguments void-function)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (make-thread (lambda () "hello")) (error (car e)))
      (condition-case e (thread-yield) (error (car e)))
      (condition-case e (thread-name) (error (car e)))
      (condition-case e (thread-alive-p) (error (car e))))"##,
        expect,
    );
}

#[test]
fn div_cx462_thread_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((t1 (make-thread (lambda () (+ 1 2)) "calc")))
      (thread-join t1))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx462_thread_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK quit""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((t1 (make-thread (lambda () (while t (thread-yield))) "looper")))
      (thread-signal t1 'quit nil)
      (thread-join t1))
  (quit (car e))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx462_current_thread() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t (current-thread)))
  (list (threadp t) (thread-name t)))"##,
        expect,
    );
}

#[test]
fn div_cx462_all_threads() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((threads (all-threads)))
  (list (> (length threads) 0) (threadp (car threads))))"##,
        expect,
    );
}

#[test]
fn div_cx462_mutex_lock_unlock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ok""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((m (make-mutex "test")))
      (mutex-lock m)
      (mutex-unlock m)
      'ok)
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx462_atomic_inc_dec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function atomic-inc)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((x 0))
  (atomic-inc x)
  (atomic-inc x)
  (atomic-dec x)
  x)"##,
        expect,
    );
}

#[test]
fn div_cx462_atomic_compare_and_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function atomic-compare-and-swap)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((x 0))
  (list (atomic-compare-and-swap x 0 42)
        (atomic-compare-and-swap x 0 99)
        x))"##,
        expect,
    );
}

#[test]
fn div_cx462_with_mutex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((m (make-mutex "with-mutex")))
      (with-mutex m (+ 1 2)))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx462_threadp_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (threadp (current-thread))
      (threadp (make-thread (lambda () nil) "test"))
      (threadp nil))"##,
        expect,
    );
}

#[test]
fn div_cx462_mutex_owner() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((m (make-mutex "own")))
      (mutex-lock m)
      (list (mutex-owner m))
      (mutex-unlock m))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx462_condition_signal_broadcast() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((cv (make-condvar "cv")))
      (condition-notify cv 'all)
      'ok)
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx462_thread_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((t1 (make-thread (lambda () (error "thread-err")) "err")))
      (thread-join t1))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx462_make_thread_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (make-thread (lambda () 42) "answer")))
  (thread-join t1))"##,
        expect,
    );
}
