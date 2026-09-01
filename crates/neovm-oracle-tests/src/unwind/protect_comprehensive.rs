//! Comprehensive oracle parity tests for `unwind-protect`.
//!
//! Covers: cleanup on normal exit, cleanup on error, cleanup on throw,
//! nested unwind-protect, cleanup ordering (LIFO), cleanup with side effects,
//! interaction with condition-case, cleanup that itself signals, multiple body
//! forms via progn, and dynamic binding restoration via unwind-protect.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Cleanup runs on normal (non-error) exit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_cleanup_on_normal_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify cleanup forms execute and accumulate side effects
    // even when the body completes successfully without error or throw.
    let form = r#"
(let ((log nil))
  (let ((result
         (unwind-protect
             (progn
               (setq log (cons 'body-start log))
               (setq log (cons 'body-end log))
               'body-result)
           (setq log (cons 'cleanup-a log))
           (setq log (cons 'cleanup-b log))
           (setq log (cons 'cleanup-c log)))))
    (list result (nreverse log))))
"#;
    let expect = expect_test::expect![[
        r#""OK (body-result (body-start body-end cleanup-a cleanup-b cleanup-c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_comp_normal_exit_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Body return value must survive even when cleanup forms produce
    // values of different types (int, string, list, symbol).
    let form = r#"
(unwind-protect
    (+ 100 200 300)
  "ignored-string"
  '(ignored list)
  42
  :ignored-keyword
  t)
"#;
    let expect = expect_test::expect![[r#""OK 600""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(o, "OK 600");
    assert_eq!(n, o);
}

// ---------------------------------------------------------------------------
// Cleanup runs on error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_cleanup_on_error_with_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify cleanup runs on signal and the full error data propagates
    // to the handler. Cleanup accumulates observable side effects.
    let form = r#"
(let ((log nil))
  (condition-case err
      (unwind-protect
          (progn
            (setq log (cons 'before-error log))
            (signal 'wrong-type-argument '(stringp 42))
            (setq log (cons 'unreachable log)))
        (setq log (cons 'cleanup-ran log)))
    (wrong-type-argument
     (list 'caught
           (car err)
           (cadr err)
           (nreverse log)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (caught wrong-type-argument stringp (before-error cleanup-ran))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_comp_cleanup_on_user_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // (error ...) is a convenience wrapper around signal.
    // Cleanup should still fire and error message propagates.
    let form = r#"
(let ((resource-freed nil))
  (condition-case err
      (unwind-protect
          (error "cannot open file %s: %s" "/tmp/foo" "permission denied")
        (setq resource-freed t))
    (error
     (list resource-freed (cadr err)))))
"#;
    let expect =
        expect_test::expect![[r#""OK (t \"cannot open file /tmp/foo: permission denied\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cleanup runs on throw
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_cleanup_on_nested_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple catch tags; an inner throw unwinds through two
    // unwind-protects before reaching its catch.
    let form = r#"
(let ((log nil))
  (catch 'outer
    (catch 'inner
      (unwind-protect
          (unwind-protect
              (progn
                (setq log (cons 'deep-body log))
                (throw 'outer 'escaped))
            (setq log (cons 'inner-cleanup log)))
        (setq log (cons 'outer-cleanup log)))))
  (nreverse log))
"#;
    let expect = expect_test::expect![[r#""OK (deep-body inner-cleanup outer-cleanup)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested unwind-protect — LIFO cleanup ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_five_deep_lifo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Five levels deep; verify LIFO ordering on both normal exit
    // and error exit.
    let form = r#"
(let ((log nil))
  ;; Normal exit path
  (unwind-protect
      (unwind-protect
          (unwind-protect
              (unwind-protect
                  (unwind-protect
                      (setq log (cons 'body log))
                    (setq log (cons 'c5 log)))
                (setq log (cons 'c4 log)))
            (setq log (cons 'c3 log)))
        (setq log (cons 'c2 log)))
    (setq log (cons 'c1 log)))
  (let ((normal-order (nreverse log)))
    ;; Error exit path
    (setq log nil)
    (condition-case nil
        (unwind-protect
            (unwind-protect
                (unwind-protect
                    (unwind-protect
                        (unwind-protect
                            (progn
                              (setq log (cons 'body log))
                              (error "boom"))
                          (setq log (cons 'c5 log)))
                      (setq log (cons 'c4 log)))
                  (setq log (cons 'c3 log)))
              (setq log (cons 'c2 log)))
          (setq log (cons 'c1 log)))
      (error nil))
    (list normal-order (nreverse log))))
"#;
    let expect = expect_test::expect![[r#""OK ((body c5 c4 c3 c2 c1) (body c5 c4 c3 c2 c1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cleanup with side effects: mutation tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_side_effect_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a counter incremented in cleanups to verify each cleanup
    // runs exactly once, in the correct total count.
    let form = r#"
(let ((counter 0))
  (dotimes (_ 5)
    (unwind-protect
        (setq counter (+ counter 10))
      (setq counter (1+ counter))))
  ;; 5 iterations: body adds 10 each time = 50, cleanup adds 1 = 5
  counter)
"#;
    let expect = expect_test::expect![[r#""OK 55""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_comp_cleanup_modifies_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Cleanups that build up an alist for audit logging.
    let form = r#"
(let ((audit nil))
  (condition-case nil
      (let ((items '(alpha beta gamma delta)))
        (dolist (item items)
          (unwind-protect
              (progn
                (setq audit (cons (cons item 'started) audit))
                (when (eq item 'gamma)
                  (error "gamma failed")))
            (setq audit (cons (cons item 'cleaned) audit)))))
    (error nil))
  (nreverse audit))
"#;
    let expect = expect_test::expect![[
        r#""OK ((alpha . started) (alpha . cleaned) (beta . started) (beta . cleaned) (gamma . started) (gamma . cleaned))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with condition-case
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_condition_case_inside_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // condition-case inside unwind-protect body catches errors locally,
    // so cleanup still runs but no error propagates.
    let form = r#"
(let ((log nil))
  (let ((result
         (unwind-protect
             (condition-case err
                 (progn
                   (setq log (cons 'try log))
                   (/ 1 0))
               (arith-error
                (setq log (cons 'handled log))
                'recovered))
           (setq log (cons 'cleanup log)))))
    (list result (nreverse log))))
"#;
    let expect = expect_test::expect![[r#""OK (recovered (try handled cleanup))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_comp_condition_case_wrapping_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // condition-case wrapping unwind-protect: the handler runs AFTER
    // the cleanup, and sees the original error.
    let form = r#"
(let ((log nil))
  (condition-case err
      (unwind-protect
          (progn
            (setq log (cons 'body log))
            (signal 'void-variable '(undefined-sym)))
        (setq log (cons 'cleanup log)))
    (void-variable
     (setq log (cons (list 'handler (car err) (cadr err)) log))))
  (nreverse log))
"#;
    let expect =
        expect_test::expect![[r#""OK (body cleanup (handler void-variable undefined-sym))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cleanup that itself signals an error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_cleanup_error_replaces_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // If body throws but cleanup signals an error, the throw is lost
    // and the cleanup's error propagates instead.
    let form = r#"
(condition-case err
    (catch 'tag
      (unwind-protect
          (throw 'tag 'thrown-value)
        (error "cleanup boom")))
  (error (list 'caught-cleanup-error (cadr err))))
"#;
    let expect = expect_test::expect![[r#""OK (caught-cleanup-error \"cleanup boom\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_comp_nested_cleanup_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested unwind-protects where each cleanup errors.
    // The outermost cleanup error is the one that propagates.
    let form = r#"
(let ((log nil))
  (condition-case err
      (unwind-protect
          (unwind-protect
              (progn
                (setq log (cons 'body log))
                (error "body-error"))
            (setq log (cons 'inner-cleanup log))
            (error "inner-cleanup-error"))
        (setq log (cons 'outer-cleanup log))
        (error "outer-cleanup-error"))
    (error
     (list (cadr err) (nreverse log)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"outer-cleanup-error\" (body inner-cleanup outer-cleanup))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple body forms via progn
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_progn_body_last_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unwind-protect only takes one body form. The standard pattern is
    // wrapping in progn. Verify last form's value is returned.
    let form = r#"
(let ((trace nil))
  (let ((val
         (unwind-protect
             (progn
               (setq trace (cons 1 trace))
               (setq trace (cons 2 trace))
               (setq trace (cons 3 trace))
               (* 7 8))
           (setq trace (cons 'cleaned trace)))))
    (list val (nreverse trace))))
"#;
    let expect = expect_test::expect![[r#""OK (56 (1 2 3 cleaned))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding restoration via unwind-protect
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_dynamic_binding_restore_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate saving and restoring a dynamically-scoped variable
    // using unwind-protect, verifying restoration on error.
    let form = r#"
(progn
  (defvar neovm--uwp-test-var 'original)
  (unwind-protect
      (let ((saved neovm--uwp-test-var))
        (condition-case nil
            (unwind-protect
                (progn
                  (setq neovm--uwp-test-var 'modified)
                  (error "fail"))
              (setq neovm--uwp-test-var saved))
          (error nil))
        ;; Should be restored to 'original
        (let ((after-restore neovm--uwp-test-var))
          ;; Modify again, this time with throw
          (let ((saved2 neovm--uwp-test-var))
            (catch 'bail
              (unwind-protect
                  (progn
                    (setq neovm--uwp-test-var 'modified-again)
                    (throw 'bail nil))
                (setq neovm--uwp-test-var saved2)))
            (list after-restore neovm--uwp-test-var))))
    (makunbound 'neovm--uwp-test-var)))
"#;
    let expect = expect_test::expect![[r#""OK (original original)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_comp_let_binding_with_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `let` already restores bindings, but verify unwind-protect inside
    // let body does not interfere with let's own restoration.
    let form = r#"
(progn
  (defvar neovm--uwp-let-var 'global-val)
  (unwind-protect
      (let ((results nil))
        (let ((neovm--uwp-let-var 'let-bound))
          (unwind-protect
              (progn
                (setq results (cons neovm--uwp-let-var results))
                (setq neovm--uwp-let-var 'mutated-inside)
                (setq results (cons neovm--uwp-let-var results)))
            (setq results (cons (list 'cleanup neovm--uwp-let-var) results))))
        ;; After let: should be back to global-val
        (setq results (cons neovm--uwp-let-var results))
        (nreverse results))
    (makunbound 'neovm--uwp-let-var)))
"#;
    let expect = expect_test::expect![[
        r#""OK (let-bound mutated-inside (cleanup mutated-inside) global-val)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: recursive unwind-protect with accumulator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_recursive_cleanup_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive function where each level uses unwind-protect to
    // log entry/exit; throw from depth causes LIFO cleanup cascade.
    let form = r#"
(progn
  (fset 'neovm--uwp-recurse
    (lambda (depth max-depth log-ref)
      (if (= depth max-depth)
          (progn
            (set log-ref (cons (list 'throw-at depth) (symbol-value log-ref)))
            (throw 'bail (list 'from-depth depth)))
        (unwind-protect
            (progn
              (set log-ref (cons (list 'enter depth) (symbol-value log-ref)))
              (funcall 'neovm--uwp-recurse (1+ depth) max-depth log-ref))
          (set log-ref (cons (list 'exit depth) (symbol-value log-ref)))))))

  (unwind-protect
      (progn
        (defvar neovm--uwp-log nil)
        (let ((result (catch 'bail
                        (funcall 'neovm--uwp-recurse 0 4 'neovm--uwp-log))))
          (list result (nreverse neovm--uwp-log))))
    (fmakunbound 'neovm--uwp-recurse)
    (makunbound 'neovm--uwp-log)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((from-depth 4) ((enter 0) (enter 1) (enter 2) (enter 3) (throw-at 4) (exit 3) (exit 2) (exit 1) (exit 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: unwind-protect as resource guard pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_comp_resource_guard_multiple_resources() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Acquire 3 resources sequentially, use them, then release in
    // reverse order even if intermediate steps error.
    let form = r#"
(let ((acquired nil)
      (released nil))
  (condition-case nil
      (unwind-protect
          (progn
            (setq acquired (cons 'mutex acquired))
            (unwind-protect
                (progn
                  (setq acquired (cons 'file acquired))
                  (unwind-protect
                      (progn
                        (setq acquired (cons 'socket acquired))
                        ;; Simulate work then error
                        (error "network timeout"))
                    (setq released (cons 'socket released))))
              (setq released (cons 'file released))))
        (setq released (cons 'mutex released)))
    (error nil))
  (list (nreverse acquired) released))
"#;
    let expect = expect_test::expect![[r#""OK ((mutex file socket) (mutex file socket))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
