//! Advanced oracle parity tests for advice patterns:
//! :before, :after, :around, :filter-args, :filter-return, :override
//! combinators. Multiple advices on the same function, advice ordering,
//! removing specific advice, advice on advised functions, advice with
//! stateful closures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// :filter-args and :filter-return advice combinators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_patterns_filter_args_and_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :filter-args receives (args-list) and must return modified (args-list).
    // :filter-return receives the return value and must return modified value.
    let form = r#"(progn
  (fset 'neovm--ap-add (lambda (a b) (+ a b)))

  ;; :filter-args: double each argument before passing to function
  (fset 'neovm--ap-double-args
    (lambda (args)
      (mapcar (lambda (x) (* x 2)) args)))

  ;; :filter-return: square the return value
  (fset 'neovm--ap-square-return
    (lambda (result) (* result result)))

  (unwind-protect
      (list
        ;; Bare call: 3 + 5 = 8
        (funcall 'neovm--ap-add 3 5)
        ;; With :filter-args: (6 + 10) = 16
        (progn
          (advice-add 'neovm--ap-add :filter-args 'neovm--ap-double-args)
          (prog1 (funcall 'neovm--ap-add 3 5)
            (advice-remove 'neovm--ap-add 'neovm--ap-double-args)))
        ;; With :filter-return: (3 + 5)^2 = 64
        (progn
          (advice-add 'neovm--ap-add :filter-return 'neovm--ap-square-return)
          (prog1 (funcall 'neovm--ap-add 3 5)
            (advice-remove 'neovm--ap-add 'neovm--ap-square-return)))
        ;; Both :filter-args AND :filter-return:
        ;; args doubled (6, 10) -> add -> 16 -> squared -> 256
        (progn
          (advice-add 'neovm--ap-add :filter-args 'neovm--ap-double-args)
          (advice-add 'neovm--ap-add :filter-return 'neovm--ap-square-return)
          (prog1 (funcall 'neovm--ap-add 3 5)
            (advice-remove 'neovm--ap-add 'neovm--ap-double-args)
            (advice-remove 'neovm--ap-add 'neovm--ap-square-return))))
    (fmakunbound 'neovm--ap-add)
    (fmakunbound 'neovm--ap-double-args)
    (fmakunbound 'neovm--ap-square-return)))"#;
    let expect = expect_test::expect![r#""OK (8 16 64 256)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple :around advisors - nesting order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_patterns_multiple_around_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When multiple :around advisors are stacked, the last-added
    // wraps outermost. Each :around receives the next in chain as orig-fn.
    let form = r#"(progn
  (defvar neovm--ap-order nil)
  (fset 'neovm--ap-base (lambda (x) (* x 10)))

  ;; :around A: log "A-enter", call, log "A-exit", add 1
  (fset 'neovm--ap-around-a
    (lambda (orig-fn &rest args)
      (setq neovm--ap-order (cons 'A-enter neovm--ap-order))
      (let ((result (apply orig-fn args)))
        (setq neovm--ap-order (cons 'A-exit neovm--ap-order))
        (+ result 1))))

  ;; :around B: log "B-enter", call, log "B-exit", add 100
  (fset 'neovm--ap-around-b
    (lambda (orig-fn &rest args)
      (setq neovm--ap-order (cons 'B-enter neovm--ap-order))
      (let ((result (apply orig-fn args)))
        (setq neovm--ap-order (cons 'B-exit neovm--ap-order))
        (+ result 100))))

  ;; :around C: log "C-enter", call, log "C-exit", add 1000
  (fset 'neovm--ap-around-c
    (lambda (orig-fn &rest args)
      (setq neovm--ap-order (cons 'C-enter neovm--ap-order))
      (let ((result (apply orig-fn args)))
        (setq neovm--ap-order (cons 'C-exit neovm--ap-order))
        (+ result 1000))))

  (unwind-protect
      (progn
        (advice-add 'neovm--ap-base :around 'neovm--ap-around-a)
        (advice-add 'neovm--ap-base :around 'neovm--ap-around-b)
        (advice-add 'neovm--ap-base :around 'neovm--ap-around-c)
        ;; Call: C wraps B wraps A wraps base
        ;; base(5) = 50, A adds 1 = 51, B adds 100 = 151, C adds 1000 = 1151
        (let ((result (funcall 'neovm--ap-base 5)))
          (list result (nreverse neovm--ap-order))))
    (advice-remove 'neovm--ap-base 'neovm--ap-around-a)
    (advice-remove 'neovm--ap-base 'neovm--ap-around-b)
    (advice-remove 'neovm--ap-base 'neovm--ap-around-c)
    (fmakunbound 'neovm--ap-base)
    (fmakunbound 'neovm--ap-around-a)
    (fmakunbound 'neovm--ap-around-b)
    (fmakunbound 'neovm--ap-around-c)
    (makunbound 'neovm--ap-order)))"#;
    let expect =
        expect_test::expect![r#""OK (1151 (C-enter B-enter A-enter A-exit B-exit C-exit))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Removing specific advice from a multi-advice stack
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_patterns_selective_removal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Add 3 advisors, remove the middle one, verify remaining behavior.
    let form = r#"(progn
  (defvar neovm--ap-sel-log nil)
  (fset 'neovm--ap-sel-fn (lambda (x) x))

  (fset 'neovm--ap-sel-b1
    (lambda (&rest args)
      (setq neovm--ap-sel-log (cons 'before-1 neovm--ap-sel-log))))
  (fset 'neovm--ap-sel-b2
    (lambda (&rest args)
      (setq neovm--ap-sel-log (cons 'before-2 neovm--ap-sel-log))))
  (fset 'neovm--ap-sel-b3
    (lambda (&rest args)
      (setq neovm--ap-sel-log (cons 'before-3 neovm--ap-sel-log))))

  (unwind-protect
      (progn
        (advice-add 'neovm--ap-sel-fn :before 'neovm--ap-sel-b1)
        (advice-add 'neovm--ap-sel-fn :before 'neovm--ap-sel-b2)
        (advice-add 'neovm--ap-sel-fn :before 'neovm--ap-sel-b3)

        ;; Call with all 3 advisors
        (setq neovm--ap-sel-log nil)
        (funcall 'neovm--ap-sel-fn 'test)
        (let ((log-all (nreverse neovm--ap-sel-log)))

          ;; Remove middle advisor (b2)
          (advice-remove 'neovm--ap-sel-fn 'neovm--ap-sel-b2)
          (setq neovm--ap-sel-log nil)
          (funcall 'neovm--ap-sel-fn 'test)
          (let ((log-no-b2 (nreverse neovm--ap-sel-log)))

            ;; Verify b2 is gone
            (list
              log-all
              log-no-b2
              (not (null (advice-member-p 'neovm--ap-sel-b1 'neovm--ap-sel-fn)))
              (not (null (advice-member-p 'neovm--ap-sel-b2 'neovm--ap-sel-fn)))
              (not (null (advice-member-p 'neovm--ap-sel-b3 'neovm--ap-sel-fn)))))))
    (advice-remove 'neovm--ap-sel-fn 'neovm--ap-sel-b1)
    (advice-remove 'neovm--ap-sel-fn 'neovm--ap-sel-b3)
    (fmakunbound 'neovm--ap-sel-fn)
    (fmakunbound 'neovm--ap-sel-b1)
    (fmakunbound 'neovm--ap-sel-b2)
    (fmakunbound 'neovm--ap-sel-b3)
    (makunbound 'neovm--ap-sel-log)))"#;
    let expect =
        expect_test::expect![r#""OK ((before-3 before-2 before-1) (before-3 before-1) t nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Advice with stateful closures (counters, accumulators)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_patterns_stateful_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use lexical closures to maintain per-advice state:
    // a call counter and a result accumulator.
    let form = r#"(progn
  (fset 'neovm--ap-st-fn (lambda (x) (* x x)))

  ;; Create a closure-based call counter advice
  (let ((call-count 0))
    (fset 'neovm--ap-st-counter
      (lambda (&rest _args)
        (setq call-count (1+ call-count))))
    (fset 'neovm--ap-st-get-count
      (lambda () call-count)))

  ;; Create a closure-based result accumulator
  (let ((results nil))
    (fset 'neovm--ap-st-accum
      (lambda (orig-fn &rest args)
        (let ((result (apply orig-fn args)))
          (setq results (cons result results))
          result)))
    (fset 'neovm--ap-st-get-results
      (lambda () (nreverse results))))

  (unwind-protect
      (progn
        (advice-add 'neovm--ap-st-fn :before 'neovm--ap-st-counter)
        (advice-add 'neovm--ap-st-fn :around 'neovm--ap-st-accum)

        ;; Make several calls
        (let ((r1 (funcall 'neovm--ap-st-fn 3))
              (r2 (funcall 'neovm--ap-st-fn 5))
              (r3 (funcall 'neovm--ap-st-fn 7))
              (r4 (funcall 'neovm--ap-st-fn 2)))
          (list
            ;; Individual results
            (list r1 r2 r3 r4)
            ;; Call count
            (funcall 'neovm--ap-st-get-count)
            ;; Accumulated results
            (funcall 'neovm--ap-st-get-results))))
    (advice-remove 'neovm--ap-st-fn 'neovm--ap-st-counter)
    (advice-remove 'neovm--ap-st-fn 'neovm--ap-st-accum)
    (fmakunbound 'neovm--ap-st-fn)
    (fmakunbound 'neovm--ap-st-counter)
    (fmakunbound 'neovm--ap-st-get-count)
    (fmakunbound 'neovm--ap-st-accum)
    (fmakunbound 'neovm--ap-st-get-results)))"#;
    let expect = expect_test::expect![r#""OK ((9 25 49 4) 4 (9 25 49 4))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :override with conditional delegation back to original
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_patterns_override_conditional() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :override replaces the function entirely.
    // Unlike :around, the original function is NOT passed.
    // We use :around to conditionally delegate or override.
    let form = r#"(progn
  (fset 'neovm--ap-ov-fn (lambda (x) (* x 3)))

  ;; :override that completely replaces
  (fset 'neovm--ap-ov-replace (lambda (&rest _args) 'replaced))

  ;; :around that conditionally overrides: negative args get special treatment
  (fset 'neovm--ap-ov-conditional
    (lambda (orig-fn x)
      (if (< x 0)
          (list 'negative x)
        (funcall orig-fn x))))

  (unwind-protect
      (list
        ;; Bare
        (funcall 'neovm--ap-ov-fn 4)
        ;; :override completely replaces
        (progn
          (advice-add 'neovm--ap-ov-fn :override 'neovm--ap-ov-replace)
          (let ((r (funcall 'neovm--ap-ov-fn 4)))
            (advice-remove 'neovm--ap-ov-fn 'neovm--ap-ov-replace)
            r))
        ;; :around with conditional delegation
        (progn
          (advice-add 'neovm--ap-ov-fn :around 'neovm--ap-ov-conditional)
          (let ((r1 (funcall 'neovm--ap-ov-fn 4))
                (r2 (funcall 'neovm--ap-ov-fn -3)))
            (advice-remove 'neovm--ap-ov-fn 'neovm--ap-ov-conditional)
            (list r1 r2)))
        ;; Bare again after removal
        (funcall 'neovm--ap-ov-fn 4))
    (fmakunbound 'neovm--ap-ov-fn)
    (fmakunbound 'neovm--ap-ov-replace)
    (fmakunbound 'neovm--ap-ov-conditional)))"#;
    let expect = expect_test::expect![r#""OK (12 replaced (12 (negative -3)) 12)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: advice-based retry/backoff wrapper
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_patterns_retry_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a retry wrapper using :around advice.
    // A flaky function fails N times then succeeds. The advice retries up to
    // a max number of attempts, logging each attempt.
    let form = r#"(progn
  (defvar neovm--ap-retry-fail-count 0)
  (defvar neovm--ap-retry-log nil)

  ;; "Flaky" function: fails first N times, then succeeds
  (fset 'neovm--ap-retry-flaky
    (lambda (fail-times value)
      (if (< neovm--ap-retry-fail-count fail-times)
          (progn
            (setq neovm--ap-retry-fail-count (1+ neovm--ap-retry-fail-count))
            (signal 'error (list "transient failure")))
        value)))

  ;; Retry :around advice: catches errors and retries
  (fset 'neovm--ap-retry-advice
    (lambda (orig-fn &rest args)
      (let ((max-retries 5)
            (attempt 0)
            (result nil)
            (succeeded nil))
        (while (and (not succeeded) (< attempt max-retries))
          (setq attempt (1+ attempt))
          (setq neovm--ap-retry-log
                (cons (list 'attempt attempt) neovm--ap-retry-log))
          (condition-case err
              (progn
                (setq result (apply orig-fn args))
                (setq succeeded t))
            (error
              (setq neovm--ap-retry-log
                    (cons (list 'failed attempt (cadr err))
                          neovm--ap-retry-log)))))
        (if succeeded
            result
          'all-retries-exhausted))))

  (unwind-protect
      (progn
        (advice-add 'neovm--ap-retry-flaky :around 'neovm--ap-retry-advice)

        ;; Case 1: Fails 2 times, succeeds on 3rd
        (setq neovm--ap-retry-fail-count 0)
        (setq neovm--ap-retry-log nil)
        (let ((r1 (funcall 'neovm--ap-retry-flaky 2 'success-val))
              (log1 (nreverse neovm--ap-retry-log)))

          ;; Case 2: Never fails
          (setq neovm--ap-retry-fail-count 0)
          (setq neovm--ap-retry-log nil)
          (let ((r2 (funcall 'neovm--ap-retry-flaky 0 'instant))
                (log2 (nreverse neovm--ap-retry-log)))

            ;; Case 3: Fails more than max-retries
            (setq neovm--ap-retry-fail-count 0)
            (setq neovm--ap-retry-log nil)
            (let ((r3 (funcall 'neovm--ap-retry-flaky 10 'never-reached))
                  (log3 (nreverse neovm--ap-retry-log)))

              (list
                (list r1 (length log1))
                (list r2 (length log2))
                (list r3 (length log3)))))))
    (advice-remove 'neovm--ap-retry-flaky 'neovm--ap-retry-advice)
    (fmakunbound 'neovm--ap-retry-flaky)
    (fmakunbound 'neovm--ap-retry-advice)
    (makunbound 'neovm--ap-retry-fail-count)
    (makunbound 'neovm--ap-retry-log)))"#;
    let expect =
        expect_test::expect![r#""OK ((success-val 5) (instant 1) (all-retries-exhausted 10))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: advice-based access control / permission system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_patterns_access_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use :around advice to implement an access control layer.
    // Functions are guarded by permission checks; different "users"
    // have different permissions.
    let form = r#"(progn
  (defvar neovm--ap-ac-current-user nil)
  (defvar neovm--ap-ac-permissions
    '((admin . (read write delete))
      (editor . (read write))
      (viewer . (read))))

  (fset 'neovm--ap-ac-read-data (lambda () '(data "secret-info")))
  (fset 'neovm--ap-ac-write-data (lambda (val) (list 'written val)))
  (fset 'neovm--ap-ac-delete-data (lambda () 'deleted))

  ;; Generic permission-checking :around advice factory
  (fset 'neovm--ap-ac-make-guard
    (lambda (required-perm)
      (lambda (orig-fn &rest args)
        (let* ((user neovm--ap-ac-current-user)
               (perms (cdr (assq user neovm--ap-ac-permissions))))
          (if (memq required-perm perms)
              (apply orig-fn args)
            (list 'access-denied user required-perm))))))

  (unwind-protect
      (let ((read-guard (funcall 'neovm--ap-ac-make-guard 'read))
            (write-guard (funcall 'neovm--ap-ac-make-guard 'write))
            (delete-guard (funcall 'neovm--ap-ac-make-guard 'delete)))
        ;; Install guards as named advice (using lambdas stored in symbols)
        (fset 'neovm--ap-ac-rg read-guard)
        (fset 'neovm--ap-ac-wg write-guard)
        (fset 'neovm--ap-ac-dg delete-guard)
        (advice-add 'neovm--ap-ac-read-data :around 'neovm--ap-ac-rg)
        (advice-add 'neovm--ap-ac-write-data :around 'neovm--ap-ac-wg)
        (advice-add 'neovm--ap-ac-delete-data :around 'neovm--ap-ac-dg)

        ;; Admin: can do everything
        (setq neovm--ap-ac-current-user 'admin)
        (let ((admin-results
               (list (funcall 'neovm--ap-ac-read-data)
                     (funcall 'neovm--ap-ac-write-data 'test-val)
                     (funcall 'neovm--ap-ac-delete-data))))

          ;; Editor: can read + write, not delete
          (setq neovm--ap-ac-current-user 'editor)
          (let ((editor-results
                 (list (funcall 'neovm--ap-ac-read-data)
                       (funcall 'neovm--ap-ac-write-data 'edit-val)
                       (funcall 'neovm--ap-ac-delete-data))))

            ;; Viewer: can only read
            (setq neovm--ap-ac-current-user 'viewer)
            (let ((viewer-results
                   (list (funcall 'neovm--ap-ac-read-data)
                         (funcall 'neovm--ap-ac-write-data 'no-write)
                         (funcall 'neovm--ap-ac-delete-data))))

              (list admin-results editor-results viewer-results)))))
    (advice-remove 'neovm--ap-ac-read-data 'neovm--ap-ac-rg)
    (advice-remove 'neovm--ap-ac-write-data 'neovm--ap-ac-wg)
    (advice-remove 'neovm--ap-ac-delete-data 'neovm--ap-ac-dg)
    (fmakunbound 'neovm--ap-ac-read-data)
    (fmakunbound 'neovm--ap-ac-write-data)
    (fmakunbound 'neovm--ap-ac-delete-data)
    (fmakunbound 'neovm--ap-ac-make-guard)
    (fmakunbound 'neovm--ap-ac-rg)
    (fmakunbound 'neovm--ap-ac-wg)
    (fmakunbound 'neovm--ap-ac-dg)
    (makunbound 'neovm--ap-ac-current-user)
    (makunbound 'neovm--ap-ac-permissions)))"#;
    let expect = expect_test::expect![[
        r#""OK (((data \"secret-info\") (written test-val) deleted) ((data \"secret-info\") (written edit-val) (access-denied editor delete)) ((data \"secret-info\") (access-denied viewer write) (access-denied viewer delete)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
