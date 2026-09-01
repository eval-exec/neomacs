//! Advanced oracle parity tests for signal/error handling:
//! custom error symbols via `put`, complex data payloads, error
//! hierarchy (parent-child), condition-case catching parent types,
//! `error` function, `user-error` vs `error` differences, error
//! classification/recovery, and error wrapping/chaining.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// signal with custom error symbols (define via put)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_custom_error_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define a custom error symbol by putting 'error-conditions and
    // 'error-message properties, then signal and catch it.
    let form = r#"(unwind-protect
      (progn
        (put 'neovm-test-custom-err 'error-conditions
             '(neovm-test-custom-err error))
        (put 'neovm-test-custom-err 'error-message
             "A custom test error")
        (list
          ;; Catch by specific symbol
          (condition-case err
              (signal 'neovm-test-custom-err '("detail-1" 42))
            (neovm-test-custom-err
             (list 'caught (car err) (cadr err) (caddr err))))
          ;; Catch by generic error (parent)
          (condition-case err
              (signal 'neovm-test-custom-err '("via-generic"))
            (error
             (list 'generic-catch (car err) (cadr err))))
          ;; error-message-string
          (condition-case err
              (signal 'neovm-test-custom-err '("payload"))
            (neovm-test-custom-err
             (error-message-string err)))))
      (put 'neovm-test-custom-err 'error-conditions nil)
      (put 'neovm-test-custom-err 'error-message nil))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught neovm-test-custom-err \"detail-1\" 42) (generic-catch neovm-test-custom-err \"via-generic\") \"A custom test error: \\\"payload\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// signal with complex data payloads (lists, alists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_complex_data_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Error data can be arbitrarily complex: nested lists, alists, vectors.
    let form = r#"(list
      ;; Nested list payload
      (condition-case err
          (signal 'error (list (list 1 2 3) (list 'a 'b) "msg"))
        (error
         (list (length (cdr err))
               (car (cadr err))
               (cadr (cadr err))
               (caddr (cadr err))
               (caddr err))))
      ;; Alist payload
      (condition-case err
          (signal 'error (list (list (cons 'key1 "val1")
                                     (cons 'key2 42)
                                     (cons 'key3 '(nested data)))))
        (error
         (let ((alist (cadr err)))
           (list (cdr (assq 'key1 alist))
                 (cdr (assq 'key2 alist))
                 (cdr (assq 'key3 alist))))))
      ;; Vector in payload
      (condition-case err
          (signal 'error (list [10 20 30] "extra"))
        (error
         (let ((v (cadr err)))
           (list (aref v 0) (aref v 1) (aref v 2)
                 (caddr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 1 2 3 (a b)) (\"val1\" 42 (nested data)) (10 20 30 \"extra\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error hierarchy (parent-child error types)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_error_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a 3-level error hierarchy:
    // error -> neovm-test-io-error -> neovm-test-file-not-found
    let form = r#"(unwind-protect
      (progn
        (put 'neovm-test-io-err 'error-conditions
             '(neovm-test-io-err error))
        (put 'neovm-test-io-err 'error-message "I/O error")
        (put 'neovm-test-fnf-err 'error-conditions
             '(neovm-test-fnf-err neovm-test-io-err error))
        (put 'neovm-test-fnf-err 'error-message "File not found")
        (list
          ;; Catch child by child symbol
          (condition-case err
              (signal 'neovm-test-fnf-err '("/tmp/missing.txt"))
            (neovm-test-fnf-err (list 'fnf (cadr err))))
          ;; Catch child by parent symbol
          (condition-case err
              (signal 'neovm-test-fnf-err '("/etc/secret"))
            (neovm-test-io-err (list 'io-parent (car err) (cadr err))))
          ;; Catch child by grandparent (error)
          (condition-case err
              (signal 'neovm-test-fnf-err '("deep"))
            (error (list 'error-gp (car err))))
          ;; Parent not caught by child handler
          (condition-case err
              (condition-case inner
                  (signal 'neovm-test-io-err '("generic-io"))
                (neovm-test-fnf-err (list 'should-not-match)))
            (neovm-test-io-err (list 'outer-io (cadr err))))))
      (put 'neovm-test-io-err 'error-conditions nil)
      (put 'neovm-test-io-err 'error-message nil)
      (put 'neovm-test-fnf-err 'error-conditions nil)
      (put 'neovm-test-fnf-err 'error-message nil))"#;
    let expect = expect_test::expect![[
        r#""OK ((fnf \"/tmp/missing.txt\") (io-parent neovm-test-fnf-err \"/etc/secret\") (error-gp neovm-test-fnf-err) (outer-io \"generic-io\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case catching parent error type (built-in hierarchy)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_builtin_hierarchy_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Built-in error types form a hierarchy. Test that parent catches
    // work for several built-in error families.
    let form = r#"(list
      ;; arith-error is a child of error
      (condition-case err
          (/ 1 0)
        (error (list 'caught-as-error (car err))))
      ;; wrong-type-argument is a child of error
      (condition-case err
          (car 42)
        (error (list 'caught-wta-as-error (car err))))
      ;; void-variable is a child of error
      (condition-case err
          (symbol-value 'neovm--test-unbound-xyz-123)
        (error (list 'caught-void-as-error (car err))))
      ;; Specific handler preferred over generic
      (condition-case err
          (/ 1 0)
        (arith-error 'specific-arith)
        (error 'generic-error))
      ;; Multiple specific: only matching fires
      (condition-case err
          (car "not-a-list")
        (arith-error 'wrong-match)
        (wrong-type-argument 'correct-match)
        (error 'fallback)))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught-as-error arith-error) (caught-wta-as-error wrong-type-argument) (caught-void-as-error void-variable) specific-arith correct-match)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_signal_adv_handler_bind_normal_return_continues_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU eval.c:Fhandler_bind_1 installs dynamic handlers that observe a
    // signal before condition-case.  If a handler returns normally, it does
    // not consume the signal; handler search continues from where it left off.
    // GNU lisp/subr.el:handler-bind is the public macro over handler-bind-1.
    let form = r#"
(list
 (let ((log nil))
   (condition-case err
       (handler-bind
           ((error (lambda (e)
                     (push (list :first e) log)
                     :first-return))
            (error (lambda (e)
                     (push (list :second e) log)
                     :second-return)))
         (signal 'error '("boom")))
     (error (list :caught err :log log))))
 (catch 'done
   (handler-bind
       ((error (lambda (e)
                 (throw 'done (list :thrown e)))))
     (signal 'error '("nonlocal"))))
 (condition-case err
     (handler-bind-1
      (lambda () (signal 'error '("missing-handler")))
      '(error))
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:caught (error \"boom\") :log ((:second (error \"boom\")) (:first (error \"boom\")))) (:thrown (error \"nonlocal\")) (error (\"Trailing CONDITIONS without HANDLER in ‘handler-bind‘\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// error function (convenient string-based signaling)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_error_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The `error` function signals an error with a formatted string message.
    let form = r#"(list
      ;; Basic error with simple string
      (condition-case err
          (error "something went wrong")
        (error (list (car err) (cadr err))))
      ;; error with format args
      (condition-case err
          (error "Expected %s but got %d" "string" 42)
        (error (cadr err)))
      ;; error vs signal: error auto-wraps message
      (condition-case err
          (error "auto-wrapped: %s" "test")
        (error
         (list (car err)
               (stringp (cadr err))
               (cadr err))))
      ;; Nested error calls
      (condition-case outer
          (condition-case inner
              (error "inner error %d" 1)
            (error
             (error "outer wraps: %s" (cadr inner))))
        (error (cadr outer))))"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"something went wrong\") \"Expected string but got 42\" (error t \"auto-wrapped: test\") \"outer wraps: inner error 1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// user-error vs error differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_user_error_vs_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // user-error signals 'user-error, which IS caught by (error ...)
    // handler since user-error is a child of error. But a specific
    // (user-error ...) handler catches it more precisely.
    let form = r#"(list
      ;; user-error caught by user-error handler
      (condition-case err
          (user-error "User did something wrong")
        (user-error (list 'user-err (cadr err))))
      ;; user-error caught by generic error handler
      (condition-case err
          (user-error "Also an error")
        (error (list 'generic (car err) (cadr err))))
      ;; user-error specific beats generic
      (condition-case err
          (user-error "specific wins")
        (user-error 'user-specific)
        (error 'error-generic))
      ;; Regular error NOT caught by user-error handler
      (condition-case err
          (condition-case inner
              (error "regular error")
            (user-error 'should-not-match))
        (error (list 'outer-caught (car inner))))
      ;; user-error with format args
      (condition-case err
          (user-error "Invalid input: %S (expected %s)" '(a b) "number")
        (user-error (cadr err))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable inner)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: error classification and recovery strategy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_classification_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a "robust executor" that classifies errors and applies
    // different recovery strategies: retry, default, or escalate.
    let form = r#"(unwind-protect
      (progn
        (put 'neovm-test-retryable 'error-conditions
             '(neovm-test-retryable error))
        (put 'neovm-test-defaultable 'error-conditions
             '(neovm-test-defaultable error))
        (put 'neovm-test-fatal 'error-conditions
             '(neovm-test-fatal error))
        (let ((attempt-count 0)
              (robust-execute nil))
          (setq robust-execute
                (lambda (thunk max-retries default-val)
                  (setq attempt-count 0)
                  (catch 'result
                    (let ((retries-left max-retries))
                      (while (>= retries-left 0)
                        (condition-case err
                            (throw 'result
                                   (list 'ok (funcall thunk)))
                          ;; Retryable: decrement and loop
                          (neovm-test-retryable
                           (setq attempt-count (1+ attempt-count))
                           (setq retries-left (1- retries-left)))
                          ;; Defaultable: return default
                          (neovm-test-defaultable
                           (throw 'result
                                  (list 'default default-val (cadr err))))
                          ;; Fatal: escalate immediately
                          (neovm-test-fatal
                           (throw 'result
                                  (list 'fatal (cadr err))))))
                      ;; Exhausted retries
                      (list 'exhausted attempt-count)))))
          (list
            ;; Success on first try
            (funcall robust-execute (lambda () 42) 3 0)
            ;; Always retryable -> exhausts retries
            (funcall robust-execute
                     (lambda ()
                       (signal 'neovm-test-retryable '("temp fail")))
                     2 nil)
            attempt-count
            ;; Defaultable
            (funcall robust-execute
                     (lambda ()
                       (signal 'neovm-test-defaultable '("missing")))
                     5 99)
            ;; Fatal
            (funcall robust-execute
                     (lambda ()
                       (signal 'neovm-test-fatal '("critical")))
                     10 0))))
      (put 'neovm-test-retryable 'error-conditions nil)
      (put 'neovm-test-defaultable 'error-conditions nil)
      (put 'neovm-test-fatal 'error-conditions nil))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 42) (exhausted 3) 3 (default 99 \"missing\") (fatal \"critical\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: error wrapping/chaining (add context to errors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_adv_error_wrapping_chaining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate error chaining: each layer catches, adds context,
    // and re-signals with accumulated information.
    let form = r#"(let ((wrap-error
                     (lambda (layer-name thunk)
                       (condition-case err
                           (funcall thunk)
                         (error
                          (signal 'error
                                  (list (format "[%s] %s"
                                                layer-name
                                                (if (stringp (cadr err))
                                                    (cadr err)
                                                  (prin1-to-string (cdr err))))
                                        (cons layer-name
                                              (if (and (cddr err)
                                                       (listp (caddr err)))
                                                  (caddr err)
                                                nil)))))))))
      ;; Build a 4-layer call stack that fails at the bottom
      (condition-case final-err
          (funcall wrap-error "http-client"
                   (lambda ()
                     (funcall wrap-error "auth-service"
                              (lambda ()
                                (funcall wrap-error "token-validator"
                                         (lambda ()
                                           (funcall wrap-error "crypto-lib"
                                                    (lambda ()
                                                      (error "invalid key length: %d" 7)))))))))
        (error
         (list
          ;; The message should have all layer prefixes
          (cadr final-err)
          ;; The chain should record layer names
          (caddr final-err)
          ;; Error symbol is still 'error
          (car final-err)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[http-client] [auth-service] [token-validator] [crypto-lib] invalid key length: 7\" (\"http-client\" \"auth-service\" \"token-validator\" \"crypto-lib\") error)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
