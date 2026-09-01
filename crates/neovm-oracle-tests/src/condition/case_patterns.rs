//! Complex oracle parity tests for condition-case patterns:
//! specific error symbol catching, multiple handler dispatch,
//! :success handlers, nested re-signaling, error during let binding,
//! condition-case + unwind-protect ordering, error-based control flow,
//! and transaction-like rollback patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Catching specific error symbols (wrong-type-argument, void-variable, etc.)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_specific_error_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each error type should be caught by its specific handler
    let form = r#"(list
  ;; wrong-type-argument from (car 1)
  (condition-case err
      (car 1)
    (wrong-type-argument
     (list 'wta (car err) (cadr err))))
  ;; arith-error from division by zero
  (condition-case err
      (/ 1 0)
    (arith-error
     (list 'arith (car err))))
  ;; void-variable from unbound symbol
  (condition-case err
      (symbol-value 'neovm--definitely-unbound-var-xyz)
    (void-variable
     (list 'void (car err) (cadr err))))
  ;; wrong-number-of-arguments from (+ 1 2 3) -- this is fine, try funcall
  (condition-case err
      (funcall (lambda (x) x) 1 2)
    (wrong-number-of-arguments
     (list 'wrong-nargs (car err))))
  ;; void-function
  (condition-case err
      (funcall 'neovm--definitely-unbound-fn-xyz 1)
    (void-function
     (list 'void-fn (car err) (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((wta wrong-type-argument listp) (arith arith-error) (void void-variable neovm--definitely-unbound-var-xyz) (wrong-nargs wrong-number-of-arguments) (void-fn void-function neovm--definitely-unbound-fn-xyz))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple handler clauses with priority dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_multiple_handler_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // First matching handler wins; more specific before generic
    let form = r#"(let ((classify-error
         (lambda (body-fn)
           (condition-case err
               (funcall body-fn)
             (arith-error 'caught-arith)
             (wrong-type-argument 'caught-wta)
             (void-variable 'caught-void-var)
             (void-function 'caught-void-fn)
             (error (list 'caught-generic (car err)))))))
  (list
   (funcall classify-error (lambda () (/ 1 0)))
   (funcall classify-error (lambda () (car 42)))
   (funcall classify-error (lambda () (symbol-value 'neovm--no-such-var-abc)))
   (funcall classify-error (lambda () (funcall 'neovm--no-such-fn-abc)))
   (funcall classify-error (lambda () (signal 'file-error '("not found"))))
   ;; No error case
   (funcall classify-error (lambda () 'all-good))))"#;
    let expect = expect_test::expect![[
        r#""OK (caught-arith caught-wta caught-void-var caught-void-fn (caught-generic file-error) all-good)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested condition-case with error re-signaling (wrapping)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_nested_resignal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner handler wraps the error, outer catches the wrapped version
    let form = r#"(let ((log nil))
  (condition-case outer-err
      (condition-case inner-err
          (condition-case deepest-err
              (/ 1 0)
            (arith-error
             (setq log (cons 'level-3 log))
             (signal 'error (list "wrapped-arith" (cdr deepest-err)))))
        (error
         (setq log (cons 'level-2 log))
         (signal 'error (list "double-wrapped" (cadr inner-err)))))
    (error
     (setq log (cons 'level-1 log))
     (list (nreverse log) (cadr outer-err) (caddr outer-err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((level-3 level-2 level-1) \"double-wrapped\" \"wrapped-arith\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case in let bindings (error during binding evaluation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_error_in_let_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Error during let binding evaluation should be catchable
    let form = r#"(list
  ;; Error in first binding
  (condition-case err
      (let ((x (/ 1 0))
            (y 42))
        (list x y))
    (arith-error 'caught-in-first-binding))
  ;; Error in second binding (first succeeds)
  (condition-case err
      (let* ((x 10)
             (y (/ x 0)))
        (list x y))
    (arith-error (list 'caught-in-second 'x-was x)))
  ;; Error in nested let
  (condition-case err
      (let ((x 1))
        (let ((y 2))
          (let ((z (car (+ x y))))
            z)))
    (wrong-type-argument 'nested-let-error))
  ;; No error in let bindings
  (condition-case err
      (let ((x 10) (y 20))
        (+ x y))
    (error 'should-not-reach)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// condition-case combined with unwind-protect ordering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_with_unwind_protect_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify exact ordering of handler vs cleanup execution
    let form = r#"(let ((log nil))
  ;; Pattern 1: unwind-protect inside condition-case body
  (condition-case err
      (unwind-protect
          (progn
            (setq log (cons 'body-start log))
            (/ 1 0)
            (setq log (cons 'body-end log)))
        (setq log (cons 'cleanup log)))
    (arith-error
     (setq log (cons 'handler log))))
  (let ((pattern1 (nreverse log)))
    (setq log nil)
    ;; Pattern 2: condition-case inside unwind-protect body
    (unwind-protect
        (progn
          (setq log (cons 'outer-body log))
          (condition-case err
              (progn
                (setq log (cons 'inner-body log))
                (/ 1 0))
            (arith-error
             (setq log (cons 'inner-handler log)))))
      (setq log (cons 'outer-cleanup log)))
    (let ((pattern2 (nreverse log)))
      (setq log nil)
      ;; Pattern 3: error in unwind-protect cleanup caught by outer condition-case
      (condition-case err
          (unwind-protect
              (progn
                (setq log (cons 'body log))
                (signal 'error '("body-error")))
            (setq log (cons 'cleanup-start log))
            ;; Cleanup runs even though body errored
            (setq log (cons 'cleanup-end log)))
        (error
         (setq log (cons 'outer-handler log))))
      (list pattern1 pattern2 (nreverse log)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((body-start cleanup handler) (outer-body inner-body inner-handler outer-cleanup) (body cleanup-start cleanup-end outer-handler))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: error-based early return (non-local exit as control flow)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_error_as_early_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use condition-case + signal as a non-local early return mechanism
    // simulating "return" from a deep computation
    let form = r#"(let ((process-items
         (lambda (items)
           (condition-case result
               (let ((acc 0))
                 (dolist (item items)
                   (cond
                    ((not (numberp item))
                     (signal 'error (list 'bad-type item acc)))
                    ((< item 0)
                     (signal 'error (list 'negative item acc)))
                    ((> item 1000)
                     (signal 'error (list 'overflow item acc)))
                    (t (setq acc (+ acc item)))))
                 (list 'success acc))
             (error
              (list 'early-return
                    (nth 1 result)
                    (nth 2 result)
                    (nth 3 result)))))))
  (list
   (funcall process-items '(1 2 3 4 5))
   (funcall process-items '(10 20 "bad" 40))
   (funcall process-items '(1 2 -5 4))
   (funcall process-items '(100 200 9999 1))
   (funcall process-items '())
   (funcall process-items '(500 500 1 500))))"#;
    let expect = expect_test::expect![[
        r#""OK ((success 15) (early-return bad-type \"bad\" 30) (early-return negative -5 3) (early-return overflow 9999 300) (success 0) (success 1501))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: transaction-like pattern with rollback on error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_transaction_rollback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a transaction log: on error, undo all recorded operations
    let form = r#"(let ((db (make-hash-table :test 'equal))
      (run-transaction
       nil))
  ;; Pre-populate the "database"
  (puthash "balance-a" 100 db)
  (puthash "balance-b" 200 db)
  (puthash "balance-c" 50 db)
  (setq run-transaction
    (lambda (operations)
      (let ((undo-log nil))
        (condition-case err
            (progn
              (dolist (op operations)
                (let* ((account (nth 0 op))
                       (delta (nth 1 op))
                       (old-val (gethash account db))
                       (new-val (+ old-val delta)))
                  ;; Record undo entry
                  (setq undo-log (cons (list account old-val) undo-log))
                  ;; Check constraint: no negative balances
                  (when (< new-val 0)
                    (signal 'error (list "insufficient funds" account new-val)))
                  (puthash account new-val db)))
              'committed)
          (error
           ;; Rollback: restore all changed values
           (dolist (undo undo-log)
             (puthash (car undo) (cadr undo) db))
           (list 'rolled-back (cadr err)))))))
  ;; Transaction 1: should succeed (A-=50, B+=50)
  (let ((r1 (funcall run-transaction
                     '(("balance-a" -50) ("balance-b" 50)))))
    ;; Transaction 2: should fail (C-=100 would go negative)
    (let ((r2 (funcall run-transaction
                       '(("balance-c" -100) ("balance-a" 100)))))
      (list r1
            (gethash "balance-a" db)
            (gethash "balance-b" db)
            r2
            ;; C should be unchanged after rollback
            (gethash "balance-c" db)
            ;; A should also be unchanged (rollback undid the +100)
            (gethash "balance-a" db)))))"#;
    let expect = expect_test::expect![[
        r#""OK (committed 50 250 (rolled-back \"insufficient funds\") 50 50)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: condition-case with catch/throw interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ccpat_condition_case_with_catch_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that condition-case and catch/throw interact correctly:
    // throw passes through condition-case, signal passes through catch
    let form = r#"(list
  ;; throw passes through condition-case (not an error)
  (catch 'outer
    (condition-case err
        (throw 'outer 'thrown-through)
      (error 'should-not-catch-throw)))
  ;; signal does NOT get caught by catch
  (condition-case err
      (catch 'outer
        (signal 'error '("passes through catch")))
    (error (list 'caught (cadr err))))
  ;; Nested: throw from within a condition-case handler
  (catch 'escape
    (condition-case err
        (/ 1 0)
      (arith-error
       (throw 'escape (list 'escaped (car err))))))
  ;; Nested: error in catch body caught by condition-case
  (condition-case err
      (catch 'tag
        (/ 1 0))
    (arith-error 'arith-caught-outside-catch)))"#;
    let expect = expect_test::expect![[
        r#""OK (thrown-through (caught \"passes through catch\") (escaped arith-error) arith-caught-outside-catch)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
