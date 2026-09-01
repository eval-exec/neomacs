//! Oracle parity tests for complex `catch`/`throw` patterns:
//! non-local return from deep call stacks, nested catch with
//! same tags, complex thrown values, interactions with
//! condition-case and unwind-protect, coroutine simulation,
//! and exception-like error handling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Non-local return from deep call stack
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_deep_nonlocal_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // throw unwinds through 5 levels of function calls, collecting
    // breadcrumbs along the way via unwind-protect.
    let form = r#"(let ((trail nil))
                    (let ((level5 (lambda ()
                                    (unwind-protect
                                        (throw 'escape '(from-level-5))
                                      (setq trail (cons 'unwind-5 trail)))))
                          (level4 (lambda (fn)
                                    (unwind-protect
                                        (progn
                                          (setq trail (cons 'enter-4 trail))
                                          (funcall fn))
                                      (setq trail (cons 'unwind-4 trail)))))
                          (level3 (lambda (fn4 fn5)
                                    (unwind-protect
                                        (progn
                                          (setq trail (cons 'enter-3 trail))
                                          (funcall fn4 fn5))
                                      (setq trail (cons 'unwind-3 trail)))))
                          (level2 (lambda (fn3 fn4 fn5)
                                    (unwind-protect
                                        (funcall fn3 fn4 fn5)
                                      (setq trail (cons 'unwind-2 trail)))))
                          (level1 (lambda (fn2 fn3 fn4 fn5)
                                    (funcall fn2 fn3 fn4 fn5))))
                      (let ((result (catch 'escape
                                      (funcall level1 level2 level3 level4 level5))))
                        (list result (nreverse trail)))))"#;
    let expect = expect_test::expect![
        r#""OK ((from-level-5) (enter-3 enter-4 unwind-5 unwind-4 unwind-3 unwind-2))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested catch with same tag: innermost catches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_same_tag_innermost_catches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of catch with the same tag 'done.
    // throw always goes to the innermost matching catch.
    let form = r#"(let ((results nil))
                    ;; Outer catch
                    (let ((r1 (catch 'done
                                (setq results (cons 'outer-body results))
                                ;; Middle catch
                                (let ((r2 (catch 'done
                                            (setq results (cons 'middle-body results))
                                            ;; Inner catch
                                            (let ((r3 (catch 'done
                                                        (setq results (cons 'inner-body results))
                                                        (throw 'done 'inner-val))))
                                              (setq results (cons 'after-inner results))
                                              ;; This throw goes to middle
                                              (throw 'done (list 'from-middle r3))))))
                                  (setq results (cons 'after-middle results))
                                  ;; This throw goes to outer
                                  (throw 'done (list 'from-outer r2))))))
                      (list r1 (nreverse results))))"#;
    let expect = expect_test::expect![
        r#""OK ((from-outer (from-middle inner-val)) (outer-body middle-body inner-body after-inner after-middle))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// catch/throw with complex thrown values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_complex_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Throw various complex data structures and verify they arrive intact.
    let form = r#"(list
                    ;; Throw a hash-table (by serializing it)
                    (catch 'tag
                      (let ((h (make-hash-table :test 'equal)))
                        (puthash "x" 1 h)
                        (puthash "y" 2 h)
                        (throw 'tag (list (gethash "x" h) (gethash "y" h)))))
                    ;; Throw a deeply nested list
                    (catch 'tag
                      (throw 'tag '(1 (2 (3 (4 (5)))))))
                    ;; Throw a vector
                    (catch 'tag
                      (throw 'tag [a b c d e]))
                    ;; Throw a cons of lambda result
                    (catch 'tag
                      (throw 'tag (cons (+ 10 20)
                                        (mapcar #'1+ '(1 2 3)))))
                    ;; Throw nil (valid value)
                    (catch 'tag
                      (throw 'tag nil)
                      'never-reached))"#;
    let expect =
        expect_test::expect![r#""OK ((1 2) (1 (2 (3 (4 (5))))) [a b c d e] (30 2 3 4) nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// catch/throw crossing condition-case boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_crossing_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // throw should pass through condition-case without being caught
    // (condition-case only catches signals, not throws).
    let form = r#"(let ((log nil))
                    (let ((result
                           (catch 'escape
                             (condition-case err
                                 (progn
                                   (setq log (cons 'before-throw log))
                                   (throw 'escape 'thrown-value)
                                   (setq log (cons 'never log)))
                               (error
                                (setq log (cons 'error-handler log))
                                'error-result)))))
                      ;; Also test: signal inside catch is caught by condition-case
                      (let ((result2
                             (catch 'escape
                               (condition-case err
                                   (progn
                                     (setq log (cons 'before-signal log))
                                     (error "test error")
                                     (setq log (cons 'never2 log)))
                                 (error
                                  (setq log (cons 'caught-signal log))
                                  'signal-caught)))))
                        (list result result2 (nreverse log)))))"#;
    let expect = expect_test::expect![
        r#""OK (thrown-value signal-caught (before-throw before-signal caught-signal))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// catch/throw crossing unwind-protect: cleanup always runs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_crossing_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple unwind-protect layers with various cleanup actions.
    // All cleanups must run even though throw bypasses normal flow.
    let form = r#"(let ((resources nil)
                        (cleanups nil))
                    (let ((result
                           (catch 'abort
                             ;; Acquire resource 1
                             (setq resources (cons 'res-1 resources))
                             (unwind-protect
                                 (progn
                                   ;; Acquire resource 2
                                   (setq resources (cons 'res-2 resources))
                                   (unwind-protect
                                       (progn
                                         ;; Acquire resource 3
                                         (setq resources (cons 'res-3 resources))
                                         (unwind-protect
                                             (progn
                                               ;; Do some work then abort
                                               (when (= (length resources) 3)
                                                 (throw 'abort
                                                        (list 'aborted
                                                              (copy-sequence resources))))
                                               'normal-exit)
                                           ;; Cleanup 3
                                           (setq cleanups (cons 'clean-3 cleanups))
                                           (setq resources (delq 'res-3 resources))))
                                     ;; Cleanup 2
                                     (setq cleanups (cons 'clean-2 cleanups))
                                     (setq resources (delq 'res-2 resources))))
                               ;; Cleanup 1
                               (setq cleanups (cons 'clean-1 cleanups))
                               (setq resources (delq 'res-1 resources))))))
                      (list result
                            (nreverse cleanups)
                            resources)))"#;
    let expect = expect_test::expect![
        r#""OK ((aborted (res-3 res-2 res-1)) (clean-3 clean-2 clean-1) nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// catch return value when no throw occurs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_no_throw_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When no throw happens, catch returns the value of its body.
    // Test with various body forms.
    let form = r#"(list
                    ;; Simple expression
                    (catch 'tag (+ 1 2 3))
                    ;; progn: last form value
                    (catch 'tag
                      (progn
                        (+ 1 1)
                        (+ 2 2)
                        (* 3 3)))
                    ;; let body
                    (catch 'tag
                      (let ((x 10) (y 20))
                        (list x y (+ x y))))
                    ;; Nested catch, both without throw
                    (catch 'outer
                      (catch 'inner
                        (list 'no 'throw 'happened)))
                    ;; nil body
                    (catch 'tag)
                    ;; Conditional that doesn't throw
                    (catch 'tag
                      (if (> 3 2)
                          'yes
                        (throw 'tag 'no))))"#;
    let expect = expect_test::expect![r#""OK (6 9 (10 20 30) (no throw happened) nil yes)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: catch/throw-based coroutine simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_coroutine_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a generator-like coroutine using catch/throw.
    // A "range generator" yields values one at a time.
    let form = r#"(let ((gen-state nil)
                        (gen-current 0)
                        (gen-max 5))
                    ;; Collect values by repeatedly "resuming" the generator
                    (let ((collected nil))
                      (catch 'gen-exhausted
                        (dotimes (_ 10)  ; try more than gen-max times
                          (let ((val (catch 'gen-yield
                                       ;; "Resume": produce next value or signal done
                                       (if (< gen-current gen-max)
                                           (let ((v gen-current))
                                             (setq gen-current (1+ gen-current))
                                             (throw 'gen-yield v))
                                         (throw 'gen-exhausted 'done)))))
                            (setq collected (cons val collected)))))
                      (list (nreverse collected)
                            gen-current)))"#;
    let expect = expect_test::expect![r#""OK ((0 1 2 3 4) 5)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: catch/throw-based exception-like error handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_throw_exception_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a mini exception system using catch/throw with typed errors.
    // Supports try/catch-like patterns with multiple exception types.
    let form = r#"(let ((safe-divide
                         (lambda (a b)
                           (if (= b 0)
                               (throw 'exception (list 'division-by-zero a b))
                             (/ a b))))
                        (safe-sqrt
                         (lambda (x)
                           (if (< x 0)
                               (throw 'exception (list 'negative-sqrt x))
                             (sqrt x))))
                        (handle-exception
                         (lambda (exc)
                           (let ((type (car exc)))
                             (cond
                              ((eq type 'division-by-zero)
                               (format "DivError: %d / %d" (nth 1 exc) (nth 2 exc)))
                              ((eq type 'negative-sqrt)
                               (format "SqrtError: sqrt(%d)" (nth 1 exc)))
                              (t (format "Unknown: %S" exc)))))))
                    ;; Run several computations, catching exceptions
                    (list
                     ;; Successful computation
                     (let ((exc (catch 'exception
                                  (list 'ok (funcall safe-divide 10 3)))))
                       (if (and (consp exc) (eq (car exc) 'ok))
                           (cadr exc)
                         (funcall handle-exception exc)))
                     ;; Division by zero
                     (let ((exc (catch 'exception
                                  (list 'ok (funcall safe-divide 10 0)))))
                       (if (and (consp exc) (eq (car exc) 'ok))
                           (cadr exc)
                         (funcall handle-exception exc)))
                     ;; Negative sqrt
                     (let ((exc (catch 'exception
                                  (list 'ok (funcall safe-sqrt -4)))))
                       (if (and (consp exc) (eq (car exc) 'ok))
                           (cadr exc)
                         (funcall handle-exception exc)))
                     ;; Chained: divide then sqrt (first error wins)
                     (let ((exc (catch 'exception
                                  (list 'ok
                                        (funcall safe-sqrt
                                                 (funcall safe-divide 100 4))))))
                       (if (and (consp exc) (eq (car exc) 'ok))
                           (cadr exc)
                         (funcall handle-exception exc)))))"#;
    let expect =
        expect_test::expect![[r#""OK (3 \"DivError: 10 / 0\" \"SqrtError: sqrt(-4)\" 5.0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
