//! Advanced oracle parity tests for `unwind-protect`.
//!
//! Covers: multiple cleanup forms, cleanup on throw, cleanup on signal/error,
//! nested unwind-protect LIFO ordering, error inside cleanup, return value
//! preservation, RAII-like patterns, and dynamic binding restoration.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multiple cleanup forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_multiple_cleanup_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unwind-protect takes ONE body form and MULTIPLE cleanup forms.
    // All cleanup forms run and their return values are ignored.
    let form = r#"(let ((log nil))
                    (unwind-protect
                        (progn (setq log (cons 'body log)) 'result)
                      (setq log (cons 'clean1 log))
                      (setq log (cons 'clean2 log))
                      (setq log (cons 'clean3 log)))
                    (nreverse log))"#;
    let expect = expect_test::expect![[r#""OK (body clean1 clean2 clean3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_cleanup_values_discarded() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The return value of unwind-protect is the body form's value,
    // even though cleanup forms return different values.
    let form = r#"(unwind-protect
                      42
                    (+ 1 2)
                    (+ 3 4)
                    (* 5 6))"#;
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("42", &o, &n);
}

// ---------------------------------------------------------------------------
// Cleanup runs on throw
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_cleanup_on_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When throw unwinds the stack, unwind-protect cleanup still runs.
    let form = r#"(let ((log nil))
                    (catch 'bail
                      (unwind-protect
                          (progn
                            (setq log (cons 'before-throw log))
                            (throw 'bail 'thrown-value)
                            (setq log (cons 'never-reached log)))
                        (setq log (cons 'cleanup-ran log))
                        (setq log (cons 'cleanup-2-ran log))))
                    (nreverse log))"#;
    let expect = expect_test::expect![[r#""OK (before-throw cleanup-ran cleanup-2-ran)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_throw_preserves_value_through_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The throw's value is preserved as the catch result, even though
    // unwind-protect cleanup forms execute in between.
    let form = r#"(let ((cleanup-ran nil))
                    (let ((result
                           (catch 'exit
                             (unwind-protect
                                 (throw 'exit '(complex data 42))
                               (setq cleanup-ran t)))))
                      (list result cleanup-ran)))"#;
    let expect = expect_test::expect![[r#""OK ((complex data 42) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cleanup runs on signal/error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_cleanup_on_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // signal propagation: cleanup runs, then error continues to handler
    let form = r#"(let ((log nil))
                    (condition-case err
                        (unwind-protect
                            (progn
                              (setq log (cons 'body log))
                              (signal 'arith-error '("division by zero"))
                              (setq log (cons 'unreachable log)))
                          (setq log (cons 'cleanup-1 log))
                          (setq log (cons 'cleanup-2 log)))
                      (arith-error
                       (setq log (cons (list 'caught (car err)) log))))
                    (nreverse log))"#;
    let expect = expect_test::expect![[r#""OK (body cleanup-1 cleanup-2 (caught arith-error))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested unwind-protect LIFO ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_nested_lifo_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested unwind-protect forms should run cleanup in LIFO order
    // (innermost first, outermost last).
    let form = r#"(let ((log nil))
                    (condition-case nil
                        (unwind-protect
                            (unwind-protect
                                (unwind-protect
                                    (progn
                                      (setq log (cons 'body log))
                                      (error "boom"))
                                  (setq log (cons 'inner log)))
                              (setq log (cons 'middle log)))
                          (setq log (cons 'outer log)))
                      (error nil))
                    (nreverse log))"#;
    let expect = expect_test::expect![[r#""OK (body inner middle outer)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_nested_lifo_with_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Same LIFO ordering but triggered by throw instead of error.
    let form = r#"(let ((log nil))
                    (catch 'exit
                      (unwind-protect
                          (unwind-protect
                              (unwind-protect
                                  (progn
                                    (setq log (cons 'deepest log))
                                    (throw 'exit 'done))
                                (setq log (cons 'cleanup-3 log)))
                            (setq log (cons 'cleanup-2 log)))
                        (setq log (cons 'cleanup-1 log))))
                    (nreverse log))"#;
    let expect = expect_test::expect![[r#""OK (deepest cleanup-3 cleanup-2 cleanup-1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(deepest cleanup-3 cleanup-2 cleanup-1)", &o, &n);
}

// ---------------------------------------------------------------------------
// Error in cleanup form itself
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_error_in_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // If the body succeeds but a cleanup form signals an error,
    // that error propagates (the body's return value is lost).
    let form = r#"(condition-case err
                      (unwind-protect
                          'body-value
                        (error "cleanup exploded"))
                    (error (list 'caught (cadr err))))"#;
    let expect = expect_test::expect![[r#""OK (caught \"cleanup exploded\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_error_in_cleanup_overrides_body_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When body signals an error AND cleanup also signals an error,
    // the cleanup error replaces the body error.
    let form = r#"(condition-case err
                      (unwind-protect
                          (error "body error")
                        (error "cleanup error"))
                    (error (cadr err)))"#;
    let expect = expect_test::expect![[r#""OK \"cleanup error\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_partial_cleanup_on_cleanup_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple cleanup forms: if second cleanup errors, first already ran
    // but third does NOT run (error aborts remaining cleanup forms).
    let form = r#"(let ((log nil))
                    (condition-case nil
                        (unwind-protect
                            'body
                          (setq log (cons 'c1 log))
                          (error "cleanup-2 failed")
                          (setq log (cons 'c3 log)))
                      (error nil))
                    (nreverse log))"#;
    let expect = expect_test::expect![[r#""OK (c1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Return value preservation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_preserves_body_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex body expression: its value is returned, not the cleanup value.
    let form = r#"(let ((counter 0))
                    (let ((result
                           (unwind-protect
                               (progn
                                 (setq counter (1+ counter))
                                 (setq counter (1+ counter))
                                 (list 'computed counter))
                             (setq counter 999))))
                      (list result counter)))"#;
    let expect = expect_test::expect![[r#""OK ((computed 2) 999)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_preserves_nil_body_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Body returning nil: cleanup must not accidentally override it.
    let form = r#"(let ((side nil))
                    (let ((result
                           (unwind-protect
                               nil
                             (setq side 'cleaned))))
                      (list result side)))"#;
    let expect = expect_test::expect![[r#""OK (nil cleaned)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: RAII resource acquisition/release pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_raii_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate resource management: acquire -> use -> release.
    // Even if "use" throws, "release" must happen.
    let form = r#"(let ((resources nil)
                        (operations nil))
                    (let ((acquire (lambda (name)
                            (setq resources (cons name resources))
                            (setq operations (cons (list 'acquired name) operations))
                            name))
                          (release (lambda (name)
                            (setq resources (delete name resources))
                            (setq operations (cons (list 'released name) operations)))))
                      ;; Acquire multiple resources, inner one causes error
                      (condition-case nil
                          (progn
                            (funcall acquire 'db-conn)
                            (unwind-protect
                                (progn
                                  (funcall acquire 'file-handle)
                                  (unwind-protect
                                      (progn
                                        (funcall acquire 'lock)
                                        (unwind-protect
                                            (error "operation failed")
                                          (funcall release 'lock)))
                                    (funcall release 'file-handle)))
                              (funcall release 'db-conn)))
                        (error nil))
                      (list (nreverse operations) resources)))"#;
    let expect = expect_test::expect![[
        r#""OK (((acquired db-conn) (acquired file-handle) (acquired lock) (released lock) (released file-handle) (released db-conn)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: unwind-protect with dynamic binding restoration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unwind_protect_dynamic_binding_restoration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate save/restore of dynamic state using unwind-protect.
    // This is how save-excursion, save-match-data, etc. work internally.
    let form = r#"(let ((state-stack nil)
                        (current-state 'initial))
                    (let ((save-state (lambda ()
                            (setq state-stack (cons current-state state-stack))))
                          (restore-state (lambda ()
                            (setq current-state (car state-stack))
                            (setq state-stack (cdr state-stack)))))
                      (let ((log nil))
                        ;; Save, modify, error, restore
                        (funcall save-state)
                        (unwind-protect
                            (progn
                              (setq current-state 'modified-1)
                              (setq log (cons (list 'inside-1 current-state) log))
                              ;; Nested save/restore with throw
                              (funcall save-state)
                              (catch 'bail
                                (unwind-protect
                                    (progn
                                      (setq current-state 'modified-2)
                                      (setq log (cons (list 'inside-2 current-state) log))
                                      (throw 'bail nil))
                                  (funcall restore-state)))
                              (setq log (cons (list 'after-throw current-state) log)))
                          (funcall restore-state))
                        (setq log (cons (list 'final current-state) log))
                        (nreverse log))))"#;
    let expect = expect_test::expect![[
        r#""OK ((inside-1 modified-1) (inside-2 modified-2) (after-throw modified-1) (final initial))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_condition_case_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex interplay: condition-case inside unwind-protect,
    // with re-signaling from handler.
    let form = r#"(let ((log nil))
                    (condition-case outer-err
                        (unwind-protect
                            (condition-case inner-err
                                (progn
                                  (setq log (cons 'body log))
                                  (error "inner error"))
                              (error
                               (setq log (cons (list 'caught-inner (cadr inner-err)) log))
                               ;; Re-signal a different error
                               (error "re-signaled from handler")))
                          (setq log (cons 'outer-cleanup log)))
                      (error
                       (setq log (cons (list 'caught-outer (cadr outer-err)) log))))
                    (nreverse log))"#;
    let expect = expect_test::expect![[
        r#""OK (body (caught-inner \"inner error\") outer-cleanup (caught-outer \"re-signaled from handler\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_unwind_protect_loop_with_early_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unwind-protect inside a loop with catch/throw for early exit.
    // Each iteration's cleanup must run.
    let form = r#"(let ((log nil))
                    (catch 'done
                      (let ((items '(a b c d e)))
                        (dolist (item items)
                          (unwind-protect
                              (progn
                                (setq log (cons (list 'process item) log))
                                (when (eq item 'c)
                                  (throw 'done 'stopped-at-c)))
                            (setq log (cons (list 'cleanup item) log))))))
                    (nreverse log))"#;
    let expect = expect_test::expect![[
        r#""OK ((process a) (cleanup a) (process b) (cleanup b) (process c) (cleanup c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
