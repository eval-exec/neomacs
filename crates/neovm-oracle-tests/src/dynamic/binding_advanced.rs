//! Advanced oracle parity tests for dynamic binding.
//!
//! Tests dynamic vs lexical scoping differences, nested rebinding,
//! unwinding on error, interaction with closures, condition-case,
//! unwind-protect, and aspect-oriented / context-system patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Dynamic vs lexical binding behavior difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_vs_lexical_scoping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A defvar'd variable is dynamically scoped: a function sees the caller's binding.
    // A non-defvar'd variable is lexically scoped: a function sees the defining scope.
    let form = "(progn
                  (defvar neovm--test-dyn-scope 'global-dyn)
                  (unwind-protect
                      (let ((lex-var 'global-lex))
                        (let ((read-dyn (lambda () neovm--test-dyn-scope))
                              (read-lex (lambda () lex-var)))
                          ;; Under new dynamic binding, function sees new value
                          (let ((neovm--test-dyn-scope 'local-dyn))
                            (let ((lex-var 'local-lex))
                              (list
                                ;; Dynamic: sees 'local-dyn (caller's binding)
                                (funcall read-dyn)
                                ;; Lexical: still sees 'global-lex (defining scope)
                                (funcall read-lex))))))
                    (makunbound 'neovm--test-dyn-scope)))";
    let expect = expect_test::expect![[r#""OK (local-dyn global-lex)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple nested dynamic bindings with same variable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_deeply_nested_same_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Push 5 nested bindings, read at each level, then verify unwinding
    let form = "(progn
                  (defvar neovm--test-nest-v 0)
                  (unwind-protect
                      (let ((reader (lambda () neovm--test-nest-v))
                            (results nil))
                        (setq results (cons (funcall reader) results))
                        (let ((neovm--test-nest-v 10))
                          (setq results (cons (funcall reader) results))
                          (let ((neovm--test-nest-v 20))
                            (setq results (cons (funcall reader) results))
                            (let ((neovm--test-nest-v 30))
                              (setq results (cons (funcall reader) results))
                              (let ((neovm--test-nest-v 40))
                                (setq results (cons (funcall reader) results))
                                (let ((neovm--test-nest-v 50))
                                  (setq results (cons (funcall reader) results))))
                              ;; After innermost unwinds, back to 30
                              (setq results (cons (funcall reader) results)))
                            ;; Back to 20
                            (setq results (cons (funcall reader) results)))
                          ;; Back to 10
                          (setq results (cons (funcall reader) results)))
                        ;; Back to 0
                        (setq results (cons (funcall reader) results))
                        (nreverse results))
                    (makunbound 'neovm--test-nest-v)))";
    let expect = expect_test::expect![[r#""OK (0 10 20 30 40 50 30 20 10 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding unwinding on error (condition-case)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_unwind_on_error_cascade() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested dynamic bindings with error at different depths;
    // verify each level restores correctly
    let form = "(progn
                  (defvar neovm--test-ue-a 'base-a)
                  (defvar neovm--test-ue-b 'base-b)
                  (unwind-protect
                      (let ((snapshots nil))
                        ;; Level 1
                        (let ((neovm--test-ue-a 'level1-a)
                              (neovm--test-ue-b 'level1-b))
                          ;; Level 2 — error here
                          (condition-case nil
                              (let ((neovm--test-ue-a 'level2-a)
                                    (neovm--test-ue-b 'level2-b))
                                ;; Level 3 — error thrown here
                                (let ((neovm--test-ue-a 'level3-a))
                                  (signal 'error '(\"deep error\"))))
                            (error
                             ;; After error, should be back at level 1
                             (setq snapshots
                                   (list neovm--test-ue-a
                                         neovm--test-ue-b))))
                          ;; Still at level 1
                          (setq snapshots
                                (append snapshots
                                        (list neovm--test-ue-a
                                              neovm--test-ue-b))))
                        ;; Back at base
                        (append snapshots
                                (list neovm--test-ue-a
                                      neovm--test-ue-b)))
                    (makunbound 'neovm--test-ue-a)
                    (makunbound 'neovm--test-ue-b)))";
    let expect =
        expect_test::expect![[r#""OK (level1-a level1-b level1-a level1-b base-a base-b)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding interaction with closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_closure_capture_vs_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A closure captures lexical vars at definition time, but dynamic vars
    // are looked up at call time. Test with a factory that creates closures.
    let form = "(progn
                  (defvar neovm--test-dc-mode 'default)
                  (unwind-protect
                      (let ((make-printer
                             (lambda (prefix)
                               ;; prefix is lexical, neovm--test-dc-mode is dynamic
                               (lambda (msg)
                                 (format \"%s[%s]: %s\" prefix neovm--test-dc-mode msg)))))
                        (let ((p1 (funcall make-printer \"LOG\"))
                              (p2 (funcall make-printer \"ERR\")))
                          (let ((r1 (funcall p1 \"hello\"))
                                (r2 (funcall p2 \"world\")))
                            ;; Now change dynamic mode
                            (let ((neovm--test-dc-mode 'verbose))
                              (let ((r3 (funcall p1 \"hello\"))
                                    (r4 (funcall p2 \"world\")))
                                ;; Verify prefix (lexical) stays, mode (dynamic) changes
                                (list r1 r2 r3 r4))))))
                    (makunbound 'neovm--test-dc-mode)))";
    let expect = expect_test::expect![[
        r#""OK (\"LOG[default]: hello\" \"ERR[default]: world\" \"LOG[verbose]: hello\" \"ERR[verbose]: world\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic-binding-based environment/context system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_environment_context_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a logging context system using dynamic binding stack:
    // each 'with-context' pushes a tag onto a dynamic context list
    let form = "(progn
                  (defvar neovm--test-ctx-stack nil)
                  (unwind-protect
                      (let ((with-context
                             (lambda (tag body-fn)
                               (let ((neovm--test-ctx-stack
                                      (cons tag neovm--test-ctx-stack)))
                                 (funcall body-fn))))
                            (get-context
                             (lambda ()
                               (mapconcat #'symbol-name
                                          (reverse neovm--test-ctx-stack)
                                          \"/\")))
                            (results nil))
                        ;; Nested contexts
                        (funcall with-context 'app
                          (lambda ()
                            (setq results (cons (funcall get-context) results))
                            (funcall with-context 'http
                              (lambda ()
                                (setq results (cons (funcall get-context) results))
                                (funcall with-context 'handler
                                  (lambda ()
                                    (setq results (cons (funcall get-context) results))))))
                            ;; After inner contexts unwind
                            (setq results (cons (funcall get-context) results))))
                        ;; Fully unwound
                        (setq results (cons (funcall get-context) results))
                        (nreverse results))
                    (makunbound 'neovm--test-ctx-stack)))";
    let expect =
        expect_test::expect![[r#""OK (\"app\" \"app/http\" \"app/http/handler\" \"app\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Aspect-oriented programming using dynamic binding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_aspect_oriented_tracing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use dynamic binding to implement a simple function-call tracing aspect:
    // a trace log is dynamically bound, and wrapper functions append to it
    let form = "(progn
                  (defvar neovm--test-trace-log nil)
                  (unwind-protect
                      (let ((traced-call
                             (lambda (name fn &rest args)
                               (setq neovm--test-trace-log
                                     (cons (list 'enter name args)
                                           neovm--test-trace-log))
                               (let ((result (apply fn args)))
                                 (setq neovm--test-trace-log
                                       (cons (list 'exit name result)
                                             neovm--test-trace-log))
                                 result)))
                            (add (lambda (a b) (+ a b)))
                            (mul (lambda (a b) (* a b))))
                        ;; Execute with tracing enabled
                        (let ((neovm--test-trace-log nil))
                          (let ((r1 (funcall traced-call 'add add 3 4)))
                            (let ((r2 (funcall traced-call 'mul mul r1 5)))
                              (list r1 r2 (nreverse neovm--test-trace-log))))))
                    (makunbound 'neovm--test-trace-log)))";
    let expect = expect_test::expect![[
        r#""OK (7 35 ((enter add (3 4)) (exit add 7) (enter mul (7 5)) (exit mul 35)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding + condition-case + unwind-protect three-way interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_condition_case_unwind_protect_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that dynamic bindings, condition-case, and unwind-protect
    // all interact correctly during complex non-local exits
    let form = "(progn
                  (defvar neovm--test-cwu-val 'initial)
                  (defvar neovm--test-cwu-log nil)
                  (unwind-protect
                      (progn
                        ;; Outer unwind-protect around a condition-case
                        (unwind-protect
                            (condition-case err
                                (let ((neovm--test-cwu-val 'outer-let))
                                  (setq neovm--test-cwu-log
                                        (cons (cons 'before-inner neovm--test-cwu-val)
                                              neovm--test-cwu-log))
                                  ;; Inner unwind-protect that changes the var
                                  (unwind-protect
                                      (let ((neovm--test-cwu-val 'inner-let))
                                        (setq neovm--test-cwu-log
                                              (cons (cons 'in-inner neovm--test-cwu-val)
                                                    neovm--test-cwu-log))
                                        (signal 'arith-error '(\"test\")))
                                    ;; Cleanup: dynamic binding already restored to outer-let
                                    (setq neovm--test-cwu-log
                                          (cons (cons 'inner-cleanup neovm--test-cwu-val)
                                                neovm--test-cwu-log))))
                              (arith-error
                               ;; Handler: dynamic binding restored to outer scope
                               (setq neovm--test-cwu-log
                                     (cons (cons 'handler neovm--test-cwu-val)
                                           neovm--test-cwu-log))))
                          ;; Outer cleanup: all lets unwound
                          (setq neovm--test-cwu-log
                                (cons (cons 'outer-cleanup neovm--test-cwu-val)
                                      neovm--test-cwu-log)))
                        ;; Final state
                        (nreverse neovm--test-cwu-log))
                    (makunbound 'neovm--test-cwu-val)
                    (makunbound 'neovm--test-cwu-log)))";
    let expect = expect_test::expect![[
        r#""OK ((before-inner . outer-let) (in-inner . inner-let) (inner-cleanup . outer-let) (handler . initial) (outer-cleanup . initial))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding with catch/throw
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_binding_catch_throw_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that dynamic bindings are properly restored when
    // throw crosses multiple let boundaries
    let form = "(progn
                  (defvar neovm--test-ct-v 'base)
                  (unwind-protect
                      (let ((result
                             (catch 'bail
                               (let ((neovm--test-ct-v 'level1))
                                 (let ((neovm--test-ct-v 'level2))
                                   (let ((neovm--test-ct-v 'level3))
                                     (throw 'bail
                                            (list 'thrown-at neovm--test-ct-v))))))))
                        ;; After catch, dynamic binding should be restored to 'base
                        (list result neovm--test-ct-v))
                    (makunbound 'neovm--test-ct-v)))";
    let expect = expect_test::expect![[r#""OK ((thrown-at level3) base)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
