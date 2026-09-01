//! Complex oracle parity tests for error handling combinations:
//! nested condition-case with different error types, custom error
//! signaling, unwind-protect cleanup ordering, catch/throw + error
//! interaction, retry patterns, and dynamic binding + error handling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Nested condition-case with different error types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_condition_case_type_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer catches generic errors, inner catches specific ones;
    // verify dispatch falls through inner to outer when type doesn't match
    let form = "(let ((dispatch
                       (lambda (err-sym data)
                         (condition-case outer-err
                             (condition-case inner-err
                                 (signal err-sym data)
                               (arith-error
                                (list 'inner-arith (cdr inner-err)))
                               (wrong-type-argument
                                (list 'inner-wta (cdr inner-err))))
                           (void-variable
                            (list 'outer-void (cdr outer-err)))
                           (error
                            (list 'outer-generic (car outer-err)
                                  (cdr outer-err)))))))
                  (list
                   ;; Caught by inner arith-error handler
                   (funcall dispatch 'arith-error '(\"div by zero\"))
                   ;; Caught by inner wrong-type-argument handler
                   (funcall dispatch 'wrong-type-argument '(numberp \"x\"))
                   ;; Falls through inner, caught by outer void-variable
                   (funcall dispatch 'void-variable '(undefined-var))
                   ;; Falls through inner, caught by outer generic error
                   (funcall dispatch 'file-error '(\"not found\"))))";
    let expect = expect_test::expect![[
        r#""OK ((inner-arith (\"div by zero\")) (inner-wta (numberp \"x\")) (outer-void (undefined-var)) (outer-generic file-error (\"not found\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error signaling with custom error data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_custom_error_data_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Signal errors with rich structured data and verify the full
    // error object is accessible in the handler
    let form = r#"(let ((results nil))
                    ;; Test 1: simple string data
                    (condition-case err
                        (signal 'error '("simple message"))
                      (error
                       (setq results (cons (list 'type (car err)
                                                 'data (cdr err))
                                           results))))
                    ;; Test 2: multiple data elements
                    (condition-case err
                        (signal 'wrong-type-argument '(numberp "hello" 42))
                      (wrong-type-argument
                       (setq results (cons (list 'sym (car err)
                                                 'expected (cadr err)
                                                 'got (caddr err)
                                                 'extra (cadddr err))
                                           results))))
                    ;; Test 3: nested list as error data
                    (condition-case err
                        (signal 'error (list (list 'context "fn-name")
                                             (list 'args '(1 2 3))))
                      (error
                       (setq results (cons (list 'nested-data (cdr err))
                                           results))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((type error data (\"simple message\")) (sym wrong-type-argument expected numberp got \"hello\" extra 42) (nested-data ((context \"fn-name\") (args (1 2 3)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// unwind-protect cleanup ordering with nested errors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_ordering_nested_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify cleanup forms run in correct order even when errors
    // occur inside cleanup forms themselves
    let form = "(let ((log nil))
                  (condition-case nil
                      (unwind-protect
                          (unwind-protect
                              (unwind-protect
                                  (progn
                                    (setq log (cons 'body-start log))
                                    (signal 'error '(\"boom\")))
                                ;; Cleanup 1: runs, then signals its own error
                                (setq log (cons 'cleanup-1-start log))
                                (condition-case nil
                                    (signal 'error '(\"cleanup-1-err\"))
                                  (error
                                   (setq log (cons 'cleanup-1-caught log))))
                                (setq log (cons 'cleanup-1-end log)))
                            ;; Cleanup 2: runs normally
                            (setq log (cons 'cleanup-2 log)))
                        ;; Cleanup 3: runs normally
                        (setq log (cons 'cleanup-3 log)))
                    (error nil))
                  (nreverse log))";
    let expect = expect_test::expect![[
        r#""OK (body-start cleanup-1-start cleanup-1-caught cleanup-1-end cleanup-2 cleanup-3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with catch/throw interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_catch_throw_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mix catch/throw with condition-case: throw unwinds through
    // condition-case; errors inside catch body can be caught
    let form = "(let ((log nil))
                  (let ((result
                         (catch 'bail
                           (condition-case err
                               (progn
                                 (setq log (cons 'before-throw log))
                                 ;; Throw bypasses the condition-case entirely
                                 (throw 'bail 'thrown-value)
                                 (setq log (cons 'unreachable log)))
                             (error
                              (setq log (cons 'handler log)))))))
                    (list result (nreverse log))))";
    let expect = expect_test::expect![[r#""OK (thrown-value (before-throw))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_inside_catch_caught_by_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An error inside a catch body is caught by condition-case,
    // not by catch; and catch sees the condition-case handler result
    let form = "(let ((log nil))
                  (let ((result
                         (catch 'tag
                           (setq log (cons 'outer log))
                           (condition-case err
                               (progn
                                 (setq log (cons 'inner log))
                                 (/ 1 0)
                                 (setq log (cons 'unreachable log)))
                             (arith-error
                              (setq log (cons 'caught-error log))
                              'error-handled)))))
                    (list result (nreverse log))))";
    let expect = expect_test::expect![[r#""OK (error-handled (outer inner caught-error))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error recovery and retry pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_recovery_retry_with_backoff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Retry a fallible operation with escalating behavior:
    // first N attempts fail, then succeed; track all attempts
    let form = r#"(let ((attempt-log nil)
                        (fail-until 3)
                        (max-retries 5)
                        (attempt 0)
                        (final-result nil))
                    (catch 'done
                      (while (<= attempt max-retries)
                        (setq attempt (1+ attempt))
                        (condition-case err
                            (progn
                              (setq attempt-log
                                    (cons (list 'try attempt) attempt-log))
                              (if (<= attempt fail-until)
                                  (signal 'error
                                          (list (format "fail-%d" attempt)))
                                (setq final-result
                                      (format "success-on-%d" attempt))
                                (throw 'done nil)))
                          (error
                           (setq attempt-log
                                 (cons (list 'err (cadr err)) attempt-log))))))
                    (list final-result (nreverse attempt-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"success-on-4\" ((try 1) (err \"fail-1\") (try 2) (err \"fail-2\") (try 3) (err \"fail-3\") (try 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding + error handling interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_binding_error_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that let-bound dynamic variables are properly restored
    // when unwinding through errors
    let form = "(progn
                  (defvar neovm--test-dynvar 'initial)
                  (unwind-protect
                      (progn
                        ;; Verify initial binding
                        (let ((before neovm--test-dynvar))
                          ;; Shadow with let
                          (let ((neovm--test-dynvar 'shadowed))
                            (condition-case nil
                                ;; Nest another let binding
                                (let ((neovm--test-dynvar 'deep-shadow))
                                  (let ((deep-val neovm--test-dynvar))
                                    ;; Error unwinds through both lets
                                    (signal 'error '(\"boom\"))))
                              (error
                               ;; After error: should see 'shadowed (inner let unwound)
                               (let ((after-error neovm--test-dynvar))
                                 ;; Return results
                                 (list before 'deep-shadow after-error
                                       neovm--test-dynvar))))))
                    (makunbound 'neovm--test-dynvar)))";
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_with_throw_and_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex scenario: unwind-protect inside catch, cleanup signals
    // an error which is caught by condition-case outside the catch
    let form = "(let ((log nil))
                  (condition-case err
                      (catch 'tag
                        (unwind-protect
                            (progn
                              (setq log (cons 'body log))
                              (throw 'tag 'thrown))
                          ;; Cleanup runs despite throw
                          (setq log (cons 'cleanup log))))
                    (error
                     (setq log (cons (list 'error (cdr err)) log))))
                  ;; catch returns the thrown value, cleanup still ran
                  (nreverse log))";
    let expect = expect_test::expect![[r#""OK (body cleanup)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
