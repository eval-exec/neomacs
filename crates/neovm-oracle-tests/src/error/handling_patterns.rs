//! Oracle parity tests for complex error handling patterns:
//! `condition-case`, `signal`, `error`, `unwind-protect` in
//! realistic combinations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Error propagation and recovery chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_retry_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Retry pattern: try up to N times, recover from error
    let form = "(let ((attempt 0)
                      (max-retries 3)
                      (result nil))
                  (while (and (not result) (< attempt max-retries))
                    (setq attempt (1+ attempt))
                    (condition-case err
                        (if (< attempt 3)
                            (signal 'error
                                    (list (format \"attempt %d\" attempt)))
                          (setq result (format \"success on %d\" attempt)))
                      (error nil)))
                  (list result attempt))";
    let expect = expect_test::expect![[r#""OK (\"success on 3\" 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_cleanup_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple unwind-protect layers with cleanup tracking
    let form = "(let ((cleanup-log nil))
                  (condition-case nil
                      (unwind-protect
                          (unwind-protect
                              (unwind-protect
                                  (signal 'error '(\"deep error\"))
                                (setq cleanup-log
                                      (cons 'inner cleanup-log)))
                            (setq cleanup-log
                                  (cons 'middle cleanup-log)))
                        (setq cleanup-log
                              (cons 'outer cleanup-log)))
                    (error nil))
                  (nreverse cleanup-log))";
    let expect = expect_test::expect![[r#""OK (inner middle outer)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(inner middle outer)", &o, &n);
}

#[test]
fn oracle_prop_error_selective_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Handle different error types differently
    let form = "(let ((handle-error
                       (lambda (err-type msg)
                         (condition-case err
                             (signal err-type (list msg))
                           (arith-error
                            (cons 'math (cdr err)))
                           (void-variable
                            (cons 'unbound (cdr err)))
                           (wrong-type-argument
                            (cons 'type (cdr err)))
                           (error
                            (cons 'generic (cdr err)))))))
                  (list
                    (funcall handle-error 'arith-error \"div/0\")
                    (funcall handle-error 'void-variable \"x\")
                    (funcall handle-error 'wrong-type-argument \"bad\")
                    (funcall handle-error 'error \"generic\")))";
    let expect = expect_test::expect![[
        r#""OK ((math \"div/0\") (unbound \"x\") (type \"bad\") (generic \"generic\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_in_mapcar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Collect errors while processing a list
    let form = "(let ((errors nil)
                      (results nil))
                  (dolist (x '(1 0 3 0 5))
                    (condition-case err
                        (setq results
                              (cons (/ 10 x) results))
                      (arith-error
                       (setq errors (cons x errors)
                             results (cons 'error results)))))
                  (list (nreverse results)
                        (nreverse errors)))";
    let expect = expect_test::expect![[r#""OK ((10 error 3 error 2) (0 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_with_resource_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate resource acquire/release with error handling
    let form = "(let ((resources nil)
                      (log nil))
                  (let ((acquire
                         (lambda (name)
                           (setq resources (cons name resources)
                                 log (cons (list 'acquire name) log))))
                        (release
                         (lambda (name)
                           (setq resources (delete name resources)
                                 log (cons (list 'release name) log)))))
                    ;; Acquire A, B, fail on C, release B, A
                    (condition-case nil
                        (progn
                          (funcall acquire 'A)
                          (unwind-protect
                              (progn
                                (funcall acquire 'B)
                                (unwind-protect
                                    (signal 'error '(\"fail on C\"))
                                  (funcall release 'B)))
                            (funcall release 'A)))
                      (error nil))
                    (list resources (nreverse log))))";
    let expect =
        expect_test::expect![[r#""OK (nil ((acquire A) (acquire B) (release B) (release A)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error wrapping / re-signaling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_wrap_and_rethrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Wrap error with additional context
    let form = "(condition-case outer-err
                    (condition-case inner-err
                        (signal 'error '(\"original problem\"))
                      (error
                       (signal 'error
                               (list (format \"wrapped: %s\"
                                             (car (cdr inner-err)))))))
                  (error (car (cdr outer-err))))";
    let expect = expect_test::expect![[r#""OK \"wrapped: original problem\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_accumulate_in_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Accumulate all errors from a processing loop
    let form = r#"(let ((items '("42" "bad" "7" "nope" "13"))
                        (successes nil)
                        (failures nil))
                    (dolist (item items)
                      (condition-case nil
                          (let ((n (string-to-number item)))
                            (if (and (> n 0) (not (string= item "0")))
                                (setq successes (cons n successes))
                              (setq failures (cons item failures))))
                        (error
                         (setq failures (cons item failures)))))
                    (list (nreverse successes)
                          (nreverse failures)))"#;
    let expect = expect_test::expect![[r#""OK ((42 7 13) (\"bad\" \"nope\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: exception-safe state machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_safe_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // State machine that handles errors at each transition
    let form = "(let ((state 'init)
                      (history nil)
                      (transitions
                       '((init . process)
                         (process . validate)
                         (validate . complete))))
                  (let ((transition
                         (lambda ()
                           (let ((next (cdr (assq state transitions))))
                             (if next
                                 (progn
                                   (setq history
                                         (cons (cons state next) history))
                                   (setq state next)
                                   next)
                               (signal 'error
                                       (list (format \"no transition from %s\"
                                                     state))))))))
                    (condition-case err
                        (progn
                          (funcall transition)
                          (funcall transition)
                          (funcall transition)
                          ;; This should error - no transition from complete
                          (funcall transition))
                      (error
                       (list state
                             (nreverse history)
                             (car (cdr err)))))))";
    let expect = expect_test::expect![[
        r#""OK (complete ((init . process) (process . validate) (validate . complete)) \"no transition from complete\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unwind-protect returns the body value, not cleanup value
    let form = "(let ((log nil))
                  (let ((result
                         (unwind-protect
                             (progn
                               (setq log (cons 'body log))
                               42)
                           (setq log (cons 'cleanup log))
                           99)))
                    (list result (nreverse log))))";
    let expect = expect_test::expect![[r#""OK (42 (body cleanup))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(42 (body cleanup))", &o, &n);
}
