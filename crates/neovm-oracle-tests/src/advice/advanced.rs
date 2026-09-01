//! Advanced oracle parity tests for the advice system: multiple advisors,
//! ordering, :around with funcall, memoization, and error handling chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Multiple advisors on the same function (LIFO ordering)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_multiple_before_advisors_lifo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two :before advisors added in sequence; last added runs first (LIFO)
    let form = r#"(progn
      (defvar neovm--adv-log nil)
      (fset 'neovm--adv-target
        (lambda (x) (setq neovm--adv-log (cons (list 'orig x) neovm--adv-log)) x))
      (fset 'neovm--adv-b1
        (lambda (&rest args) (setq neovm--adv-log (cons (cons 'before-1 args) neovm--adv-log))))
      (fset 'neovm--adv-b2
        (lambda (&rest args) (setq neovm--adv-log (cons (cons 'before-2 args) neovm--adv-log))))
      (unwind-protect
          (progn
            (advice-add 'neovm--adv-target :before 'neovm--adv-b1)
            (advice-add 'neovm--adv-target :before 'neovm--adv-b2)
            (funcall 'neovm--adv-target 42)
            (nreverse neovm--adv-log))
        (advice-remove 'neovm--adv-target 'neovm--adv-b1)
        (advice-remove 'neovm--adv-target 'neovm--adv-b2)
        (fmakunbound 'neovm--adv-target)
        (fmakunbound 'neovm--adv-b1)
        (fmakunbound 'neovm--adv-b2)
        (makunbound 'neovm--adv-log)))"#;
    let expect = expect_test::expect![r#""OK ((before-2 42) (before-1 42) (orig 42))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :before + :after + :around combined
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_before_after_around_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (defvar neovm--adv-log2 nil)
      (fset 'neovm--adv-tgt2
        (lambda (x)
          (setq neovm--adv-log2 (cons (list 'orig x) neovm--adv-log2))
          (* x 2)))
      (fset 'neovm--adv-before2
        (lambda (&rest args)
          (setq neovm--adv-log2 (cons (cons 'before args) neovm--adv-log2))))
      (fset 'neovm--adv-after2
        (lambda (&rest args)
          (setq neovm--adv-log2 (cons (cons 'after args) neovm--adv-log2))))
      (fset 'neovm--adv-around2
        (lambda (orig-fn &rest args)
          (setq neovm--adv-log2 (cons 'around-enter neovm--adv-log2))
          (let ((result (apply orig-fn args)))
            (setq neovm--adv-log2 (cons (list 'around-exit result) neovm--adv-log2))
            (+ result 100))))
      (unwind-protect
          (progn
            (advice-add 'neovm--adv-tgt2 :before 'neovm--adv-before2)
            (advice-add 'neovm--adv-tgt2 :after 'neovm--adv-after2)
            (advice-add 'neovm--adv-tgt2 :around 'neovm--adv-around2)
            (let ((result (funcall 'neovm--adv-tgt2 5)))
              (list result (nreverse neovm--adv-log2))))
        (advice-remove 'neovm--adv-tgt2 'neovm--adv-before2)
        (advice-remove 'neovm--adv-tgt2 'neovm--adv-after2)
        (advice-remove 'neovm--adv-tgt2 'neovm--adv-around2)
        (fmakunbound 'neovm--adv-tgt2)
        (fmakunbound 'neovm--adv-before2)
        (fmakunbound 'neovm--adv-after2)
        (fmakunbound 'neovm--adv-around2)
        (makunbound 'neovm--adv-log2)))"#;
    let expect = expect_test::expect![
        r#""OK (110 (around-enter (before 5) (orig 5) (after 5) (around-exit 10)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// advice-remove and advice-member-p lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_remove_and_member_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (fset 'neovm--adv-tgt3 (lambda (x) x))
      (fset 'neovm--adv-fn3a (lambda (&rest _) nil))
      (fset 'neovm--adv-fn3b (lambda (&rest _) nil))
      (unwind-protect
          (let (results)
            ;; Initially no advice
            (setq results (cons (not (null (advice-member-p 'neovm--adv-fn3a 'neovm--adv-tgt3))) results))
            ;; Add two advisors
            (advice-add 'neovm--adv-tgt3 :before 'neovm--adv-fn3a)
            (advice-add 'neovm--adv-tgt3 :after 'neovm--adv-fn3b)
            (setq results (cons (not (null (advice-member-p 'neovm--adv-fn3a 'neovm--adv-tgt3))) results))
            (setq results (cons (not (null (advice-member-p 'neovm--adv-fn3b 'neovm--adv-tgt3))) results))
            ;; Remove first
            (advice-remove 'neovm--adv-tgt3 'neovm--adv-fn3a)
            (setq results (cons (not (null (advice-member-p 'neovm--adv-fn3a 'neovm--adv-tgt3))) results))
            (setq results (cons (not (null (advice-member-p 'neovm--adv-fn3b 'neovm--adv-tgt3))) results))
            ;; Remove second
            (advice-remove 'neovm--adv-tgt3 'neovm--adv-fn3b)
            (setq results (cons (not (null (advice-member-p 'neovm--adv-fn3b 'neovm--adv-tgt3))) results))
            (nreverse results))
        (fmakunbound 'neovm--adv-tgt3)
        (fmakunbound 'neovm--adv-fn3a)
        (fmakunbound 'neovm--adv-fn3b)))"#;
    let expect = expect_test::expect![r#""OK (nil t t nil t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :around advice with funcall to original (pass-through, transform, skip)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_around_funcall_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (fset 'neovm--adv-tgt4 (lambda (x) (* x 3)))
      ;; Pass-through: calls original unchanged
      (fset 'neovm--adv-passthru
        (lambda (orig-fn &rest args) (apply orig-fn args)))
      ;; Transform: modifies args before calling original
      (fset 'neovm--adv-xform
        (lambda (orig-fn x) (funcall orig-fn (+ x 10))))
      ;; Skip: never calls original
      (fset 'neovm--adv-skip
        (lambda (orig-fn &rest _args) 'skipped))
      (unwind-protect
          (list
           ;; pass-through: 7 * 3 = 21
           (progn
             (advice-add 'neovm--adv-tgt4 :around 'neovm--adv-passthru)
             (prog1 (funcall 'neovm--adv-tgt4 7)
               (advice-remove 'neovm--adv-tgt4 'neovm--adv-passthru)))
           ;; transform: (7+10) * 3 = 51
           (progn
             (advice-add 'neovm--adv-tgt4 :around 'neovm--adv-xform)
             (prog1 (funcall 'neovm--adv-tgt4 7)
               (advice-remove 'neovm--adv-tgt4 'neovm--adv-xform)))
           ;; skip: returns 'skipped
           (progn
             (advice-add 'neovm--adv-tgt4 :around 'neovm--adv-skip)
             (prog1 (funcall 'neovm--adv-tgt4 7)
               (advice-remove 'neovm--adv-tgt4 'neovm--adv-skip)))
           ;; bare: 7 * 3 = 21
           (funcall 'neovm--adv-tgt4 7))
        (fmakunbound 'neovm--adv-tgt4)
        (fmakunbound 'neovm--adv-passthru)
        (fmakunbound 'neovm--adv-xform)
        (fmakunbound 'neovm--adv-skip)))"#;
    let expect = expect_test::expect![r#""OK (21 51 skipped 21)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :override completely replaces the function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_override_replaces_then_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (fset 'neovm--adv-tgt5 (lambda (x) (+ x 1)))
      (fset 'neovm--adv-override5 (lambda (&rest _) 'overridden))
      (unwind-protect
          (let (r1 r2 r3)
            (setq r1 (funcall 'neovm--adv-tgt5 10))
            (advice-add 'neovm--adv-tgt5 :override 'neovm--adv-override5)
            (setq r2 (funcall 'neovm--adv-tgt5 10))
            (advice-remove 'neovm--adv-tgt5 'neovm--adv-override5)
            (setq r3 (funcall 'neovm--adv-tgt5 10))
            (list r1 r2 r3))
        (fmakunbound 'neovm--adv-tgt5)
        (fmakunbound 'neovm--adv-override5)))"#;
    let expect = expect_test::expect![r#""OK (11 overridden 11)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: advice-based logging/tracing system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_logging_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (defvar neovm--adv-trace nil)
      (fset 'neovm--adv-add
        (lambda (a b) (+ a b)))
      (fset 'neovm--adv-mul
        (lambda (a b) (* a b)))
      ;; Generic tracing :around that logs function name, args, result
      (fset 'neovm--adv-tracer-add
        (lambda (orig-fn &rest args)
          (setq neovm--adv-trace (cons (list 'call 'add args) neovm--adv-trace))
          (let ((result (apply orig-fn args)))
            (setq neovm--adv-trace (cons (list 'return 'add result) neovm--adv-trace))
            result)))
      (fset 'neovm--adv-tracer-mul
        (lambda (orig-fn &rest args)
          (setq neovm--adv-trace (cons (list 'call 'mul args) neovm--adv-trace))
          (let ((result (apply orig-fn args)))
            (setq neovm--adv-trace (cons (list 'return 'mul result) neovm--adv-trace))
            result)))
      (unwind-protect
          (progn
            (advice-add 'neovm--adv-add :around 'neovm--adv-tracer-add)
            (advice-add 'neovm--adv-mul :around 'neovm--adv-tracer-mul)
            ;; Compute (2+3) * (4+5) = 5 * 9 = 45
            (let ((result (funcall 'neovm--adv-mul
                                   (funcall 'neovm--adv-add 2 3)
                                   (funcall 'neovm--adv-add 4 5))))
              (list result (nreverse neovm--adv-trace))))
        (advice-remove 'neovm--adv-add 'neovm--adv-tracer-add)
        (advice-remove 'neovm--adv-mul 'neovm--adv-tracer-mul)
        (fmakunbound 'neovm--adv-add)
        (fmakunbound 'neovm--adv-mul)
        (fmakunbound 'neovm--adv-tracer-add)
        (fmakunbound 'neovm--adv-tracer-mul)
        (makunbound 'neovm--adv-trace)))"#;
    let expect = expect_test::expect![
        r#""OK (45 ((call add (2 3)) (return add 5) (call add (4 5)) (return add 9) (call mul (5 9)) (return mul 45)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: advice-based memoization wrapper
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_memoization_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (defvar neovm--adv-memo-cache (make-hash-table :test 'equal))
      (defvar neovm--adv-memo-call-count 0)
      (fset 'neovm--adv-slow-fn
        (lambda (n)
          (setq neovm--adv-memo-call-count (1+ neovm--adv-memo-call-count))
          (if (<= n 1) n
            (+ (funcall 'neovm--adv-slow-fn (- n 1))
               (funcall 'neovm--adv-slow-fn (- n 2))))))
      ;; Memoizing :around advice
      (fset 'neovm--adv-memoize
        (lambda (orig-fn &rest args)
          (let ((cached (gethash args neovm--adv-memo-cache 'miss)))
            (if (not (eq cached 'miss))
                cached
              (let ((result (apply orig-fn args)))
                (puthash args result neovm--adv-memo-cache)
                result)))))
      (unwind-protect
          (progn
            (advice-add 'neovm--adv-slow-fn :around 'neovm--adv-memoize)
            (let ((fib-10 (funcall 'neovm--adv-slow-fn 10))
                  (calls-after-first neovm--adv-memo-call-count))
              ;; Call again; should be fully cached, no new calls
              (funcall 'neovm--adv-slow-fn 10)
              (list fib-10
                    calls-after-first
                    ;; call count shouldn't increase
                    (= neovm--adv-memo-call-count calls-after-first))))
        (advice-remove 'neovm--adv-slow-fn 'neovm--adv-memoize)
        (fmakunbound 'neovm--adv-slow-fn)
        (fmakunbound 'neovm--adv-memoize)
        (makunbound 'neovm--adv-memo-cache)
        (makunbound 'neovm--adv-memo-call-count)))"#;
    let expect = expect_test::expect![r#""OK (55 11 t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: advice chain with error handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_advanced_error_handling_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (defvar neovm--adv-err-log nil)
      (fset 'neovm--adv-divide
        (lambda (a b) (/ a b)))
      ;; :around that catches errors and returns a default
      (fset 'neovm--adv-safe-divide
        (lambda (orig-fn &rest args)
          (condition-case err
              (apply orig-fn args)
            (arith-error
             (setq neovm--adv-err-log
                   (cons (list 'caught (car err) args) neovm--adv-err-log))
             'division-error))))
      ;; :before that logs the call
      (fset 'neovm--adv-log-divide
        (lambda (&rest args)
          (setq neovm--adv-err-log
                (cons (list 'attempting args) neovm--adv-err-log))))
      (unwind-protect
          (progn
            (advice-add 'neovm--adv-divide :around 'neovm--adv-safe-divide)
            (advice-add 'neovm--adv-divide :before 'neovm--adv-log-divide)
            (let ((r1 (funcall 'neovm--adv-divide 10 2))
                  (r2 (funcall 'neovm--adv-divide 7 0))
                  (r3 (funcall 'neovm--adv-divide 20 4)))
              (list r1 r2 r3 (nreverse neovm--adv-err-log))))
        (advice-remove 'neovm--adv-divide 'neovm--adv-safe-divide)
        (advice-remove 'neovm--adv-divide 'neovm--adv-log-divide)
        (fmakunbound 'neovm--adv-divide)
        (fmakunbound 'neovm--adv-safe-divide)
        (fmakunbound 'neovm--adv-log-divide)
        (makunbound 'neovm--adv-err-log)))"#;
    let expect = expect_test::expect![
        r#""OK (5 division-error 5 ((attempting (10 2)) (attempting (7 0)) (caught arith-error (7 0)) (attempting (20 4))))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
