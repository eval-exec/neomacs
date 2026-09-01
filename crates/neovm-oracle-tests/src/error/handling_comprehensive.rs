//! Comprehensive oracle parity tests for error handling:
//! `condition-case` with multiple handlers and `:no-error`, `signal` with
//! custom error symbols and data, `error` vs `user-error`, nested
//! `condition-case`, `unwind-protect` cleanup under normal/error/throw exits,
//! `condition-case-unless-debug`, error propagation through call chains,
//! `ignore-errors`, and `with-demoted-errors`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// condition-case with multiple handlers and :no-error clause
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_condition_case_multi_handler_no_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test :no-error clause: runs when body succeeds, receives body value
    // Also test multiple handlers with different error types
    let form = r#"(list
  ;; :no-error clause runs on success, receives body result as var
  (condition-case result
      (+ 10 20)
    (error (list 'error-caught result))
    (:success (list 'no-error result)))

  ;; :no-error clause NOT reached when error occurs
  (condition-case err
      (/ 1 0)
    (arith-error (list 'arith-caught (car err)))
    (:success (list 'success-should-not-run err)))

  ;; Multiple handlers, specific before generic, with :no-error
  (condition-case err
      (* 6 7)
    (arith-error (list 'arith))
    (wrong-type-argument (list 'wta))
    (void-variable (list 'void))
    (error (list 'generic))
    (:success (list 'success err)))

  ;; :no-error body can be multi-form
  (condition-case val
      (string-to-number "42")
    (error 'fail)
    (:success
     (let ((doubled (* val 2)))
       (list 'doubled doubled))))

  ;; Verify that :no-error clause receives the LAST form's value from body
  (condition-case res
      (progn 1 2 3 (+ 100 200))
    (error 'err)
    (:success (list 'got res))))"#;
    let expect = expect_test::expect![[
        r#""OK ((no-error 30) (arith-caught arith-error) (success 42) (doubled 84) (got 300))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// signal with custom error symbols and structured data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_signal_custom_symbols_and_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define multi-level error hierarchy, signal with rich data, verify extraction
    let form = r#"(progn
  ;; Define error hierarchy: app-error -> error
  ;;                         db-error -> app-error -> error
  ;;                         validation-error -> app-error -> error
  (put 'neovm--ehc-app-error 'error-conditions '(neovm--ehc-app-error error))
  (put 'neovm--ehc-app-error 'error-message "Application error")
  (put 'neovm--ehc-db-error 'error-conditions '(neovm--ehc-db-error neovm--ehc-app-error error))
  (put 'neovm--ehc-db-error 'error-message "Database error")
  (put 'neovm--ehc-val-error 'error-conditions '(neovm--ehc-val-error neovm--ehc-app-error error))
  (put 'neovm--ehc-val-error 'error-message "Validation error")

  (unwind-protect
      (list
       ;; Signal db-error, caught by app-error (parent)
       (condition-case err
           (signal 'neovm--ehc-db-error '("connection lost" :port 5432 :host "localhost"))
         (neovm--ehc-app-error
          (list 'app-caught (car err) (cadr err) (length (cddr err)))))

       ;; Signal db-error, caught by most specific handler
       (condition-case err
           (signal 'neovm--ehc-db-error '("timeout" :timeout 30))
         (neovm--ehc-db-error (list 'db-specific (cadr err)))
         (neovm--ehc-app-error (list 'app-generic))
         (error (list 'too-generic)))

       ;; Signal validation-error with structured data
       (condition-case err
           (signal 'neovm--ehc-val-error '("field 'email' invalid" :field email :value "bad"))
         (neovm--ehc-val-error
          (list 'val-caught (cadr err)
                'extra-count (length (cddr err)))))

       ;; Signal with nil data
       (condition-case err
           (signal 'neovm--ehc-app-error nil)
         (neovm--ehc-app-error
          (list 'nil-data (car err) (cdr err))))

       ;; Signal with deeply nested data
       (condition-case err
           (signal 'neovm--ehc-db-error '("nested" (a (b (c d)))))
         (error
          (list 'nested-data (car (caddr err))))))

    ;; Cleanup
    (put 'neovm--ehc-app-error 'error-conditions nil)
    (put 'neovm--ehc-app-error 'error-message nil)
    (put 'neovm--ehc-db-error 'error-conditions nil)
    (put 'neovm--ehc-db-error 'error-message nil)
    (put 'neovm--ehc-val-error 'error-conditions nil)
    (put 'neovm--ehc-val-error 'error-message nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((app-caught neovm--ehc-db-error \"connection lost\" 4) (db-specific \"timeout\") (val-caught \"field 'email' invalid\" extra-count 4) (nil-data neovm--ehc-app-error nil) (nested-data a))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// error vs user-error signal differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_error_vs_user_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `error` signals generic error condition
    // `user-error` signals user-error which is a child of error
    let form = r#"(list
  ;; (error ...) signals 'error
  (condition-case err
      (error "formatted: %d %s" 42 "hello")
    (user-error (list 'user-caught (cadr err)))
    (error (list 'error-caught (cadr err))))

  ;; (user-error ...) signals 'user-error, which IS-A error
  (condition-case err
      (user-error "user problem: %s" "bad input")
    (user-error (list 'user-caught (cadr err)))
    (error (list 'generic-caught (cadr err))))

  ;; user-error is caught by generic error handler
  (condition-case err
      (user-error "should be caught by error handler")
    (error (list 'generic-caught-user (car err))))

  ;; error is NOT caught by user-error handler
  (condition-case outer
      (condition-case err
          (error "plain error")
        (user-error (list 'user-handler-should-not-fire)))
    (error (list 'outer-caught (car outer))))

  ;; Verify error-message-string works for both
  (condition-case err
      (error "test message A")
    (error (error-message-string err)))
  (condition-case err
      (user-error "test message B")
    (error (error-message-string err))))"#;
    let expect = expect_test::expect![[
        r#""OK ((error-caught \"formatted: 42 hello\") (user-caught \"user problem: bad input\") (generic-caught-user user-error) (outer-caught error) \"test message A\" \"test message B\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested condition-case: inner catches vs outer catches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_nested_condition_case_selective() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner condition-case handles some errors, lets others propagate to outer
    let form = r#"(let ((trace nil))
  (list
   ;; Inner catches arith-error, outer catches wrong-type
   (condition-case outer-err
       (condition-case inner-err
           (/ 1 0)
         (arith-error
          (setq trace (cons 'inner-arith trace))
          'inner-handled))
     (error
      (setq trace (cons 'outer trace))
      'outer-handled))

   ;; Inner does NOT catch wrong-type, so it propagates to outer
   (condition-case outer-err
       (condition-case inner-err
           (+ "string" 1)
         (arith-error 'inner-arith))
     (wrong-type-argument
      (setq trace (cons 'outer-wta trace))
      (list 'outer-caught (cadr outer-err)))
     (error 'outer-generic))

   ;; Three levels deep: innermost handles, middle skips, outer catches
   (condition-case l3
       (condition-case l2
           (condition-case l1
               (signal 'error '("deep"))
             (arith-error 'l1-arith))
         (void-variable 'l2-void))
     (error
      (setq trace (cons 'l3-caught trace))
      (list 'l3 (cadr l3))))

   ;; Inner handles and re-signals, outer catches transformed error
   (condition-case outer
       (condition-case inner
           (/ 1 0)
         (arith-error
          (signal 'error (list (format "wrapped: %s" (car inner))))))
     (error
      (list 'rewrapped (cadr outer))))

   ;; Final trace
   (nreverse trace)))"#;
    let expect = expect_test::expect![[
        r#""OK (inner-handled (outer-caught number-or-marker-p) (l3 \"deep\") (rewrapped \"wrapped: arith-error\") (inner-arith outer-wta l3-caught))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// unwind-protect cleanup execution: normal exit, error exit, throw exit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_unwind_protect_all_exit_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify cleanup runs in ALL cases: normal, error, throw
    let form = r#"(let ((log nil))
  (list
   ;; Normal exit: cleanup runs, body value returned
   (let ((r (unwind-protect
                (progn
                  (setq log (cons 'normal-body log))
                  'normal-result)
              (setq log (cons 'normal-cleanup log)))))
     (list 'normal r))

   ;; Error exit: cleanup runs before error propagates
   (condition-case err
       (unwind-protect
           (progn
             (setq log (cons 'error-body log))
             (/ 1 0))
         (setq log (cons 'error-cleanup log)))
     (arith-error
      (setq log (cons 'error-handler log))
      'error-handled))

   ;; Throw exit: cleanup runs during stack unwinding
   (catch 'tag
     (unwind-protect
         (progn
           (setq log (cons 'throw-body log))
           (throw 'tag 'thrown-value))
       (setq log (cons 'throw-cleanup log))))

   ;; Nested unwind-protect: all cleanups run in correct order
   (condition-case nil
       (unwind-protect
           (unwind-protect
               (unwind-protect
                   (progn
                     (setq log (cons 'deep-body log))
                     (error "deep error"))
                 (setq log (cons 'cleanup-3 log)))
             (setq log (cons 'cleanup-2 log)))
         (setq log (cons 'cleanup-1 log)))
     (error 'deep-handled))

   ;; Cleanup runs even when error occurs IN the cleanup
   ;; (the cleanup error replaces the original error)
   (condition-case err
       (unwind-protect
           (progn
             (setq log (cons 'body-before-cleanup-err log))
             (error "original error"))
         (setq log (cons 'cleanup-that-errors log))
         (error "cleanup error"))
     (error (list 'caught (cadr err))))

   ;; Final execution order log
   (nreverse log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((normal normal-result) error-handled thrown-value deep-handled (caught \"cleanup error\") (normal-body normal-cleanup error-body error-cleanup error-handler throw-body throw-cleanup deep-body cleanup-3 cleanup-2 cleanup-1 body-before-cleanup-err cleanup-that-errors))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error propagation through function call chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_propagation_through_call_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Errors propagate up through multiple function calls until caught
    let form = r#"(progn
  (fset 'neovm--ehc-level3
    (lambda (x)
      (if (= x 0)
          (error "bottom-level error: %d" x)
        (* x 2))))
  (fset 'neovm--ehc-level2
    (lambda (x)
      (+ (funcall 'neovm--ehc-level3 x) 10)))
  (fset 'neovm--ehc-level1
    (lambda (x)
      (let ((r (funcall 'neovm--ehc-level2 x)))
        (list 'result r))))

  (unwind-protect
      (list
       ;; Normal call chain: no error
       (condition-case err
           (funcall 'neovm--ehc-level1 5)
         (error (list 'caught (cadr err))))

       ;; Error propagates from level3 through level2 and level1
       (condition-case err
           (funcall 'neovm--ehc-level1 0)
         (error (list 'propagated (cadr err))))

       ;; Catch at intermediate level
       (fset 'neovm--ehc-level2-safe
         (lambda (x)
           (condition-case err
               (+ (funcall 'neovm--ehc-level3 x) 10)
             (error (list 'caught-at-level2 (cadr err))))))
       (funcall 'neovm--ehc-level2-safe 0)

       ;; Error in mapcar propagates out
       (condition-case err
           (mapcar (lambda (x) (funcall 'neovm--ehc-level3 x)) '(1 2 0 4))
         (error (list 'mapcar-error (cadr err))))

       ;; Error in dolist propagates out
       (condition-case err
           (let ((acc nil))
             (dolist (x '(3 2 1 0 -1))
               (setq acc (cons (funcall 'neovm--ehc-level3 x) acc)))
             acc)
         (error (list 'dolist-error (cadr err)))))

    (fmakunbound 'neovm--ehc-level1)
    (fmakunbound 'neovm--ehc-level2)
    (fmakunbound 'neovm--ehc-level2-safe)
    (fmakunbound 'neovm--ehc-level3)))"#;
    let expect = expect_test::expect![[
        r#""OK ((result 20) (propagated \"bottom-level error: 0\") (closure (t) (x) (condition-case err (+ (funcall 'neovm--ehc-level3 x) 10) (error (list 'caught-at-level2 (cadr err))))) (caught-at-level2 \"bottom-level error: 0\") (mapcar-error \"bottom-level error: 0\") (dolist-error \"bottom-level error: 0\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ignore-errors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_ignore_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // ignore-errors returns nil on error, body value on success
    let form = r#"(list
  ;; Success: returns body value
  (ignore-errors (+ 1 2))

  ;; Error: returns nil
  (ignore-errors (/ 1 0))

  ;; Multiple forms: returns last value on success
  (ignore-errors 1 2 3 (+ 4 5))

  ;; Error in middle: returns nil, later forms not evaluated
  (let ((reached nil))
    (list (ignore-errors
            (/ 1 0)
            (setq reached t))
          reached))

  ;; Nested ignore-errors
  (ignore-errors
    (list (ignore-errors (/ 1 0))
          (ignore-errors (+ 1 2))
          (ignore-errors (car 42))
          (ignore-errors (* 3 4))))

  ;; ignore-errors with string operations
  (ignore-errors (substring "hello" 0 3))
  (ignore-errors (substring "hello" 10 20))

  ;; ignore-errors swallows user-error too
  (ignore-errors (user-error "test"))

  ;; ignore-errors with complex body
  (ignore-errors
    (let ((x 10) (y 20))
      (when (> x 5)
        (if (< y 30)
            (+ x y)
          0)))))"#;
    let expect =
        expect_test::expect![[r#""OK (3 nil 9 (nil nil) (nil 3 nil 12) \"hel\" nil nil 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-demoted-errors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_with_demoted_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // with-demoted-errors: on success returns body value,
    // on error logs a message and returns nil
    let form = r#"(list
  ;; Success case: returns body value
  (with-demoted-errors "Error: %S" (+ 1 2))

  ;; Error case: returns nil (error is demoted to message)
  (with-demoted-errors "Error: %S" (/ 1 0))

  ;; Success with complex body
  (with-demoted-errors "Oops: %S"
    (let ((x 10))
      (* x x)))

  ;; Error with format string
  (with-demoted-errors "Failed: %S" (error "kaboom"))

  ;; Nested: outer succeeds because inner demotes its error
  (with-demoted-errors "Outer: %S"
    (list (with-demoted-errors "Inner: %S" (/ 1 0))
          42))

  ;; with-demoted-errors returns nil on error, not the error itself
  (let ((result (with-demoted-errors "Err: %S" (error "test"))))
    (list 'result result (null result))))"#;
    let expect = expect_test::expect![[r#""OK (3 nil 100 nil (nil 42) (result nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: error handling state machine with recovery strategies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_recovery_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a multi-strategy error recovery system: try primary, fallback, default
    let form = r#"(progn
  (fset 'neovm--ehc-safe-divide
    (lambda (a b)
      (condition-case err
          (/ a b)
        (arith-error 'division-by-zero)
        (wrong-type-argument 'bad-type))))

  (fset 'neovm--ehc-try-strategies
    (lambda (input strategies)
      (let ((result nil)
            (tried nil)
            (remaining strategies))
        (catch 'found
          (while remaining
            (let ((strategy (car remaining)))
              (setq remaining (cdr remaining))
              (condition-case err
                  (let ((r (funcall strategy input)))
                    (setq tried (cons (list 'success (car err)) tried))
                    (throw 'found r))
                (error
                 (setq tried (cons (list 'failed (cadr err)) tried)))))))
        (list 'result result 'tried (nreverse tried)))))

  (unwind-protect
      (list
       ;; Simple safe divide tests
       (funcall 'neovm--ehc-safe-divide 10 3)
       (funcall 'neovm--ehc-safe-divide 10 0)
       (funcall 'neovm--ehc-safe-divide "x" 2)

       ;; Multi-operation with mixed errors and recoveries
       (let ((ops '((10 2) (10 0) (7 3) (1 0) (100 10)))
             (results nil))
         (dolist (op ops)
           (let ((r (condition-case err
                        (let ((val (/ (car op) (cadr op))))
                          (list 'ok val))
                      (arith-error (list 'err 'div0))
                      (error (list 'err 'other)))))
             (setq results (cons r results))))
         (nreverse results))

       ;; Retry with exponential backoff simulation
       (let ((attempt 0)
             (max-attempts 5)
             (success-at 3)
             (log nil))
         (catch 'done
           (while (< attempt max-attempts)
             (setq attempt (1+ attempt))
             (condition-case err
                 (progn
                   (setq log (cons (list 'try attempt) log))
                   (if (< attempt success-at)
                       (error "not yet: attempt %d" attempt)
                     (progn
                       (setq log (cons (list 'ok attempt) log))
                       (throw 'done (list 'succeeded attempt)))))
               (error
                (setq log (cons (list 'fail attempt (cadr err)) log))))))
         (list 'log (nreverse log))))

    (fmakunbound 'neovm--ehc-safe-divide)
    (fmakunbound 'neovm--ehc-try-strategies)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 division-by-zero bad-type ((ok 5) (err div0) (ok 2) (err div0) (ok 10)) (log ((try 1) (fail 1 \"not yet: attempt 1\") (try 2) (fail 2 \"not yet: attempt 2\") (try 3) (ok 3))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with t handler (catch-all) and complex handler bodies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_comprehensive_condition_case_t_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `t` as error condition catches everything including non-error signals
    let form = r#"(list
  ;; t handler catches any error
  (condition-case err
      (/ 1 0)
    (t (list 'caught-by-t (car err))))

  ;; t handler catches wrong-type-argument
  (condition-case err
      (car 42)
    (t (list 'caught-by-t (car err))))

  ;; t vs error: both catch standard errors, but t is documented to catch all
  (condition-case err
      (error "test")
    (t (list 'caught-all (car err))))

  ;; Multiple handlers with t last as catch-all
  (condition-case err
      (signal 'file-error '("no file"))
    (arith-error 'arith)
    (wrong-type-argument 'wta)
    (t (list 'catch-all (car err) (cadr err))))

  ;; Handler body with multiple forms
  (condition-case err
      (error "multi-form handler test")
    (error
     (let ((sym (car err))
           (msg (cadr err)))
       (list sym
             (upcase msg)
             (length msg))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught-by-t arith-error) (caught-by-t wrong-type-argument) (caught-all error) (catch-all file-error \"no file\") (error \"MULTI-FORM HANDLER TEST\" 23))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
