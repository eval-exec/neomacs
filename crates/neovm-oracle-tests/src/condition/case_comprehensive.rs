//! Comprehensive oracle parity tests for condition-case:
//! multiple handlers, handler ordering, error catch-all, re-signaling,
//! nested condition-case, interaction with unwind-protect, error data
//! extraction, custom error types, signal with complex data.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multiple handlers with specific and general error types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_multiple_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple handlers: most specific first, then general
    let form = r#"(list
  ;; arith-error handler catches division by zero
  (condition-case err
      (/ 1 0)
    (void-function 'wrong-handler)
    (arith-error (list 'caught-arith (cdr err)))
    (error 'too-general))
  ;; void-variable handler
  (condition-case err
      (symbol-value 'neovm--cctest-nonexistent-var-12345)
    (void-variable (list 'caught-void-var (car (cdr err))))
    (error 'too-general))
  ;; wrong-type-argument handler
  (condition-case err
      (car 42)
    (wrong-type-argument (list 'caught-wrong-type (car (cdr err))))
    (error 'too-general))
  ;; wrong-number-of-args handler
  (condition-case err
      (funcall (lambda (a b) (+ a b)) 1 2 3)
    (wrong-number-of-args 'caught-wrong-num-args)
    (error 'too-general)))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught-arith nil) (caught-void-var neovm--cctest-nonexistent-var-12345) (caught-wrong-type listp) too-general)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Handler ordering: first matching handler wins
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_handler_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When multiple handlers could match, the first one wins
    let form = r#"(list
  ;; error is parent of arith-error; if error handler comes first, it wins
  (condition-case err
      (/ 1 0)
    (error 'caught-by-error)
    (arith-error 'caught-by-arith))
  ;; arith-error first: it wins for division by zero
  (condition-case err
      (/ 1 0)
    (arith-error 'caught-by-arith)
    (error 'caught-by-error))
  ;; void-variable does NOT match arith-error
  (condition-case err
      (/ 1 0)
    (void-variable 'wrong)
    (arith-error 'correct)
    (error 'too-general)))"#;
    let expect = expect_test::expect![[r#""OK (caught-by-error caught-by-arith correct)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error catch-all handler
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_error_catchall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The `error` condition type is a catch-all for all error signals
    let form = r#"(list
  ;; Catches arith-error via generic error
  (condition-case err (/ 1 0) (error (list 'caught (car err))))
  ;; Catches void-variable via generic error
  (condition-case err
      (symbol-value 'neovm--cctest-nonexistent-98765)
    (error (list 'caught (car err))))
  ;; Catches wrong-type-argument via generic error
  (condition-case err (car 42) (error (list 'caught (car err))))
  ;; Catches user signal via generic error
  (condition-case err
      (signal 'error '("custom message"))
    (error (list 'caught (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught arith-error) (caught void-variable) (caught wrong-type-argument) (caught \"custom message\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Re-signaling from handler
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_resignal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Catch an error, then re-signal it (or a different error) from the handler
    let form = r#"(condition-case outer-err
  (condition-case inner-err
      (/ 1 0)
    (arith-error
     ;; Re-signal as a generic error with additional context
     (signal 'error (list "wrapped" (car inner-err) (cdr inner-err)))))
  (error (list 'outer-caught (cdr outer-err))))"#;
    let expect = expect_test::expect![[r#""OK (outer-caught (\"wrapped\" arith-error nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_comprehensive_resignal_different_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Catch one error type and re-signal as a completely different type
    let form = r#"(condition-case outer-err
  (condition-case inner-err
      (car 42)
    (wrong-type-argument
     (signal 'arith-error nil)))
  (arith-error 'retyped-to-arith)
  (error 'generic-catch))"#;
    let expect = expect_test::expect![[r#""OK retyped-to-arith""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested condition-case
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_nested_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of nesting with different error types at each level
    let form = r#"(condition-case e1
  (progn
    (condition-case e2
        (progn
          (condition-case e3
              (/ 1 0)
            (void-variable 'wrong-inner))  ;; Does not match arith-error
          'inner-no-catch)  ;; Not reached because arith-error propagates
      (arith-error (list 'mid-caught (car e2))))
    ;; After mid-level catch, no error propagates
    )
  (error 'outer-catch))"#;
    let expect = expect_test::expect![[r#""OK (mid-caught arith-error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_comprehensive_nested_independent_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple nested condition-case each handling independent errors
    let form = r#"(list
  (condition-case e1
      (list
       'first
       (condition-case e2
           (/ 1 0)
         (arith-error 'inner-arith-handled))
       'after-inner)
    (error 'outer-never-reached))
  ;; Second: inner error propagates to outer
  (condition-case e1
      (condition-case e2
          (signal 'error '("test"))
        (void-variable 'inner-no-match))
    (error (list 'outer-got-it (cadr e1)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((first inner-arith-handled after-inner) (outer-got-it \"test\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with unwind-protect
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_with_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify unwind-protect cleanup runs even when condition-case catches
    let form = r#"(let ((cleanup-ran nil)
                        (result nil))
  (setq result
        (condition-case err
            (unwind-protect
                (/ 1 0)
              (setq cleanup-ran t))
          (arith-error (list 'caught cleanup-ran))))
  (list result cleanup-ran))"#;
    let expect = expect_test::expect![[r#""OK ((caught t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_comprehensive_unwind_protect_inside_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unwind-protect inside a condition-case handler body
    let form = r#"(let ((trace nil))
  (condition-case err
      (/ 1 0)
    (arith-error
     (unwind-protect
         (progn
           (setq trace (cons 'handler-body trace))
           (+ 1 2))
       (setq trace (cons 'handler-cleanup trace)))))
  (list (nreverse trace)))"#;
    let expect = expect_test::expect![[r#""OK ((handler-body handler-cleanup))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error data extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_error_data_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract all parts of error data from various error types
    let form = r#"(list
  ;; arith-error data
  (condition-case err (/ 1 0)
    (arith-error (list 'sym (car err) 'data (cdr err))))
  ;; Signal with multi-element data list
  (condition-case err
      (signal 'error '("msg" extra1 extra2 42))
    (error (list 'len (length (cdr err))
                 'first (nth 0 (cdr err))
                 'second (nth 1 (cdr err))
                 'third (nth 2 (cdr err))
                 'fourth (nth 3 (cdr err)))))
  ;; wrong-type-argument data includes expected type and actual value
  (condition-case err (+ "not-a-number" 1)
    (wrong-type-argument
     (list 'type (car err)
           'expected (nth 1 err)
           'got-type (type-of (nth 2 err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((sym arith-error data nil) (len 4 first \"msg\" second extra1 third extra2 fourth 42) (type wrong-type-argument expected number-or-marker-p got-type string))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Custom error types via define-error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_custom_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define custom error hierarchy and test handler matching
    let form = r#"(progn
  (define-error 'neovm--cctest-base-error "Base error")
  (define-error 'neovm--cctest-child-error "Child error" 'neovm--cctest-base-error)
  (define-error 'neovm--cctest-grandchild-error "Grandchild" 'neovm--cctest-child-error)

  (unwind-protect
      (list
       ;; Direct match: child error caught by child handler
       (condition-case err
           (signal 'neovm--cctest-child-error '("child data"))
         (neovm--cctest-child-error (list 'child-caught (cadr err)))
         (neovm--cctest-base-error 'base-too-general))
       ;; Parent match: child error caught by base handler when no child handler
       (condition-case err
           (signal 'neovm--cctest-child-error '("child data"))
         (neovm--cctest-base-error (list 'base-caught (cadr err))))
       ;; Grandchild caught by base
       (condition-case err
           (signal 'neovm--cctest-grandchild-error '("gc data"))
         (neovm--cctest-base-error (list 'base-caught-gc (cadr err))))
       ;; Grandchild NOT caught by unrelated handler, falls through to error
       (condition-case err
           (signal 'neovm--cctest-grandchild-error '("gc data"))
         (void-variable 'wrong)
         (error (list 'generic-caught (car err))))
       ;; Base error NOT caught by child handler
       (condition-case err
           (signal 'neovm--cctest-base-error '("base data"))
         (neovm--cctest-child-error 'wrong-child)
         (neovm--cctest-base-error (list 'correct-base (cadr err)))))
    ;; Cleanup custom error symbols
    (put 'neovm--cctest-base-error 'error-conditions nil)
    (put 'neovm--cctest-base-error 'error-message nil)
    (put 'neovm--cctest-child-error 'error-conditions nil)
    (put 'neovm--cctest-child-error 'error-message nil)
    (put 'neovm--cctest-grandchild-error 'error-conditions nil)
    (put 'neovm--cctest-grandchild-error 'error-message nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((child-caught \"child data\") (base-caught \"child data\") (base-caught-gc \"gc data\") (generic-caught neovm--cctest-grandchild-error) (correct-base \"base data\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Signal with complex data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_signal_complex_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Signal errors with various data structures as error data
    let form = r#"(list
  ;; Signal with nested list data
  (condition-case err
      (signal 'error (list (list 'a 1) (list 'b 2) (list 'c 3)))
    (error (mapcar #'car (cdr err))))
  ;; Signal with cons/dotted pair data
  (condition-case err
      (signal 'error '((key . value) (key2 . value2)))
    (error (mapcar #'cdr (cdr err))))
  ;; Signal with nil data
  (condition-case err
      (signal 'arith-error nil)
    (arith-error (list 'sym (car err) 'data-nil (null (cdr err)))))
  ;; Signal with single string
  (condition-case err
      (signal 'error '("just a message"))
    (error (cadr err)))
  ;; Signal with vector in data
  (condition-case err
      (signal 'error (list [1 2 3] "after-vector"))
    (error (list (aref (cadr err) 1) (caddr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c) (value value2) (sym arith-error data-nil t) \"just a message\" (2 \"after-vector\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with no error (passthrough) and multi-form body
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_no_error_passthrough() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When no error occurs, body result is returned, handlers are ignored
    let form = r#"(list
  ;; Simple passthrough
  (condition-case err (+ 1 2 3) (error 'never))
  ;; Multi-form body (progn-like): only last value returned
  (condition-case err
      (progn
        (+ 1 1)
        (+ 2 2)
        (+ 3 3))
    (error 'never))
  ;; Side effects in body should persist
  (let ((x 0))
    (condition-case err
        (progn (setq x 10) (+ x 5))
      (error 'never))
    (list x))
  ;; Nil body
  (condition-case err nil (error 'never))
  ;; Body returns a complex value
  (condition-case err
      (list 1 (cons 'a 'b) [3 4 5])
    (error 'never)))"#;
    let expect = expect_test::expect![[r#""OK (6 6 (10) nil (1 (a . b) [3 4 5]))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Handler body with multiple forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_handler_multi_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Handler body can have multiple forms (implicit progn)
    let form = r#"(let ((side-effect nil))
  (let ((result
         (condition-case err
             (/ 1 0)
           (arith-error
            (setq side-effect 'handler-ran)
            (let ((err-type (car err)))
              (list 'handled err-type))))))
    (list result side-effect)))"#;
    let expect = expect_test::expect![[r#""OK ((handled arith-error) handler-ran)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case with variable binding (err) vs nil (no binding)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_condition_case_comprehensive_var_binding_vs_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare behavior with bound vs unbound error variable
    let form = r#"(list
  ;; With variable binding: err is bound to error data
  (condition-case err
      (/ 1 0)
    (arith-error (list 'with-var (car err))))
  ;; With nil: no variable binding, handler body still runs
  (condition-case nil
      (/ 1 0)
    (arith-error 'no-var-but-caught))
  ;; Verify err is NOT bound outside condition-case scope
  ;; (using a fresh name to avoid interference)
  (let ((neovm--cctest-outer 'original))
    (condition-case neovm--cctest-outer
        (/ 1 0)
      (arith-error neovm--cctest-outer))
    ;; After condition-case, the let binding should be restored
    ))"#;
    let expect =
        expect_test::expect![[r#""OK ((with-var arith-error) no-var-but-caught (arith-error))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
