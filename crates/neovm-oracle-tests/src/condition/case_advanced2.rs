//! Advanced oracle parity tests for `condition-case`:
//! multiple error handlers with specificity ordering, re-signaling errors,
//! nested condition-case, error data extraction, user-defined error symbols
//! with `put`, `error-message-string`, and `condition-case-unless-debug`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Handler specificity: most-specific-first vs generic catch-all
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_handler_specificity_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When multiple handlers exist, the first matching one wins.
    // Specific error types should be listed before generic `error`.
    let form = r#"(list
  ;; arith-error is more specific than error; arith-error handler fires
  (condition-case err
      (/ 10 0)
    (arith-error (list 'specific-arith (car err)))
    (error (list 'generic (car err))))
  ;; wrong-type-argument is more specific than error
  (condition-case err
      (car 42)
    (wrong-type-argument (list 'specific-wta (car err) (cadr err)))
    (error (list 'generic (car err))))
  ;; If we put generic first, it catches everything
  (condition-case err
      (/ 10 0)
    (error (list 'generic-caught (car err)))
    (arith-error (list 'specific-never-reached)))
  ;; void-variable caught specifically
  (condition-case err
      (symbol-value 'neovm--cc-adv2-nonexistent-var-abc)
    (void-variable (list 'void-var (cadr err)))
    (error 'generic))
  ;; void-function caught specifically
  (condition-case err
      (funcall 'neovm--cc-adv2-nonexistent-fn-abc)
    (void-function (list 'void-fn (cadr err)))
    (error 'generic))
  ;; No handler matches: the most generic `error` catches file-error
  (condition-case err
      (signal 'file-error '("No such file" "/tmp/nonexistent"))
    (arith-error 'arith)
    (wrong-type-argument 'wta)
    (error (list 'generic-file (car err) (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((specific-arith arith-error) (specific-wta wrong-type-argument listp) (generic-caught arith-error) (void-var neovm--cc-adv2-nonexistent-var-abc) (void-fn neovm--cc-adv2-nonexistent-fn-abc) (generic-file file-error \"No such file\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Re-signaling errors: catch, transform, re-signal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_resignal_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of condition-case, each catching and re-signaling
    // with additional context data
    let form = r#"(let ((trace nil))
  (condition-case err3
      (condition-case err2
          (condition-case err1
              (progn
                (setq trace (cons 'original-body trace))
                (/ 1 0))
            (arith-error
             (setq trace (cons 'handler-1 trace))
             (signal 'error
                     (list "level-1-wrap" (car err1)))))
        (error
         (setq trace (cons 'handler-2 trace))
         (signal 'error
                 (list "level-2-wrap" (cadr err2)))))
    (error
     (setq trace (cons 'handler-3 trace))
     (list 'final-trace (nreverse trace)
           'error-data (cdr err3)))))"#;
    let expect = expect_test::expect![[
        r#""OK (final-trace (original-body handler-1 handler-2 handler-3) error-data (\"level-2-wrap\" \"level-1-wrap\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested condition-case: inner handles some, outer catches escaping errors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_nested_selective_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner condition-case only handles arith-error; other errors escape to outer
    let form = r#"(let ((run-with-handlers
         (lambda (body-fn)
           (condition-case outer-err
               (condition-case inner-err
                   (funcall body-fn)
                 (arith-error
                  (list 'inner-caught 'arith (car inner-err))))
             (wrong-type-argument
              (list 'outer-caught 'wta (car outer-err) (cadr outer-err)))
             (void-variable
              (list 'outer-caught 'void (cadr outer-err)))
             (error
              (list 'outer-caught 'generic (car outer-err)))))))
  (list
   ;; arith-error caught by inner
   (funcall run-with-handlers (lambda () (/ 1 0)))
   ;; wrong-type-argument escapes inner, caught by outer
   (funcall run-with-handlers (lambda () (car 99)))
   ;; void-variable escapes inner, caught by outer
   (funcall run-with-handlers (lambda () (symbol-value 'neovm--cc-adv2-undef-xyz)))
   ;; file-error escapes inner, caught by outer generic
   (funcall run-with-handlers (lambda () (signal 'file-error '("nope"))))
   ;; No error: body value returned
   (funcall run-with-handlers (lambda () (+ 10 20)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((inner-caught arith arith-error) (outer-caught wta wrong-type-argument listp) (outer-caught void neovm--cc-adv2-undef-xyz) (outer-caught generic file-error) 30)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error data extraction: cdr of error, error-message-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_error_data_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract and inspect all parts of error data from various error types
    let form = r#"(list
  ;; arith-error data: (arith-error)
  (condition-case err
      (/ 1 0)
    (arith-error
     (list 'symbol (car err)
           'data (cdr err)
           'msg (error-message-string err))))
  ;; wrong-type-argument data: (wrong-type-argument PREDICATE VALUE)
  (condition-case err
      (car "not-a-list")
    (wrong-type-argument
     (list 'symbol (car err)
           'predicate (cadr err)
           'value (caddr err)
           'msg (error-message-string err))))
  ;; Custom signal with structured data
  (condition-case err
      (signal 'error '("custom message" extra-data 42))
    (error
     (list 'symbol (car err)
           'message (cadr err)
           'extra (cddr err)
           'msg (error-message-string err))))
  ;; void-variable data
  (condition-case err
      (symbol-value 'neovm--cc-adv2-unbound-for-msg)
    (void-variable
     (list 'symbol (car err)
           'varname (cadr err)
           'msg (error-message-string err))))
  ;; error-message-string on manually constructed error data
  (error-message-string '(error "Something went wrong"))
  (error-message-string '(arith-error)))"#;
    let expect = expect_test::expect![[
        r#""OK ((symbol arith-error data nil msg \"Arithmetic error\") (symbol wrong-type-argument predicate listp value \"not-a-list\" msg \"Wrong type argument: listp, \\\"not-a-list\\\"\") (symbol error message \"custom message\" extra (extra-data 42) msg \"custom message: extra-data, 42\") (symbol void-variable varname neovm--cc-adv2-unbound-for-msg msg \"Symbol’s value as variable is void: neovm--cc-adv2-unbound-for-msg\") \"Something went wrong\" \"Arithmetic error\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// User-defined error symbols with `put` error-conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_user_defined_error_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define custom error hierarchy using `put` and `error-conditions`
    // my-error -> error
    // my-network-error -> my-error -> error
    // my-timeout-error -> my-network-error -> my-error -> error
    let form = r#"(progn
  (put 'neovm--cc-adv2-my-error 'error-conditions
       '(neovm--cc-adv2-my-error error))
  (put 'neovm--cc-adv2-my-error 'error-message "My custom error")
  (put 'neovm--cc-adv2-my-net-error 'error-conditions
       '(neovm--cc-adv2-my-net-error neovm--cc-adv2-my-error error))
  (put 'neovm--cc-adv2-my-net-error 'error-message "Network error")
  (put 'neovm--cc-adv2-my-timeout 'error-conditions
       '(neovm--cc-adv2-my-timeout neovm--cc-adv2-my-net-error neovm--cc-adv2-my-error error))
  (put 'neovm--cc-adv2-my-timeout 'error-message "Timeout error")
  (unwind-protect
      (list
       ;; Custom error caught by its own handler
       (condition-case err
           (signal 'neovm--cc-adv2-my-error '("detail-1"))
         (neovm--cc-adv2-my-error (list 'caught-my (cadr err))))
       ;; Network error caught by parent my-error handler
       (condition-case err
           (signal 'neovm--cc-adv2-my-net-error '("conn refused"))
         (neovm--cc-adv2-my-error (list 'caught-parent (car err) (cadr err))))
       ;; Timeout caught by grandparent my-error handler
       (condition-case err
           (signal 'neovm--cc-adv2-my-timeout '("5 seconds"))
         (neovm--cc-adv2-my-error (list 'caught-grandparent (car err) (cadr err))))
       ;; Most-specific handler wins over parent
       (condition-case err
           (signal 'neovm--cc-adv2-my-timeout '("10 seconds"))
         (neovm--cc-adv2-my-timeout (list 'caught-specific (cadr err)))
         (neovm--cc-adv2-my-net-error (list 'caught-net))
         (neovm--cc-adv2-my-error (list 'caught-base)))
       ;; Generic error catches custom errors too
       (condition-case err
           (signal 'neovm--cc-adv2-my-net-error '("host unreachable"))
         (error (list 'caught-generic (car err) (cadr err))))
       ;; error-message-string with custom error
       (condition-case err
           (signal 'neovm--cc-adv2-my-timeout '("30 seconds"))
         (error (error-message-string err))))
    (put 'neovm--cc-adv2-my-error 'error-conditions nil)
    (put 'neovm--cc-adv2-my-error 'error-message nil)
    (put 'neovm--cc-adv2-my-net-error 'error-conditions nil)
    (put 'neovm--cc-adv2-my-net-error 'error-message nil)
    (put 'neovm--cc-adv2-my-timeout 'error-conditions nil)
    (put 'neovm--cc-adv2-my-timeout 'error-message nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught-my \"detail-1\") (caught-parent neovm--cc-adv2-my-net-error \"conn refused\") (caught-grandparent neovm--cc-adv2-my-timeout \"5 seconds\") (caught-specific \"10 seconds\") (caught-generic neovm--cc-adv2-my-net-error \"host unreachable\") \"Timeout error: \\\"30 seconds\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with complex handler bodies and side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_handler_body_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Handler bodies can contain multiple forms with side effects,
    // loops, and nested condition-case
    let form = r#"(let ((log nil)
      (retry-count 0))
  ;; Simulate retry logic: attempt operation, on failure retry up to 3 times
  (condition-case final-err
      (let ((attempts '(fail fail fail succeed))
            (attempt-idx 0)
            (result nil))
        (catch 'done
          (while (< attempt-idx (length attempts))
            (condition-case err
                (let ((action (nth attempt-idx attempts)))
                  (setq log (cons (list 'attempt attempt-idx action) log))
                  (if (eq action 'fail)
                      (signal 'error (list "operation failed" attempt-idx))
                    (progn
                      (setq result (list 'success attempt-idx))
                      (throw 'done result))))
              (error
               (setq retry-count (1+ retry-count))
               (setq log (cons (list 'retry retry-count (cadr err)) log))
               (setq attempt-idx (1+ attempt-idx))))
            ))
        result)
    (error
     (list 'all-failed (cdr final-err))))
  (list 'retries retry-count
        'log (nreverse log)))"#;
    let expect = expect_test::expect![[
        r#""OK (retries 3 log ((attempt 0 fail) (retry 1 \"operation failed\") (attempt 1 fail) (retry 2 \"operation failed\") (attempt 2 fail) (retry 3 \"operation failed\") (attempt 3 succeed)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case interacting with catch/throw and unwind-protect together
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_three_way_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test the interaction between condition-case, catch/throw, and unwind-protect
    // all nested together in complex ways
    let form = r#"(let ((log nil))
  (list
   ;; 1. throw from inside condition-case handler, through unwind-protect
   (catch 'escape
     (unwind-protect
         (condition-case err
             (/ 1 0)
           (arith-error
            (setq log (cons 'handler log))
            (throw 'escape 'escaped-from-handler)))
       (setq log (cons 'cleanup-1 log))))

   ;; 2. error in unwind-protect cleanup, caught by outer condition-case
   (condition-case err
       (unwind-protect
           (progn
             (setq log (cons 'body-2 log))
             42)
         (setq log (cons 'cleanup-2-before-error log))
         (signal 'error '("cleanup-error")))
     (error
      (setq log (cons 'outer-handler-2 log))
      (list 'caught-cleanup-error (cadr err))))

   ;; 3. throw through both condition-case and unwind-protect
   (catch 'outer-tag
     (condition-case err
         (unwind-protect
             (catch 'inner-tag
               (unwind-protect
                   (throw 'outer-tag 'deep-throw)
                 (setq log (cons 'inner-cleanup log))))
           (setq log (cons 'outer-cleanup log)))
       (error 'should-not-reach)))

   ;; 4. error inside catch body, unwind-protect cleanup runs
   (condition-case err
       (catch 'tag
         (unwind-protect
             (progn
               (setq log (cons 'in-catch-body log))
               (/ 1 0))
           (setq log (cons 'catch-cleanup log))))
     (arith-error
      (setq log (cons 'arith-handler log))
      'arith-caught))

   ;; Final log showing execution order
   (nreverse log)))"#;
    let expect = expect_test::expect![[
        r#""OK (escaped-from-handler (caught-cleanup-error \"cleanup-error\") deep-throw arith-caught (handler cleanup-1 body-2 cleanup-2-before-error outer-handler-2 inner-cleanup outer-cleanup in-catch-body catch-cleanup arith-handler))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with complex error hierarchy and dispatch table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cc_adv2_dispatch_table_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dispatch table that maps operation names to functions,
    // with comprehensive error handling for each operation
    let form = r#"(let ((results nil)
      (ops '(divide type-check lookup signal-custom normal)))
  (dolist (op ops)
    (let ((result
           (condition-case err
               (cond
                ((eq op 'divide) (/ 100 0))
                ((eq op 'type-check) (+ "not-a-number" 1))
                ((eq op 'lookup) (symbol-value 'neovm--cc-adv2-no-such-var-zzz))
                ((eq op 'signal-custom)
                 (signal 'error '("custom" 1 2 3)))
                ((eq op 'normal) (* 6 7))
                (t (signal 'error (list "unknown op" op))))
             (arith-error
              (list op 'arith-error
                    (length (cdr err))))
             (wrong-type-argument
              (list op 'wrong-type
                    (cadr err)))
             (void-variable
              (list op 'void-var
                    (cadr err)))
             (error
              (list op 'generic-error
                    (cadr err)
                    (length (cddr err)))))))
      (setq results (cons result results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((divide arith-error 0) (type-check wrong-type number-or-marker-p) (lookup void-var neovm--cc-adv2-no-such-var-zzz) (signal-custom generic-error \"custom\" 3) 42)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
