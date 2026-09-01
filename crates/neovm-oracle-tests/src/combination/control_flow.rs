//! Oracle parity tests for complex control flow patterns combining
//! catch/throw, condition-case, unwind-protect, recursion, closures,
//! and dynamic binding in non-trivial ways.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. Nested condition-case with different error types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_nested_condition_case_different_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner handler catches arith-error, middle catches void-variable,
    // outer catches generic error. Each logs which handler ran.
    let form = r#"(let ((log nil))
  (let ((try-eval
         (lambda (expr-fn)
           (setq log nil)
           (condition-case outer-err
               (condition-case mid-err
                   (condition-case inner-err
                       (funcall expr-fn)
                     (arith-error
                      (setq log (cons 'inner log))
                      (list 'arith (car inner-err))))
                 (void-variable
                  (setq log (cons 'middle log))
                  (list 'void (car mid-err))))
             (error
              (setq log (cons 'outer log))
              (list 'generic (car outer-err))))
           (cons (nreverse log) nil))))
    (list
      ;; Division by zero -> inner handler
      (funcall try-eval (lambda () (/ 1 0)))
      ;; Void variable -> middle handler (skip inner)
      (funcall try-eval (lambda () (symbol-value 'neovm--unbound-xyz-var)))
      ;; Wrong type -> outer handler (skip inner and middle)
      (funcall try-eval (lambda () (car 42))))))"#;
    let expect = expect_test::expect![[r#""OK (((inner)) ((middle)) ((outer)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. catch/throw for non-local exit from deep recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_catch_throw_deep_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive tree search that throws on finding target value.
    // The tree is nested lists; throw exits immediately from any depth.
    let form = r#"(progn
  (fset 'neovm--test-tree-find
    (lambda (tree target depth)
      (cond
        ((null tree) nil)
        ((and (numberp tree) (= tree target))
         (throw 'found (list 'found-at-depth depth)))
        ((consp tree)
         (funcall 'neovm--test-tree-find (car tree) target (1+ depth))
         (funcall 'neovm--test-tree-find (cdr tree) target (1+ depth))))))
  (unwind-protect
      (let ((tree '(1 (2 (3 (4 (5 (6 (7 (8 9))))))))))
        (list
          ;; Find 9 deep in the tree
          (catch 'found
            (funcall 'neovm--test-tree-find tree 9 0)
            'not-found)
          ;; Find 1 at the top
          (catch 'found
            (funcall 'neovm--test-tree-find tree 1 0)
            'not-found)
          ;; Find something not in tree
          (catch 'found
            (funcall 'neovm--test-tree-find tree 99 0)
            'not-found)))
    (fmakunbound 'neovm--test-tree-find)))"#;
    let expect =
        expect_test::expect![[r#""OK ((found-at-depth 16) (found-at-depth 1) not-found)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. unwind-protect chains ensuring cleanup ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_unwind_protect_cleanup_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three nested unwind-protect forms. An error in the body should
    // trigger cleanup in reverse order (innermost first).
    let form = r#"(let ((cleanup-log nil))
  (condition-case err
      (unwind-protect
          (unwind-protect
              (unwind-protect
                  (progn
                    (setq cleanup-log (cons 'body-start cleanup-log))
                    (error "deliberate failure")
                    (setq cleanup-log (cons 'body-end cleanup-log)))
                ;; Innermost cleanup
                (setq cleanup-log (cons 'cleanup-3 cleanup-log)))
            ;; Middle cleanup
            (setq cleanup-log (cons 'cleanup-2 cleanup-log)))
        ;; Outermost cleanup
        (setq cleanup-log (cons 'cleanup-1 cleanup-log)))
    (error nil))
  ;; Log should show: body-start, then cleanups 3, 2, 1
  (nreverse cleanup-log))"#;
    let expect = expect_test::expect![[r#""OK (body-start cleanup-3 cleanup-2 cleanup-1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. State machine using cond + setq with transition table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_state_machine_transition_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A finite state machine with explicit transition table.
    // States: start, reading, escaped, done.
    // Input: a string with backslash escapes, extract content between quotes.
    let form = r#"(let* ((transitions
          '((start  . ((?\  . start) (?\" . reading)))
            (reading . ((?\" . done) (?\\ . escaped)))
            (escaped . nil)))
        (input "  \"hello\\\"world\" rest")
        (state 'start)
        (collected nil)
        (i 0)
        (len (length input)))
  (while (and (< i len) (not (eq state 'done)))
    (let* ((ch (aref input i))
           (state-trans (cdr (assq state transitions)))
           (next (cdr (assq ch state-trans))))
      (cond
        (next (setq state next))
        ((eq state 'reading)
         (setq collected (cons ch collected)))
        ((eq state 'escaped)
         (setq collected (cons ch collected))
         (setq state 'reading)))
      (setq i (1+ i))))
  (list state
        (apply #'string (nreverse collected))
        i))"#;
    let expect = expect_test::expect![[r#""OK (done \"hello\\\"world\" 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. Coroutine-like pattern using catch/throw for cooperative scheduling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_cooperative_scheduling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate cooperative multitasking: two "tasks" yield control to each
    // other via catch/throw, accumulating results in a shared log.
    let form = r#"(let ((log nil)
       (task-a-state 0)
       (task-b-state 10))
  ;; Run scheduler for N steps
  (let ((steps 0)
        (max-steps 8))
    (while (< steps max-steps)
      (catch 'yield
        ;; Task A: count up, yield every step
        (when (< task-a-state 100)
          (setq task-a-state (1+ task-a-state))
          (setq log (cons (list 'A task-a-state) log))
          (throw 'yield nil)))
      (catch 'yield
        ;; Task B: count down, yield every step
        (when (> task-b-state 0)
          (setq task-b-state (1- task-b-state))
          (setq log (cons (list 'B task-b-state) log))
          (throw 'yield nil)))
      (setq steps (1+ steps))))
  (list (nreverse log)
        task-a-state
        task-b-state))"#;
    let expect = expect_test::expect![[
        r#""OK (((A 1) (B 9) (A 2) (B 8) (A 3) (B 7) (A 4) (B 6) (A 5) (B 5) (A 6) (B 4) (A 7) (B 3) (A 8) (B 2)) 8 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. Error retry pattern with counter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_error_retry_with_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Retry an operation up to N times, succeeding on the Kth attempt.
    // Uses condition-case in a loop with a success flag.
    let form = r#"(progn
  (defvar neovm--test-attempt-count 0)
  (fset 'neovm--test-flaky-op
    (lambda (succeed-on)
      (setq neovm--test-attempt-count (1+ neovm--test-attempt-count))
      (if (>= neovm--test-attempt-count succeed-on)
          (list 'success neovm--test-attempt-count)
        (error "attempt %d failed" neovm--test-attempt-count))))
  (unwind-protect
      (let ((max-retries 5)
            (results nil))
        ;; Test 1: succeeds on attempt 3
        (setq neovm--test-attempt-count 0)
        (let ((retry 0) (done nil) (result nil))
          (while (and (< retry max-retries) (not done))
            (condition-case err
                (progn
                  (setq result (funcall 'neovm--test-flaky-op 3))
                  (setq done t))
              (error
               (setq retry (1+ retry)))))
          (setq results (cons (list done result retry) results)))
        ;; Test 2: never succeeds (exceed retries)
        (setq neovm--test-attempt-count 0)
        (let ((retry 0) (done nil) (result nil))
          (while (and (< retry max-retries) (not done))
            (condition-case err
                (progn
                  (setq result (funcall 'neovm--test-flaky-op 99))
                  (setq done t))
              (error
               (setq retry (1+ retry)))))
          (setq results (cons (list done result retry) results)))
        ;; Test 3: succeeds on first attempt
        (setq neovm--test-attempt-count 0)
        (let ((retry 0) (done nil) (result nil))
          (while (and (< retry max-retries) (not done))
            (condition-case err
                (progn
                  (setq result (funcall 'neovm--test-flaky-op 1))
                  (setq done t))
              (error
               (setq retry (1+ retry)))))
          (setq results (cons (list done result retry) results)))
        (nreverse results))
    (fmakunbound 'neovm--test-flaky-op)
    (makunbound 'neovm--test-attempt-count)))"#;
    let expect =
        expect_test::expect![[r#""OK ((t (success 3) 2) (nil nil 5) (t (success 1) 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 7. prog1 preserving return value across side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_prog1_preserving_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use prog1 to return a value while performing complex side effects:
    // pop from a stack, accumulate into another, transform in-flight.
    let form = r#"(let ((stack '(10 20 30 40 50))
       (popped nil)
       (side-effects nil))
  ;; Pop items one by one, recording side effects
  (dotimes (_ 3)
    (setq popped
          (cons
           (prog1 (car stack)
             (setq side-effects
                   (cons (list 'popped (car stack) 'remaining (length (cdr stack)))
                         side-effects))
             (setq stack (cdr stack)))
           popped)))
  (list
    (nreverse popped)
    stack
    (nreverse side-effects)
    ;; prog1 with multiple side-effect forms
    (let ((x 0))
      (prog1 (setq x 42)
        (setq x (* x 2))
        (setq x (+ x 1))))
    ;; Nested prog1
    (let ((a nil) (b nil))
      (prog1
          (prog1 'inner
            (setq a 'inner-side))
        (setq b 'outer-side))
      (list a b))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30) (40 50) ((popped 10 remaining 4) (popped 20 remaining 3) (popped 30 remaining 2)) 42 (inner-side outer-side))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 8. Exception-safe resource management pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_exception_safe_resource_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a transaction system: begin-transaction, do operations,
    // commit on success, rollback on error. Track all operations.
    let form = r#"(progn
  (defvar neovm--test-txn-log nil)
  (defvar neovm--test-txn-data nil)
  (fset 'neovm--test-with-transaction
    (lambda (body-fn)
      (let ((snapshot (copy-sequence neovm--test-txn-data)))
        (setq neovm--test-txn-log (cons 'begin neovm--test-txn-log))
        (condition-case err
            (prog1
                (unwind-protect
                    (funcall body-fn)
                  ;; Cleanup: nothing to do on success path
                  nil)
              (setq neovm--test-txn-log (cons 'commit neovm--test-txn-log)))
          (error
           ;; Rollback: restore snapshot
           (setq neovm--test-txn-data snapshot)
           (setq neovm--test-txn-log (cons 'rollback neovm--test-txn-log))
           (list 'rolled-back (cadr err)))))))
  (unwind-protect
      (progn
        (setq neovm--test-txn-data (list 0))
        (setq neovm--test-txn-log nil)
        ;; Successful transaction
        (let ((r1 (funcall 'neovm--test-with-transaction
                    (lambda ()
                      (setcar neovm--test-txn-data 42)
                      'ok))))
          ;; Failed transaction
          (let ((r2 (funcall 'neovm--test-with-transaction
                      (lambda ()
                        (setcar neovm--test-txn-data 99)
                        (error "abort!")
                        'never-reached))))
            ;; Nested successful inside failed
            (let ((r3 (funcall 'neovm--test-with-transaction
                        (lambda ()
                          (setcar neovm--test-txn-data 77)
                          ;; Inner successful transaction
                          (let ((inner (funcall 'neovm--test-with-transaction
                                         (lambda ()
                                           (setcar neovm--test-txn-data 88)
                                           'inner-ok))))
                            ;; Then fail the outer
                            (error "outer fails after inner succeeds"))))))
              (list
                r1 r2 r3
                (car neovm--test-txn-data)
                (nreverse neovm--test-txn-log))))))
    (fmakunbound 'neovm--test-with-transaction)
    (makunbound 'neovm--test-txn-log)
    (makunbound 'neovm--test-txn-data)))"#;
    let expect = expect_test::expect![[
        r#""OK (ok (rolled-back \"abort!\") (rolled-back \"outer fails after inner succeeds\") 42 (begin commit begin rollback begin begin commit rollback))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 9. Coroutine pipeline with catch/throw
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_coroutine_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate pipeline stages with catch/throw for early termination
    let form = r#"(let ((log nil))
                    (let ((pipeline
                           (lambda (input stages)
                             (catch 'pipeline-abort
                               (let ((val input))
                                 (dolist (stage stages)
                                   (setq val (funcall stage val))
                                   (setq log (cons val log)))
                                 val)))))
                      (let ((stages
                             (list
                              (lambda (x)
                                (if (numberp x) (* x 2)
                                  (throw 'pipeline-abort
                                         (list 'error "not a number"))))
                              (lambda (x)
                                (if (> x 100)
                                    (throw 'pipeline-abort
                                           (list 'overflow x))
                                  (+ x 10)))
                              (lambda (x) (* x 3)))))
                        ;; Normal flow
                        (let ((r1 (funcall pipeline 5 stages))
                              (_ (setq log nil))
                              ;; Overflow abort
                              (r2 (funcall pipeline 60 stages))
                              (_ (setq log nil))
                              ;; Type error abort
                              (r3 (funcall pipeline "bad" stages)))
                          (list r1 r2 r3)))))"#;
    let expect = expect_test::expect![[r#""OK (60 (overflow 120) (error \"not a number\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 10. Trampoline pattern for tail-call optimization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_trampoline_mutual_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trampoline: bounce thunks until we get a non-function result.
    // Used for mutual recursion (even/odd) without stack overflow.
    let form = r#"(progn
  (fset 'neovm--test-trampoline
    (lambda (thunk)
      (let ((result thunk))
        (while (functionp result)
          (setq result (funcall result)))
        result)))
  (fset 'neovm--test-collatz-bounce
    (lambda (n steps)
      (cond
        ((= n 1) (list 'done steps))
        ((= 0 (% n 2))
         (lambda () (funcall 'neovm--test-collatz-bounce (/ n 2) (1+ steps))))
        (t
         (lambda () (funcall 'neovm--test-collatz-bounce (+ (* 3 n) 1) (1+ steps)))))))
  (unwind-protect
      (list
        (funcall 'neovm--test-trampoline
                 (funcall 'neovm--test-collatz-bounce 1 0))
        (funcall 'neovm--test-trampoline
                 (funcall 'neovm--test-collatz-bounce 6 0))
        (funcall 'neovm--test-trampoline
                 (funcall 'neovm--test-collatz-bounce 27 0))
        (funcall 'neovm--test-trampoline
                 (funcall 'neovm--test-collatz-bounce 12 0)))
    (fmakunbound 'neovm--test-trampoline)
    (fmakunbound 'neovm--test-collatz-bounce)))"#;
    let expect = expect_test::expect![[r#""OK ((done 0) (done 8) (done 111) (done 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 11. CPS factorial with dynamic binding context
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cf_cps_with_dynamic_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS factorial that also tracks call depth via dynamic binding,
    // returning both the result and the max depth reached.
    let form = r#"(progn
  (defvar neovm--test-max-depth 0)
  (fset 'neovm--test-cps-fact-tracked
    (lambda (n depth k)
      (when (> depth neovm--test-max-depth)
        (setq neovm--test-max-depth depth))
      (if (= n 0)
          (funcall k 1)
        (funcall 'neovm--test-cps-fact-tracked
                 (1- n)
                 (1+ depth)
                 (lambda (result)
                   (funcall k (* n result)))))))
  (unwind-protect
      (let ((results nil))
        (dolist (n '(0 1 5 8))
          (setq neovm--test-max-depth 0)
          (let ((factorial (funcall 'neovm--test-cps-fact-tracked
                                     n 0 #'identity)))
            (setq results (cons (list n factorial neovm--test-max-depth)
                                results))))
        (nreverse results))
    (fmakunbound 'neovm--test-cps-fact-tracked)
    (makunbound 'neovm--test-max-depth)))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 0) (1 1 1) (5 120 5) (8 40320 8))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
