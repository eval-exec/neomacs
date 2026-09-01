//! Advanced oracle parity tests for error handling combination patterns.
//!
//! Covers: retry with backoff, circuit breaker, error aggregation,
//! timeout simulation, fallback chains, resource pools, transactions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Retry with exponential backoff simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_err_adv_retry_exponential_backoff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate retry with exponential backoff: attempt counter doubles
    // each retry.  Succeeds on attempt 4.  Track delays.
    let form = r#"(let ((attempt 0)
                        (delay 1)
                        (delays nil)
                        (result nil))
                    (while (and (not result) (< attempt 5))
                      (setq attempt (1+ attempt))
                      (condition-case err
                          (if (< attempt 4)
                              (progn
                                (setq delays (cons delay delays))
                                (setq delay (* delay 2))
                                (signal 'error
                                        (list (format "fail-%d" attempt))))
                            (setq result (format "ok-on-%d" attempt)))
                        (error nil)))
                    (list result attempt (nreverse delays)))"#;
    let expect = expect_test::expect![[r#""OK (\"ok-on-4\" 4 (1 2 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Circuit breaker pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_err_adv_circuit_breaker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Circuit breaker: after 3 consecutive failures, go to "open"
    // state and fail fast without attempting the operation.
    let form = r#"(let ((failure-count 0)
                        (threshold 3)
                        (state 'closed)
                        (log nil))
                    ;; Define the operation as a list of outcomes
                    (let ((outcomes '(fail fail fail fail succeed))
                          (idx 0))
                      (dotimes (_ 5)
                        (cond
                          ((eq state 'open)
                           (setq log (cons (list idx 'fast-fail) log)))
                          (t
                           (let ((outcome (nth idx outcomes)))
                             (condition-case nil
                                 (if (eq outcome 'fail)
                                     (progn
                                       (setq failure-count (1+ failure-count))
                                       (when (>= failure-count threshold)
                                         (setq state 'open))
                                       (signal 'error '("operation failed")))
                                   (setq log (cons (list idx 'success) log)
                                         failure-count 0))
                               (error
                                (setq log (cons (list idx 'failed) log)))))))
                        (setq idx (1+ idx))))
                    (list state failure-count (nreverse log)))"#;
    let expect = expect_test::expect![[
        r#""OK (open 3 ((0 failed) (1 failed) (2 failed) (3 fast-fail) (4 fast-fail)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error aggregation (collect all errors, don't stop at first)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_err_adv_aggregate_all_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process all items, collect errors and successes separately,
    // then return both.  Validates division, type checks, bounds.
    let form = r#"(let ((items '((div 10 2) (div 10 0) (div 15 3)
                                  (div 7 0) (div 100 5) (div 0 0)))
                        (results nil)
                        (errors nil))
                    (dolist (item items)
                      (let ((op (nth 0 item))
                            (a (nth 1 item))
                            (b (nth 2 item)))
                        (condition-case err
                            (cond
                              ((eq op 'div)
                               (setq results
                                     (cons (list a b (/ a b)) results)))
                              (t (signal 'error
                                         (list (format "unknown op: %s" op)))))
                          (arith-error
                           (setq errors
                                 (cons (list a b 'div-by-zero) errors)))
                          (error
                           (setq errors
                                 (cons (list a b (car (cdr err))) errors))))))
                    (list (nreverse results) (nreverse errors)))"#;
    let expect = expect_test::expect![[
        r#""OK (((10 2 5) (15 3 5) (100 5 20)) ((10 0 div-by-zero) (7 0 div-by-zero) (0 0 div-by-zero)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Timeout simulation using catch/throw with counter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_err_adv_timeout_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a timeout: a "long-running" loop that throws 'timeout
    // after exceeding a step budget.
    let form = r#"(let ((budget 10)
                        (steps 0))
                    (catch 'timeout
                      (let ((result nil)
                            (i 1))
                        (while t
                          (setq steps (1+ steps))
                          (when (> steps budget)
                            (throw 'timeout
                                   (list 'timed-out steps (nreverse result))))
                          ;; "Expensive" computation: accumulate Collatz steps
                          (let ((n i) (count 0))
                            (while (> n 1)
                              (setq steps (1+ steps))
                              (when (> steps budget)
                                (throw 'timeout
                                       (list 'timed-out steps (nreverse result))))
                              (setq n (if (= (% n 2) 0) (/ n 2) (1+ (* 3 n)))
                                    count (1+ count)))
                            (setq result (cons (cons i count) result)))
                          (setq i (1+ i))))))"#;
    let expect = expect_test::expect![[r#""OK (timed-out 11 ((1 . 0) (2 . 1)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error recovery with fallback chain (try A, else B, else C)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_err_adv_fallback_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Try multiple strategies in order; first success wins.
    let form = r#"(let ((strategies
                         (list
                           ;; Strategy A: fails for input > 10
                           (lambda (x)
                             (if (> x 10)
                                 (signal 'error '("A: too large"))
                               (list 'strategy-a (* x 2))))
                           ;; Strategy B: fails for odd input
                           (lambda (x)
                             (if (= (% x 2) 1)
                                 (signal 'error '("B: odd number"))
                               (list 'strategy-b (* x 3))))
                           ;; Strategy C: always works (fallback)
                           (lambda (x)
                             (list 'strategy-c x)))))
                    (let ((try-all
                           (lambda (input fns)
                             (let ((result nil)
                                   (remaining fns))
                               (while (and (not result) remaining)
                                 (condition-case nil
                                     (setq result (funcall (car remaining) input))
                                   (error nil))
                                 (setq remaining (cdr remaining)))
                               (or result (list 'all-failed input))))))
                      (list
                        (funcall try-all 5 strategies)    ; A succeeds
                        (funcall try-all 12 strategies)   ; A fails, B succeeds (even)
                        (funcall try-all 15 strategies)   ; A,B fail, C catches
                        (funcall try-all 8 strategies)))) ; A succeeds"#;
    let expect = expect_test::expect![[
        r#""OK ((strategy-a 10) (strategy-b 36) (strategy-c 15) (strategy-a 16))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error-safe resource pool
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_err_adv_resource_pool() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pool of 3 resources.  Acquire, use (may fail), always release.
    // Track pool state and operation log.
    let form = r#"(let ((pool '(r1 r2 r3))
                        (in-use nil)
                        (log nil))
                    (let ((acquire
                           (lambda ()
                             (if (null pool)
                                 (signal 'error '("pool exhausted"))
                               (let ((r (car pool)))
                                 (setq pool (cdr pool)
                                       in-use (cons r in-use)
                                       log (cons (list 'acquire r) log))
                                 r))))
                          (release
                           (lambda (r)
                             (setq in-use (delete r in-use)
                                   pool (cons r pool)
                                   log (cons (list 'release r) log)))))
                      ;; Operation 1: acquire + succeed + release
                      (let ((r (funcall acquire)))
                        (unwind-protect
                            (setq log (cons (list 'use r 'ok) log))
                          (funcall release r)))
                      ;; Operation 2: acquire + fail + release via unwind
                      (condition-case nil
                          (let ((r (funcall acquire)))
                            (unwind-protect
                                (progn
                                  (setq log (cons (list 'use r 'about-to-fail) log))
                                  (signal 'error '("boom")))
                              (funcall release r)))
                        (error nil))
                      ;; Operation 3: acquire two, use both, release both
                      (let ((r1 (funcall acquire))
                            (r2 (funcall acquire)))
                        (unwind-protect
                            (setq log (cons (list 'use-both r1 r2) log))
                          (funcall release r2)
                          (funcall release r1)))
                      ;; Return: pool should be fully restored, log in order
                      (list (length pool) (length in-use) (nreverse log))))"#;
    let expect = expect_test::expect![[
        r#""OK (3 0 ((acquire r1) (use r1 ok) (release r1) (acquire r1) (use r1 about-to-fail) (release r1) (acquire r1) (acquire r2) (use-both r1 r2) (release r2) (release r1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transaction with savepoints and partial rollback
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_err_adv_transaction_savepoints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a transaction log with savepoints.  On error, roll back
    // to the last savepoint (discard operations after it).
    let form = r#"(let ((tx-log nil)
                        (committed nil)
                        (savepoints nil))
                    (let ((do-op
                           (lambda (op)
                             (setq tx-log (cons op tx-log))))
                          (savepoint
                           (lambda (name)
                             (setq savepoints
                                   (cons (cons name (length tx-log))
                                         savepoints))))
                          (rollback-to
                           (lambda (name)
                             (let ((sp (assq name savepoints)))
                               (if (not sp)
                                   (signal 'error
                                           (list (format "no savepoint: %s" name)))
                                 ;; Truncate tx-log to savepoint length
                                 (let ((keep (cdr sp))
                                       (new-log nil)
                                       (count 0))
                                   (dolist (entry (nreverse tx-log))
                                     (when (< count keep)
                                       (setq new-log (cons entry new-log)))
                                     (setq count (1+ count)))
                                   (setq tx-log (nreverse new-log)))))))
                          (commit
                           (lambda ()
                             (setq committed (nreverse tx-log)
                                   tx-log nil
                                   savepoints nil))))
                      ;; Begin transaction
                      (funcall do-op 'insert-user)
                      (funcall savepoint 'sp1)
                      (funcall do-op 'insert-order)
                      (funcall do-op 'insert-payment)
                      (funcall savepoint 'sp2)
                      (funcall do-op 'send-email)
                      ;; Email fails -> rollback to sp2
                      (condition-case nil
                          (signal 'error '("email service down"))
                        (error
                         (funcall rollback-to 'sp2)))
                      ;; Retry with different approach
                      (funcall do-op 'queue-email)
                      (funcall commit)
                      committed))"#;
    let expect =
        expect_test::expect![[r#""OK (insert-payment insert-order insert-user queue-email)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
