//! Advanced oracle parity tests for error handling patterns.
//!
//! Covers: multiple condition-case handlers with re-signaling, error wrapping,
//! retry with exponential backoff simulation, error aggregation from batch
//! operations, circuit breaker pattern, and transactional all-or-nothing
//! with rollback on error.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multiple condition-case handlers with re-signaling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aeh_multiple_handlers_resignal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner handler catches, logs, wraps, and re-signals to outer handler.
    // Outer handler catches the re-signaled error with enriched context.
    let form = r#"(progn
                    (defvar neovm--test-aeh-log nil)
                    (unwind-protect
                        (progn
                          (setq neovm--test-aeh-log nil)
                          (let ((process-item
                                 (lambda (item)
                                   (condition-case inner-err
                                       (cond
                                        ((= item 0) (/ 1 item))
                                        ((< item 0) (signal 'wrong-type-argument
                                                            (list 'natnump item)))
                                        (t (* item 10)))
                                     (arith-error
                                      (setq neovm--test-aeh-log
                                            (cons (list 'arith-caught item) neovm--test-aeh-log))
                                      ;; Re-signal with enriched data
                                      (signal 'error (list (format "arith-error processing %d" item)
                                                           inner-err)))
                                     (wrong-type-argument
                                      (setq neovm--test-aeh-log
                                            (cons (list 'wta-caught item) neovm--test-aeh-log))
                                      (signal 'error (list (format "type-error processing %d" item)
                                                           inner-err)))))))
                            ;; Outer handler catches the wrapped errors
                            (let ((results nil))
                              (dolist (item '(5 0 3 -2 7))
                                (condition-case outer-err
                                    (setq results
                                          (cons (list 'ok item (funcall process-item item))
                                                results))
                                  (error
                                   (setq results
                                         (cons (list 'err item (cadr outer-err))
                                               results)))))
                              (list (nreverse results)
                                    (nreverse neovm--test-aeh-log)))))
                      (makunbound 'neovm--test-aeh-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (((ok 5 50) (err 0 \"arith-error processing 0\") (ok 3 30) (err -2 \"type-error processing -2\") (ok 7 70)) ((arith-caught 0) (wta-caught -2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error wrapping: catch low-level, signal higher-level
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aeh_error_wrapping_layers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three layers: data-access -> service -> controller
    // Each layer catches errors from below and wraps them with context
    let form = r#"(progn
                    (fset 'neovm--test-data-access
                          (lambda (key)
                            (cond
                             ((string= key "missing")
                              (signal 'void-variable (list (intern key))))
                             ((string= key "corrupt")
                              (signal 'wrong-type-argument (list 'stringp 42)))
                             (t (concat "data:" key)))))
                    (fset 'neovm--test-service
                          (lambda (key)
                            (condition-case err
                                (let ((data (neovm--test-data-access key)))
                                  (concat "processed:" data))
                              (void-variable
                               (signal 'error (list (format "service: key not found: %s" key)
                                                    'data-layer err)))
                              (wrong-type-argument
                               (signal 'error (list (format "service: corrupt data for: %s" key)
                                                    'data-layer err))))))
                    (fset 'neovm--test-controller
                          (lambda (key)
                            (condition-case err
                                (list 'success (neovm--test-service key))
                              (error
                               (list 'failure
                                     (cadr err)     ;; layer tag
                                     (cadr (cdr err)) ;; original error
                                     (car (cdr (cdr (cdr err)))))))))
                    (unwind-protect
                        (list
                         (neovm--test-controller "valid")
                         (neovm--test-controller "missing")
                         (neovm--test-controller "corrupt"))
                      (fmakunbound 'neovm--test-data-access)
                      (fmakunbound 'neovm--test-service)
                      (fmakunbound 'neovm--test-controller)))"#;
    let expect = expect_test::expect![[
        r#""OK ((success \"processed:data:valid\") (failure \"service: key not found: missing\" data-layer (void-variable missing)) (failure \"service: corrupt data for: corrupt\" data-layer (wrong-type-argument stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Retry with exponential backoff simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aeh_retry_exponential_backoff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate retry logic with exponential backoff: track attempt number,
    // delay (simulated), and whether each attempt succeeded or failed.
    // The operation succeeds on the 4th attempt.
    let form = r#"(progn
                    (defvar neovm--test-retry-log nil)
                    (defvar neovm--test-retry-attempt 0)
                    (fset 'neovm--test-flaky-operation
                          (lambda ()
                            (setq neovm--test-retry-attempt (1+ neovm--test-retry-attempt))
                            (if (<= neovm--test-retry-attempt 3)
                                (signal 'error
                                        (list (format "transient failure #%d"
                                                      neovm--test-retry-attempt)))
                              (format "success on attempt %d" neovm--test-retry-attempt))))
                    (fset 'neovm--test-with-retry
                          (lambda (max-retries base-delay fn)
                            "Call FN with retry. Return (ok . result) or (err . last-error)."
                            (let ((attempt 0)
                                  (delay base-delay)
                                  (last-err nil)
                                  (done nil)
                                  (result nil))
                              (while (and (not done) (< attempt max-retries))
                                (setq attempt (1+ attempt))
                                (condition-case err
                                    (progn
                                      (setq neovm--test-retry-log
                                            (cons (list 'attempt attempt 'delay delay) neovm--test-retry-log))
                                      (setq result (funcall fn))
                                      (setq neovm--test-retry-log
                                            (cons (list 'success attempt) neovm--test-retry-log))
                                      (setq done t))
                                  (error
                                   (setq last-err err)
                                   (setq neovm--test-retry-log
                                         (cons (list 'failed attempt (cadr err)) neovm--test-retry-log))
                                   ;; Exponential backoff: double the delay
                                   (setq delay (* delay 2)))))
                              (if done
                                  (cons 'ok result)
                                (cons 'err last-err)))))
                    (unwind-protect
                        (progn
                          (setq neovm--test-retry-log nil)
                          (setq neovm--test-retry-attempt 0)
                          (let ((outcome (neovm--test-with-retry 5 100 'neovm--test-flaky-operation)))
                            (list outcome (nreverse neovm--test-retry-log))))
                      (fmakunbound 'neovm--test-flaky-operation)
                      (fmakunbound 'neovm--test-with-retry)
                      (makunbound 'neovm--test-retry-log)
                      (makunbound 'neovm--test-retry-attempt)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok . \"success on attempt 4\") ((attempt 1 delay 100) (failed 1 \"transient failure #1\") (attempt 2 delay 200) (failed 2 \"transient failure #2\") (attempt 3 delay 400) (failed 3 \"transient failure #3\") (attempt 4 delay 800) (success 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error aggregation: collect all errors from a batch operation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aeh_error_aggregation_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process a batch of items. Some succeed, some fail. Collect ALL errors
    // instead of stopping at the first one. Return both successes and errors.
    let form = r#"(progn
                    (fset 'neovm--test-validate-record
                          (lambda (record)
                            "Validate a record (name age email). Signal error if invalid."
                            (let ((name (nth 0 record))
                                  (age (nth 1 record))
                                  (email (nth 2 record))
                                  (errors nil))
                              ;; Check name
                              (unless (and (stringp name) (> (length name) 0))
                                (setq errors (cons "name is empty or not a string" errors)))
                              ;; Check age
                              (unless (and (integerp age) (>= age 0) (<= age 150))
                                (setq errors (cons (format "age %S is invalid" age) errors)))
                              ;; Check email (must contain @)
                              (when (stringp email)
                                (unless (string-match-p "@" email)
                                  (setq errors (cons (format "email %S missing @" email) errors))))
                              (unless (stringp email)
                                (setq errors (cons "email is not a string" errors)))
                              (when errors
                                (signal 'error (list (nreverse errors))))
                              record)))
                    (unwind-protect
                        (let ((records '(("Alice" 30 "alice@example.com")
                                         ("" 25 "bob@example.com")
                                         ("Charlie" -5 "charlie@example.com")
                                         ("Dave" 40 "dave-no-at")
                                         ("Eve" 28 "eve@example.com")
                                         (nil 200 42)))
                              (successes nil)
                              (failures nil))
                          (let ((idx 0))
                            (dolist (rec records)
                              (condition-case err
                                  (progn
                                    (neovm--test-validate-record rec)
                                    (setq successes
                                          (cons (list 'ok idx (car rec)) successes)))
                                (error
                                 (setq failures
                                       (cons (list 'fail idx (cadr err)) failures))))
                              (setq idx (1+ idx))))
                          (list (nreverse successes)
                                (nreverse failures)
                                (length successes)
                                (length failures)))
                      (fmakunbound 'neovm--test-validate-record)))"#;
    let expect = expect_test::expect![[
        r#""OK (((ok 0 \"Alice\") (ok 4 \"Eve\")) ((fail 1 (\"name is empty or not a string\")) (fail 2 (\"age -5 is invalid\")) (fail 3 (\"email \\\"dave-no-at\\\" missing @\")) (fail 5 (\"name is empty or not a string\" \"age 200 is invalid\" \"email is not a string\"))) 1 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Circuit breaker: fail fast after N consecutive errors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aeh_circuit_breaker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Circuit breaker pattern: after 3 consecutive failures, all subsequent
    // calls fail immediately without executing the operation.
    // A success resets the counter.
    let form = r#"(progn
                    (defvar neovm--test-cb-state nil)
                    (defvar neovm--test-cb-fail-count 0)
                    (defvar neovm--test-cb-threshold 3)
                    (defvar neovm--test-cb-log nil)
                    (fset 'neovm--test-cb-call
                          (lambda (operation)
                            "Execute OPERATION through circuit breaker."
                            (cond
                             ;; Circuit is open (tripped): fail immediately
                             ((eq neovm--test-cb-state 'open)
                              (setq neovm--test-cb-log
                                    (cons (list 'circuit-open 'rejected) neovm--test-cb-log))
                              (signal 'error (list "circuit breaker open")))
                             ;; Circuit is closed or half-open: try the operation
                             (t
                              (condition-case err
                                  (let ((result (funcall operation)))
                                    ;; Success: reset counter
                                    (setq neovm--test-cb-fail-count 0)
                                    (setq neovm--test-cb-state 'closed)
                                    (setq neovm--test-cb-log
                                          (cons (list 'success result) neovm--test-cb-log))
                                    result)
                                (error
                                 ;; Failure: increment counter
                                 (setq neovm--test-cb-fail-count (1+ neovm--test-cb-fail-count))
                                 (setq neovm--test-cb-log
                                       (cons (list 'failure neovm--test-cb-fail-count (cadr err))
                                             neovm--test-cb-log))
                                 (when (>= neovm--test-cb-fail-count neovm--test-cb-threshold)
                                   (setq neovm--test-cb-state 'open)
                                   (setq neovm--test-cb-log
                                         (cons (list 'circuit-tripped neovm--test-cb-fail-count)
                                               neovm--test-cb-log)))
                                 (signal (car err) (cdr err))))))))
                    (unwind-protect
                        (progn
                          (setq neovm--test-cb-state 'closed)
                          (setq neovm--test-cb-fail-count 0)
                          (setq neovm--test-cb-log nil)
                          ;; Sequence of operations: some fail, some succeed
                          (let ((ops (list
                                     (lambda () "ok-1")        ;; success
                                     (lambda () (error "e1"))  ;; fail 1
                                     (lambda () (error "e2"))  ;; fail 2
                                     (lambda () (error "e3"))  ;; fail 3 -> trips
                                     (lambda () "ok-2")        ;; rejected (open)
                                     (lambda () "ok-3")))      ;; rejected (open)
                                (outcomes nil))
                            (dolist (op ops)
                              (condition-case err
                                  (let ((r (neovm--test-cb-call op)))
                                    (setq outcomes (cons (list 'ok r) outcomes)))
                                (error
                                 (setq outcomes (cons (list 'err (cadr err)) outcomes)))))
                            (list (nreverse outcomes)
                                  (nreverse neovm--test-cb-log)
                                  neovm--test-cb-state
                                  neovm--test-cb-fail-count)))
                      (fmakunbound 'neovm--test-cb-call)
                      (makunbound 'neovm--test-cb-state)
                      (makunbound 'neovm--test-cb-fail-count)
                      (makunbound 'neovm--test-cb-threshold)
                      (makunbound 'neovm--test-cb-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (((ok \"ok-1\") (err \"e1\") (err \"e2\") (err \"e3\") (err \"circuit breaker open\") (err \"circuit breaker open\")) ((success \"ok-1\") (failure 1 \"e1\") (failure 2 \"e2\") (failure 3 \"e3\") (circuit-tripped 3) (circuit-open rejected) (circuit-open rejected)) open 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transaction pattern: all-or-nothing with rollback on error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aeh_transaction_rollback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a transactional system: perform a series of mutations,
    // and if any step fails, roll back ALL changes to the pre-transaction state.
    let form = r#"(progn
                    (defvar neovm--test-txn-store nil)
                    (defvar neovm--test-txn-journal nil)
                    (fset 'neovm--test-txn-set
                          (lambda (key value)
                            "Set KEY to VALUE, recording old value in journal for rollback."
                            (let ((old-value (assoc key neovm--test-txn-store)))
                              (setq neovm--test-txn-journal
                                    (cons (list key (if old-value (cdr old-value) 'unset))
                                          neovm--test-txn-journal))
                              (if old-value
                                  (setcdr old-value value)
                                (setq neovm--test-txn-store
                                      (cons (cons key value) neovm--test-txn-store))))))
                    (fset 'neovm--test-txn-rollback
                          (lambda ()
                            "Undo all journal entries in reverse order."
                            (dolist (entry neovm--test-txn-journal)
                              (let ((key (car entry))
                                    (old-val (cadr entry)))
                                (if (eq old-val 'unset)
                                    ;; Key didn't exist before: remove it
                                    (setq neovm--test-txn-store
                                          (assq-delete-all key neovm--test-txn-store))
                                  ;; Restore old value
                                  (let ((cell (assoc key neovm--test-txn-store)))
                                    (when cell (setcdr cell old-val))))))))
                    (fset 'neovm--test-txn-execute
                          (lambda (operations)
                            "Execute OPERATIONS transactionally. Rollback on any error."
                            (let ((neovm--test-txn-journal nil)
                                  (committed nil))
                              (unwind-protect
                                  (progn
                                    (dolist (op operations)
                                      (funcall op))
                                    (setq committed t)
                                    'committed)
                                (unless committed
                                  (neovm--test-txn-rollback))))))
                    (unwind-protect
                        (progn
                          ;; Initialize store
                          (setq neovm--test-txn-store
                                (list (cons 'balance 1000)
                                      (cons 'name "Alice")
                                      (cons 'status 'active)))
                          (let ((before (copy-alist neovm--test-txn-store)))
                            ;; Transaction 1: successful
                            (let ((tx1-result
                                   (condition-case err
                                       (neovm--test-txn-execute
                                        (list
                                         (lambda () (neovm--test-txn-set 'balance 900))
                                         (lambda () (neovm--test-txn-set 'name "Alice Updated"))
                                         (lambda () (neovm--test-txn-set 'last-txn "2024-01-01"))))
                                     (error (list 'tx-failed (cadr err))))))
                              (let ((after-tx1 (copy-alist neovm--test-txn-store)))
                                ;; Transaction 2: fails mid-way, should rollback
                                (let ((tx2-result
                                       (condition-case err
                                           (neovm--test-txn-execute
                                            (list
                                             (lambda () (neovm--test-txn-set 'balance 500))
                                             (lambda () (neovm--test-txn-set 'status 'suspended))
                                             (lambda () (error "payment gateway timeout"))
                                             (lambda () (neovm--test-txn-set 'balance 0))))
                                         (error (list 'tx-failed (cadr err))))))
                                  (let ((after-tx2 (copy-alist neovm--test-txn-store)))
                                    ;; After rollback, store should match post-tx1 state
                                    (list (list 'before before)
                                          (list 'tx1 tx1-result after-tx1)
                                          (list 'tx2 tx2-result after-tx2)
                                          (list 'rollback-ok
                                                (equal after-tx1 after-tx2)))))))))
                      (fmakunbound 'neovm--test-txn-set)
                      (fmakunbound 'neovm--test-txn-rollback)
                      (fmakunbound 'neovm--test-txn-execute)
                      (makunbound 'neovm--test-txn-store)
                      (makunbound 'neovm--test-txn-journal)))"#;
    let expect = expect_test::expect![[
        r#""OK ((before ((balance . 1000) (name . \"Alice\") (status . active))) (tx1 committed ((last-txn . \"2024-01-01\") (balance . 900) (name . \"Alice Updated\") (status . active))) (tx2 (tx-failed \"payment gateway timeout\") ((last-txn . \"2024-01-01\") (balance . 900) (name . \"Alice Updated\") (status . active))) (rollback-ok t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handler priority and specificity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aeh_handler_priority_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that condition-case handlers are tried in order, and that
    // the most specific matching handler wins. Also test that
    // unwind-protect cleanup runs even when handler re-signals.
    let form = r#"(let ((results nil))
                    ;; Test 1: first matching handler wins
                    (condition-case err
                        (/ 1 0)
                      (arith-error
                       (setq results (cons '(first-handler arith) results)))
                      (error
                       (setq results (cons '(second-handler generic) results))))
                    ;; Test 2: generic handler catches when specific doesn't match
                    (condition-case err
                        (signal 'file-error '("not found"))
                      (arith-error
                       (setq results (cons '(arith-handler wrong) results)))
                      (error
                       (setq results (cons (list 'generic-handler (car err)) results))))
                    ;; Test 3: unwind-protect cleanup between nested condition-cases
                    (let ((cleanup-ran nil))
                      (condition-case outer-err
                          (unwind-protect
                              (condition-case nil
                                  (error "inner boom")
                                (arith-error 'wrong-handler))
                            (setq cleanup-ran t))
                        (error
                         (setq results
                               (cons (list 'cleanup-and-catch
                                           cleanup-ran
                                           (cadr outer-err))
                                     results)))))
                    ;; Test 4: condition-case with no matching handler lets error propagate
                    (condition-case outer
                        (condition-case nil
                            (signal 'void-variable '(some-var))
                          (arith-error 'nope)
                          (wrong-type-argument 'nope-2))
                      (void-variable
                       (setq results
                             (cons (list 'propagated (cadr outer)) results))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((first-handler arith) (generic-handler file-error) (cleanup-and-catch t \"inner boom\") (propagated some-var))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
