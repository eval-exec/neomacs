//! Advanced oracle parity tests for `condition-case` with ALL error types
//! and complex patterns: error symbol dispatch, multiple handlers, default
//! handler (t), :success handler, nested condition-case, retry logic,
//! error classification, user-defined error types, and error chain unwinding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Exhaustive error symbol testing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_exhaustive_error_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test catching all common built-in error symbols individually
    let form = r#"(list
  ;; arith-error: division by zero
  (condition-case e (/ 1 0) (arith-error (list 'arith (car e))))
  ;; wrong-type-argument: non-list to car
  (condition-case e (car 99) (wrong-type-argument (list 'wta (car e))))
  ;; void-variable: unbound symbol
  (condition-case e (symbol-value 'neovm--ccpa-unbound-var-xyz)
    (void-variable (list 'void-var (car e))))
  ;; void-function: undefined function
  (condition-case e (funcall 'neovm--ccpa-undef-fn-xyz)
    (void-function (list 'void-fn (car e))))
  ;; wrong-number-of-arguments
  (condition-case e (funcall (lambda (x) x) 1 2 3)
    (wrong-number-of-arguments (list 'wna (car e))))
  ;; setting-constant: attempt to set nil
  (condition-case e (set 'nil 42)
    (setting-constant (list 'setconst (car e))))
  ;; args-out-of-range: substring beyond length
  (condition-case e (substring "abc" 0 99)
    (args-out-of-range (list 'aoor (car e))))
  ;; error: generic signal
  (condition-case e (signal 'error '("custom message"))
    (error (list 'generic (car e) (cadr e)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((arith arith-error) (wta wrong-type-argument) (void-var void-variable) (void-fn void-function) (wna wrong-number-of-arguments) (setconst setting-constant) (aoor args-out-of-range) (generic error \"custom message\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple handlers with priority: first matching wins
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_handler_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When multiple handlers could match, the FIRST matching one wins.
    // arith-error is a child of error, so listing arith-error first means
    // it catches division-by-zero; the generic (error ...) handler never fires.
    let form = r#"(list
  ;; arith-error handler first: it catches the error
  (condition-case e (/ 1 0)
    (arith-error 'arith-first)
    (error 'generic-never))
  ;; error handler first: it catches arith-error too (parent)
  (condition-case e (/ 1 0)
    (error 'generic-catches-all)
    (arith-error 'arith-never))
  ;; Two specific handlers, only one matches
  (condition-case e (car 42)
    (arith-error 'arith-no)
    (wrong-type-argument 'wta-yes)
    (error 'generic-fallback))
  ;; No matching specific handler, falls through to error
  (condition-case e (signal 'file-error '("oops"))
    (arith-error 'arith-no)
    (wrong-type-argument 'wta-no)
    (error 'generic-caught)))"#;
    let expect =
        expect_test::expect![[r#""OK (arith-first generic-catches-all wta-yes generic-caught)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Default handler with t (catch any condition)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_default_handler_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A handler with symbol t catches ANY condition, including non-error signals
    let form = r#"(list
  ;; t catches arith-error
  (condition-case e (/ 1 0)
    (t (list 'caught-by-t (car e))))
  ;; t catches wrong-type-argument
  (condition-case e (car "not-a-list")
    (t (list 'caught-wta (car e))))
  ;; t catches user-defined error
  (condition-case e (signal 'my-custom-error '("data"))
    (t (list 'caught-custom (car e) (cadr e))))
  ;; When specific handler matches first, t is not reached
  (condition-case e (/ 1 0)
    (arith-error 'specific-wins)
    (t 'default-not-reached))
  ;; When no specific matches, t catches
  (condition-case e (signal 'file-error '("test"))
    (arith-error 'arith-no)
    (t (list 't-caught (car e)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught-by-t arith-error) (caught-wta wrong-type-argument) (caught-custom error \"Invalid error symbol\") specific-wins (t-caught file-error))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested condition-case with different error types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_nested_different_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner and outer condition-case handle different error types
    let form = r#"(let ((log nil))
  ;; Outer catches arith-error, inner catches wrong-type-argument
  (let ((r1
         (condition-case outer-err
             (progn
               (setq log (cons 'outer-body log))
               (condition-case inner-err
                   (progn
                     (setq log (cons 'inner-body log))
                     ;; This raises wrong-type-argument, caught by inner
                     (car 42))
                 (wrong-type-argument
                  (setq log (cons 'inner-handler log))
                  ;; Now raise arith-error, which inner does NOT catch
                  (/ 1 0))))
           (arith-error
            (setq log (cons 'outer-handler log))
            'outer-caught-arith))))
    ;; Another case: inner error NOT caught by inner, caught by outer
    (setq log nil)
    (let ((r2
           (condition-case outer-err
               (condition-case inner-err
                   (signal 'file-error '("no such file"))
                 (arith-error 'inner-no-match))
             (file-error
              (list 'outer-caught-file (cadr outer-err))))))
      (list r1 r2))))"#;
    let expect =
        expect_test::expect![[r#""OK (outer-caught-arith (outer-caught-file \"no such file\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: retry logic using condition-case in a loop
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_retry_logic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a retry mechanism: try an operation up to N times,
    // catching errors each time, before giving up.
    let form = r#"(let ((attempt 0)
      (max-retries 5)
      (success-on 3)
      (results nil))
  ;; Keep trying until success or max-retries exhausted
  (let ((done nil)
        (final-result nil))
    (while (and (not done) (< attempt max-retries))
      (setq attempt (1+ attempt))
      (let ((outcome
             (condition-case err
                 (progn
                   ;; "Fail" on attempts before success-on
                   (if (< attempt success-on)
                       (signal 'error (list (format "attempt %d failed" attempt)))
                     ;; Succeed on attempt = success-on
                     (list 'success attempt)))
               (error
                (list 'retry attempt (cadr err))))))
        (setq results (cons outcome results))
        (when (eq (car outcome) 'success)
          (setq done t)
          (setq final-result outcome))))
    (list 'attempts attempt
          'final final-result
          'log (nreverse results))))"#;
    let expect = expect_test::expect![[
        r#""OK (attempts 3 final (success 3) log ((retry 1 \"attempt 1 failed\") (retry 2 \"attempt 2 failed\") (success 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: error classification and dispatch table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_error_classification_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dispatch table that classifies errors by category
    // and applies different recovery strategies
    let form = r#"(let ((results nil))
  (let ((safe-eval
         (lambda (expr)
           (condition-case err
               (list 'ok (eval expr))
             (arith-error
              (list 'math-error 'recovery-zero 0))
             (wrong-type-argument
              (list 'type-error 'recovery-nil nil))
             (void-variable
              (list 'unbound-error 'recovery-default 'unset))
             (void-function
              (list 'fn-error 'recovery-identity expr))
             (args-out-of-range
              (list 'range-error 'recovery-empty ""))
             (error
              (list 'unknown-error (car err) (cdr err)))))))
    ;; Test various error-inducing expressions
    (setq results
          (list
           (funcall safe-eval '(+ 1 2))            ;; no error
           (funcall safe-eval '(/ 10 0))            ;; arith-error
           (funcall safe-eval '(car 42))            ;; wrong-type-argument
           (funcall safe-eval '(+ neovm--ccpa-novar 1))  ;; void-variable
           (funcall safe-eval '(neovm--ccpa-nofn 1))     ;; void-function
           (funcall safe-eval '(substring "ab" 5 10))    ;; args-out-of-range
           (funcall safe-eval '(signal 'my-err '("x"))))))  ;; generic error
  results)"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 3) (math-error recovery-zero 0) (type-error recovery-nil nil) (unbound-error recovery-default unset) (fn-error recovery-identity (neovm--ccpa-nofn 1)) (range-error recovery-empty \"\") (unknown-error error (\"Invalid error symbol\" my-err)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: error chain with wrapping and unwinding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_error_chain_unwinding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a call stack where each level wraps errors with context
    let form = r#"(progn
  (fset 'neovm--ccpa-level3
    (lambda (x)
      (if (= x 0)
          (signal 'arith-error '("division by zero at level 3"))
        (* x 2))))

  (fset 'neovm--ccpa-level2
    (lambda (x)
      (condition-case err
          (funcall 'neovm--ccpa-level3 x)
        (arith-error
         (signal 'error (list (format "level2 wrapped: %s" (cadr err))))))))

  (fset 'neovm--ccpa-level1
    (lambda (x)
      (condition-case err
          (funcall 'neovm--ccpa-level2 x)
        (error
         (list 'level1-caught (cadr err))))))

  (unwind-protect
      (list
       ;; Normal execution: no error
       (funcall 'neovm--ccpa-level1 5)
       ;; Error path: level3 -> level2 wraps -> level1 catches
       (funcall 'neovm--ccpa-level1 0)
       ;; Direct level3 error caught at level1 (bypassing level2 wrap)
       (condition-case err
           (funcall 'neovm--ccpa-level3 0)
         (arith-error (list 'direct-catch (cadr err)))))
    (fmakunbound 'neovm--ccpa-level3)
    (fmakunbound 'neovm--ccpa-level2)
    (fmakunbound 'neovm--ccpa-level1)))"#;
    let expect = expect_test::expect![[
        r#""OK (10 (level1-caught \"level2 wrapped: division by zero at level 3\") (direct-catch \"division by zero at level 3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with no-error body (no handler triggered)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_no_error_passthrough() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When the body completes without error, condition-case returns
    // the body's value and no handler runs
    let form = r#"(list
  ;; Simple value passthrough
  (condition-case e 42 (error 'never))
  ;; Complex expression passthrough
  (condition-case e
      (let ((x 10) (y 20))
        (+ (* x y) 5))
    (error 'never))
  ;; Side effects happen, no error
  (let ((log nil))
    (let ((result (condition-case e
                      (progn
                        (setq log (cons 'a log))
                        (setq log (cons 'b log))
                        (setq log (cons 'c log))
                        'done)
                    (error 'never))))
      (list result (nreverse log))))
  ;; nil body
  (condition-case e nil (error 'never))
  ;; Multiple handlers, none triggered
  (condition-case e (+ 1 2)
    (arith-error 'no)
    (wrong-type-argument 'no)
    (void-variable 'no)
    (error 'no)))"#;
    let expect = expect_test::expect![[r#""OK (42 205 (done (a b c)) nil 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with error data extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_error_data_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract detailed error data from various error types
    let form = r#"(list
  ;; arith-error data
  (condition-case e (/ 1 0)
    (arith-error (list (car e) (length e))))
  ;; wrong-type-argument data includes the expected type and actual value
  (condition-case e (car 42)
    (wrong-type-argument
     (list (car e) (cadr e) (caddr e) (length e))))
  ;; void-variable data includes the variable name
  (condition-case e (symbol-value 'neovm--ccpa-novar-extract)
    (void-variable
     (list (car e) (cadr e) (length e))))
  ;; Custom error with multiple data items
  (condition-case e
      (signal 'error '("msg" extra1 extra2 42))
    (error
     (list (car e)
           (length e)
           (cadr e)
           (caddr e)
           (cadddr e)
           (nth 4 e))))
  ;; args-out-of-range data
  (condition-case e (aref [1 2 3] 99)
    (args-out-of-range
     (list (car e) (length e)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((arith-error 1) (wrong-type-argument listp 42 3) (void-variable neovm--ccpa-novar-extract 2) (error 5 \"msg\" extra1 extra2 42) (args-out-of-range 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with unwind-protect interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_adv_cleanup_ordering_with_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that unwind-protect cleanup runs BEFORE the condition-case handler,
    // and that the handler sees the post-cleanup state
    let form = r#"(let ((execution-order nil))
  ;; Pattern: condition-case wrapping unwind-protect
  (condition-case err
      (unwind-protect
          (progn
            (setq execution-order (cons 'body execution-order))
            (/ 1 0))
        (setq execution-order (cons 'cleanup execution-order)))
    (arith-error
     (setq execution-order (cons 'handler execution-order))
     ;; Return the full execution order
     (nreverse execution-order))))"#;
    let expect = expect_test::expect![[r#""OK (body cleanup handler)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(body cleanup handler)", &o, &n);
}
