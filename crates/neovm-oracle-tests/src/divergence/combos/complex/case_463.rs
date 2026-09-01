/// Batch 463: more thread/concurrency, mutex/condvar, atomic stress.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx463_mutex_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"named-mutex\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (mutex-name (make-mutex "named-mutex"))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_condition_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (condition-name (make-condvar "named-cv"))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_thread_name_non_main() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((t1 (make-thread (lambda () (thread-name)) "worker")))
      (thread-join t1))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_mutex_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ok""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((m (make-mutex "rec")))
      (mutex-lock m)
      (mutex-lock m)
      (mutex-unlock m)
      (mutex-unlock m)
      'ok)
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_condition_wait_timeout() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((m (make-mutex "cv-timeout"))
          (cv (make-condvar "cv-timeout")))
      (mutex-lock m)
      (condition-wait cv m (seconds-to-time 0.01))
      (mutex-unlock m)
      'ok)
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_thread_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((t1 (make-thread (lambda (a b) (+ a b)) "sum" 3 4)))
      (thread-join t1))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_all_threads_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (length (all-threads))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_mutexp_condvarp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t void-function nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (mutexp (make-mutex "test"))
      (condition-case e (condvarp (make-condvar "test")) (error (car e)))
      (mutexp nil)
      (mutexp "string"))"##,
        expect,
    );
}

#[test]
fn div_cx463_atomic_add_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function atomic-inc)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((x 10))
  (atomic-inc x 5)
  x)"##,
        expect,
    );
}

#[test]
fn div_cx463_with_mutex_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 45""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (with-mutex (make-mutex "test-ret")
      (let ((x 0))
        (dotimes (i 10) (setq x (+ x i)))
        x))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_threadp_on_thread_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (condition-case e (make-thread (lambda () 1) "test-thread") (error nil))))
  (if t1 (threadp t1) nil))"##,
        expect,
    );
}

#[test]
fn div_cx463_mutex_name_different() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello-world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (mutex-name (make-mutex "hello-world"))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_thread_join_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"result\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((t1 (make-thread (lambda () "result") "result-thread")))
      (thread-join t1))
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_mutex_unlock_not_owned() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((m (make-mutex "not-owned")))
      (mutex-unlock m)
      'ok)
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx463_thread_exit_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((t1 (make-thread (lambda () 99) "exit-status")))
      (thread-join t1))
  (error (car e)))"##,
        expect,
    );
}
