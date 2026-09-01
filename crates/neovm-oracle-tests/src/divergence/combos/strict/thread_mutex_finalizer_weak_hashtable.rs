//! Strict combo oracle probes, batch 112: thread/mutex API, finalizers,
//! and weak hash table behavior (GC-dependent, guarded).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s6_thread_api_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (fboundp 'make-thread)
      (fboundp 'thread-signal)
      (fboundp 'thread-join)
      (fboundp 'current-thread)
      (fboundp 'all-threads)
      (fboundp 'mutex-lock)
      (fboundp 'mutex-unlock)
      (fboundp 'make-mutex)
      (fboundp 'make-condition-variable)
      (fboundp 'condition-variable-signal)
      (fboundp 'condition-variable-wait)
      (fboundp 'make-finalizer))
"####,
    );
}

#[test]
fn div_s6_mutex_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(condition-case err
    (let ((m (make-mutex "probe-mutex")))
      (list (mutex-lock m)
            (mutex-unlock m)
            (mutex-p m)))
  (void-function (list 'void (car err)))
  (error (list 'err (car err))))
"####,
    );
}

#[test]
fn div_s6_finalizer_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(condition-case err
    (let ((ran nil))
      (make-finalizer (lambda () (setq ran t)))
      (list (functionp (lambda () (setq ran t)))
            ran))
  (void-function (list 'void (car err)))
  (error (list 'err (car err))))
"####,
    );
}

#[test]
fn div_s6_weak_hash_table_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((h (make-hash-table :weakness 'key)))
  (puthash (cons 'a 'b) 1 h)
  (list (hash-table-weakness h)
        (hash-table-count h)
        (hash-table-p h)))
"####,
    );
}
