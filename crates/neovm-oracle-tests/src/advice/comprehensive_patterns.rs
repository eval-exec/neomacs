//! Comprehensive oracle parity tests for the Emacs advice system:
//! :before, :after, :around, :override, :filter-args, :filter-return,
//! multiple advice stacking order, selective removal, advice-member-p,
//! advice on lambda functions, argument/return modification, and
//! unwind-protect cleanup patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// All six advice types on the same function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_all_six_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exercise every advice type (:before, :after, :around, :override,
    // :filter-args, :filter-return) on the same base function, one at a
    // time, verifying each type's semantics independently.
    let form = r#"(progn
  (defvar neovm--acp-log nil)
  (fset 'neovm--acp-base (lambda (x) (* x 3)))

  ;; :before -- side effect only, does not affect return value
  (fset 'neovm--acp-bef
    (lambda (&rest args)
      (setq neovm--acp-log (cons (cons 'before args) neovm--acp-log))))

  ;; :after -- receives same args as original, return value ignored
  (fset 'neovm--acp-aft
    (lambda (&rest args)
      (setq neovm--acp-log (cons (cons 'after args) neovm--acp-log))))

  ;; :around -- wraps original, can modify args/return
  (fset 'neovm--acp-arn
    (lambda (orig-fn &rest args)
      (setq neovm--acp-log (cons 'around-enter neovm--acp-log))
      (let ((r (apply orig-fn args)))
        (setq neovm--acp-log (cons (list 'around-exit r) neovm--acp-log))
        (+ r 1000))))

  ;; :override -- completely replaces, original not called
  (fset 'neovm--acp-ovr (lambda (&rest args) (cons 'overridden args)))

  ;; :filter-args -- receives arg list, returns modified arg list
  (fset 'neovm--acp-fa (lambda (args) (mapcar #'1+ args)))

  ;; :filter-return -- receives return value, returns modified
  (fset 'neovm--acp-fr (lambda (val) (* val -1)))

  (unwind-protect
      (let (results)
        ;; Test :before
        (setq neovm--acp-log nil)
        (advice-add 'neovm--acp-base :before 'neovm--acp-bef)
        (let ((r (funcall 'neovm--acp-base 7)))
          (setq results (cons (list 'before r (nreverse neovm--acp-log)) results)))
        (advice-remove 'neovm--acp-base 'neovm--acp-bef)

        ;; Test :after
        (setq neovm--acp-log nil)
        (advice-add 'neovm--acp-base :after 'neovm--acp-aft)
        (let ((r (funcall 'neovm--acp-base 7)))
          (setq results (cons (list 'after r (nreverse neovm--acp-log)) results)))
        (advice-remove 'neovm--acp-base 'neovm--acp-aft)

        ;; Test :around
        (setq neovm--acp-log nil)
        (advice-add 'neovm--acp-base :around 'neovm--acp-arn)
        (let ((r (funcall 'neovm--acp-base 7)))
          (setq results (cons (list 'around r (nreverse neovm--acp-log)) results)))
        (advice-remove 'neovm--acp-base 'neovm--acp-arn)

        ;; Test :override
        (advice-add 'neovm--acp-base :override 'neovm--acp-ovr)
        (let ((r (funcall 'neovm--acp-base 7)))
          (setq results (cons (list 'override r) results)))
        (advice-remove 'neovm--acp-base 'neovm--acp-ovr)

        ;; Test :filter-args
        (advice-add 'neovm--acp-base :filter-args 'neovm--acp-fa)
        (let ((r (funcall 'neovm--acp-base 7)))
          (setq results (cons (list 'filter-args r) results)))
        (advice-remove 'neovm--acp-base 'neovm--acp-fa)

        ;; Test :filter-return
        (advice-add 'neovm--acp-base :filter-return 'neovm--acp-fr)
        (let ((r (funcall 'neovm--acp-base 7)))
          (setq results (cons (list 'filter-return r) results)))
        (advice-remove 'neovm--acp-base 'neovm--acp-fr)

        (nreverse results))
    (fmakunbound 'neovm--acp-base)
    (fmakunbound 'neovm--acp-bef)
    (fmakunbound 'neovm--acp-aft)
    (fmakunbound 'neovm--acp-arn)
    (fmakunbound 'neovm--acp-ovr)
    (fmakunbound 'neovm--acp-fa)
    (fmakunbound 'neovm--acp-fr)
    (makunbound 'neovm--acp-log)))"#;
    let expect = expect_test::expect![
        r#""OK ((before 21 ((before 7))) (after 21 ((after 7))) (around 1021 (around-enter (around-exit 21))) (override (overridden 7)) (filter-args 24) (filter-return -21))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple advice stacking: LIFO order verification with 4 :before advisors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_stacking_order_four_before() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Add 4 :before advisors in sequence. Last added runs first (LIFO).
    // Verify execution order by logging.
    let form = r#"(progn
  (defvar neovm--acp-stack-log nil)
  (fset 'neovm--acp-stack-fn (lambda (x) x))

  (fset 'neovm--acp-stack-a (lambda (&rest _) (setq neovm--acp-stack-log (cons 'A neovm--acp-stack-log))))
  (fset 'neovm--acp-stack-b (lambda (&rest _) (setq neovm--acp-stack-log (cons 'B neovm--acp-stack-log))))
  (fset 'neovm--acp-stack-c (lambda (&rest _) (setq neovm--acp-stack-log (cons 'C neovm--acp-stack-log))))
  (fset 'neovm--acp-stack-d (lambda (&rest _) (setq neovm--acp-stack-log (cons 'D neovm--acp-stack-log))))

  (unwind-protect
      (progn
        ;; Add in order A, B, C, D
        (advice-add 'neovm--acp-stack-fn :before 'neovm--acp-stack-a)
        (advice-add 'neovm--acp-stack-fn :before 'neovm--acp-stack-b)
        (advice-add 'neovm--acp-stack-fn :before 'neovm--acp-stack-c)
        (advice-add 'neovm--acp-stack-fn :before 'neovm--acp-stack-d)
        ;; Call
        (setq neovm--acp-stack-log nil)
        (funcall 'neovm--acp-stack-fn 42)
        (let ((log-all (nreverse neovm--acp-stack-log)))
          ;; Remove C from middle
          (advice-remove 'neovm--acp-stack-fn 'neovm--acp-stack-c)
          (setq neovm--acp-stack-log nil)
          (funcall 'neovm--acp-stack-fn 42)
          (let ((log-no-c (nreverse neovm--acp-stack-log)))
            (list log-all log-no-c))))
    (advice-remove 'neovm--acp-stack-fn 'neovm--acp-stack-a)
    (advice-remove 'neovm--acp-stack-fn 'neovm--acp-stack-b)
    (advice-remove 'neovm--acp-stack-fn 'neovm--acp-stack-d)
    (fmakunbound 'neovm--acp-stack-fn)
    (fmakunbound 'neovm--acp-stack-a)
    (fmakunbound 'neovm--acp-stack-b)
    (fmakunbound 'neovm--acp-stack-c)
    (fmakunbound 'neovm--acp-stack-d)
    (makunbound 'neovm--acp-stack-log)))"#;
    let expect = expect_test::expect![r#""OK ((D C B A) (D B A))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// advice-remove selective removal and advice-member-p lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_remove_and_member_p_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Add multiple advice of different types, selectively remove them,
    // checking advice-member-p at each stage.
    let form = r#"(progn
  (fset 'neovm--acp-rm-fn (lambda (x) (* x 2)))
  (fset 'neovm--acp-rm-bef (lambda (&rest _) nil))
  (fset 'neovm--acp-rm-aft (lambda (&rest _) nil))
  (fset 'neovm--acp-rm-arn (lambda (f &rest a) (apply f a)))
  (fset 'neovm--acp-rm-fr (lambda (v) v))

  (unwind-protect
      (let (results)
        ;; Stage 0: no advice
        (setq results
              (cons (list
                     (not (null (advice-member-p 'neovm--acp-rm-bef 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-aft 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-arn 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-fr 'neovm--acp-rm-fn))))
                    results))
        ;; Add all four
        (advice-add 'neovm--acp-rm-fn :before 'neovm--acp-rm-bef)
        (advice-add 'neovm--acp-rm-fn :after 'neovm--acp-rm-aft)
        (advice-add 'neovm--acp-rm-fn :around 'neovm--acp-rm-arn)
        (advice-add 'neovm--acp-rm-fn :filter-return 'neovm--acp-rm-fr)
        ;; Stage 1: all present
        (setq results
              (cons (list
                     (not (null (advice-member-p 'neovm--acp-rm-bef 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-aft 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-arn 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-fr 'neovm--acp-rm-fn))))
                    results))
        ;; Function still works
        (setq results (cons (funcall 'neovm--acp-rm-fn 5) results))
        ;; Remove :around and :before
        (advice-remove 'neovm--acp-rm-fn 'neovm--acp-rm-arn)
        (advice-remove 'neovm--acp-rm-fn 'neovm--acp-rm-bef)
        ;; Stage 2: only :after and :filter-return
        (setq results
              (cons (list
                     (not (null (advice-member-p 'neovm--acp-rm-bef 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-aft 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-arn 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-fr 'neovm--acp-rm-fn))))
                    results))
        ;; Remove remaining
        (advice-remove 'neovm--acp-rm-fn 'neovm--acp-rm-aft)
        (advice-remove 'neovm--acp-rm-fn 'neovm--acp-rm-fr)
        ;; Stage 3: none present, function restored
        (setq results
              (cons (list
                     (not (null (advice-member-p 'neovm--acp-rm-bef 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-aft 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-arn 'neovm--acp-rm-fn)))
                     (not (null (advice-member-p 'neovm--acp-rm-fr 'neovm--acp-rm-fn))))
                    results))
        (setq results (cons (funcall 'neovm--acp-rm-fn 5) results))
        (nreverse results))
    (fmakunbound 'neovm--acp-rm-fn)
    (fmakunbound 'neovm--acp-rm-bef)
    (fmakunbound 'neovm--acp-rm-aft)
    (fmakunbound 'neovm--acp-rm-arn)
    (fmakunbound 'neovm--acp-rm-fr)))"#;
    let expect = expect_test::expect![
        r#""OK ((nil nil nil nil) (t t t t) 10 (nil t nil t) (nil nil nil nil) 10)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :filter-args and :filter-return chaining with multiple advisors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_filter_args_return_chaining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Stack multiple :filter-args and :filter-return advisors and verify
    // they compose correctly (args filters applied outer-to-inner to the
    // arg list, return filters applied inner-to-outer on the result).
    let form = r#"(progn
  (fset 'neovm--acp-ch-fn (lambda (a b) (+ a b)))

  ;; :filter-args A: add 1 to each arg
  (fset 'neovm--acp-ch-fa1
    (lambda (args) (mapcar #'1+ args)))

  ;; :filter-args B: double each arg
  (fset 'neovm--acp-ch-fa2
    (lambda (args) (mapcar (lambda (x) (* x 2)) args)))

  ;; :filter-return A: negate
  (fset 'neovm--acp-ch-fr1
    (lambda (val) (- val)))

  ;; :filter-return B: add 100
  (fset 'neovm--acp-ch-fr2
    (lambda (val) (+ val 100)))

  (unwind-protect
      (list
        ;; Bare: 3 + 5 = 8
        (funcall 'neovm--acp-ch-fn 3 5)
        ;; :filter-args A only: (3+1)+(5+1) = 10
        (progn
          (advice-add 'neovm--acp-ch-fn :filter-args 'neovm--acp-ch-fa1)
          (prog1 (funcall 'neovm--acp-ch-fn 3 5)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fa1)))
        ;; :filter-args A then B stacked: B applied first to args, then A
        ;; Actually in Emacs, last-added filter-args wraps outermost
        ;; So B(args) then A(B(args)): B doubles: (6,10), A adds 1: (7,11) -> 18
        (progn
          (advice-add 'neovm--acp-ch-fn :filter-args 'neovm--acp-ch-fa1)
          (advice-add 'neovm--acp-ch-fn :filter-args 'neovm--acp-ch-fa2)
          (prog1 (funcall 'neovm--acp-ch-fn 3 5)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fa1)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fa2)))
        ;; :filter-return A then B stacked
        (progn
          (advice-add 'neovm--acp-ch-fn :filter-return 'neovm--acp-ch-fr1)
          (advice-add 'neovm--acp-ch-fn :filter-return 'neovm--acp-ch-fr2)
          (prog1 (funcall 'neovm--acp-ch-fn 3 5)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fr1)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fr2)))
        ;; All four filters at once
        (progn
          (advice-add 'neovm--acp-ch-fn :filter-args 'neovm--acp-ch-fa1)
          (advice-add 'neovm--acp-ch-fn :filter-args 'neovm--acp-ch-fa2)
          (advice-add 'neovm--acp-ch-fn :filter-return 'neovm--acp-ch-fr1)
          (advice-add 'neovm--acp-ch-fn :filter-return 'neovm--acp-ch-fr2)
          (prog1 (funcall 'neovm--acp-ch-fn 3 5)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fa1)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fa2)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fr1)
            (advice-remove 'neovm--acp-ch-fn 'neovm--acp-ch-fr2))))
    (fmakunbound 'neovm--acp-ch-fn)
    (fmakunbound 'neovm--acp-ch-fa1)
    (fmakunbound 'neovm--acp-ch-fa2)
    (fmakunbound 'neovm--acp-ch-fr1)
    (fmakunbound 'neovm--acp-ch-fr2)))"#;
    let expect = expect_test::expect![r#""OK (8 10 18 92 82)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Advice modifying arguments and return values via :around
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_around_modify_args_and_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :around advice that transforms arguments before calling original,
    // transforms the return value after, and stacks two such transforms.
    let form = r#"(progn
  (fset 'neovm--acp-mod-fn (lambda (a b c) (list a b c)))

  ;; :around that reverses arg order
  (fset 'neovm--acp-mod-rev
    (lambda (orig-fn &rest args)
      (apply orig-fn (reverse args))))

  ;; :around that stringifies the result
  (fset 'neovm--acp-mod-str
    (lambda (orig-fn &rest args)
      (let ((result (apply orig-fn args)))
        (format "%S" result))))

  ;; :around that adds a wrapper cons
  (fset 'neovm--acp-mod-wrap
    (lambda (orig-fn &rest args)
      (let ((result (apply orig-fn args)))
        (cons 'wrapped result))))

  (unwind-protect
      (list
        ;; Bare: (1 2 3)
        (funcall 'neovm--acp-mod-fn 1 2 3)
        ;; Reversed args: (3 2 1)
        (progn
          (advice-add 'neovm--acp-mod-fn :around 'neovm--acp-mod-rev)
          (prog1 (funcall 'neovm--acp-mod-fn 1 2 3)
            (advice-remove 'neovm--acp-mod-fn 'neovm--acp-mod-rev)))
        ;; Stringify result
        (progn
          (advice-add 'neovm--acp-mod-fn :around 'neovm--acp-mod-str)
          (prog1 (funcall 'neovm--acp-mod-fn 1 2 3)
            (advice-remove 'neovm--acp-mod-fn 'neovm--acp-mod-str)))
        ;; Wrap result
        (progn
          (advice-add 'neovm--acp-mod-fn :around 'neovm--acp-mod-wrap)
          (prog1 (funcall 'neovm--acp-mod-fn 1 2 3)
            (advice-remove 'neovm--acp-mod-fn 'neovm--acp-mod-wrap)))
        ;; Stack: reverse args + wrap result
        ;; wrap is outer, rev is inner: wrap(rev(orig)(...))
        ;; rev reverses args -> orig(3,2,1) -> (3 2 1), wrap -> (wrapped 3 2 1)
        (progn
          (advice-add 'neovm--acp-mod-fn :around 'neovm--acp-mod-rev)
          (advice-add 'neovm--acp-mod-fn :around 'neovm--acp-mod-wrap)
          (prog1 (funcall 'neovm--acp-mod-fn 1 2 3)
            (advice-remove 'neovm--acp-mod-fn 'neovm--acp-mod-rev)
            (advice-remove 'neovm--acp-mod-fn 'neovm--acp-mod-wrap))))
    (fmakunbound 'neovm--acp-mod-fn)
    (fmakunbound 'neovm--acp-mod-rev)
    (fmakunbound 'neovm--acp-mod-str)
    (fmakunbound 'neovm--acp-mod-wrap)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (3 2 1) \"(1 2 3)\" (wrapped 1 2 3) (wrapped 3 2 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Advice with unwind-protect cleanup: advice body errors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_unwind_protect_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // :around advice that uses unwind-protect internally to guarantee
    // cleanup even if the original function signals an error.
    let form = r#"(progn
  (defvar neovm--acp-up-cleanup-ran nil)
  (defvar neovm--acp-up-resource nil)

  (fset 'neovm--acp-up-fn
    (lambda (should-error)
      (if should-error
          (signal 'error '("intentional failure"))
        'success)))

  ;; :around advice with resource management via unwind-protect
  (fset 'neovm--acp-up-resource-mgr
    (lambda (orig-fn &rest args)
      (setq neovm--acp-up-resource 'acquired)
      (unwind-protect
          (apply orig-fn args)
        (setq neovm--acp-up-resource 'released)
        (setq neovm--acp-up-cleanup-ran t))))

  (unwind-protect
      (progn
        (advice-add 'neovm--acp-up-fn :around 'neovm--acp-up-resource-mgr)
        ;; Case 1: no error
        (setq neovm--acp-up-cleanup-ran nil)
        (setq neovm--acp-up-resource nil)
        (let ((r1 (funcall 'neovm--acp-up-fn nil))
              (cleanup1 neovm--acp-up-cleanup-ran)
              (resource1 neovm--acp-up-resource))
          ;; Case 2: error -- caught outside
          (setq neovm--acp-up-cleanup-ran nil)
          (setq neovm--acp-up-resource nil)
          (let ((r2 (condition-case err
                        (funcall 'neovm--acp-up-fn t)
                      (error (list 'caught (cadr err)))))
                (cleanup2 neovm--acp-up-cleanup-ran)
                (resource2 neovm--acp-up-resource))
            (list
              (list r1 cleanup1 resource1)
              (list r2 cleanup2 resource2)))))
    (advice-remove 'neovm--acp-up-fn 'neovm--acp-up-resource-mgr)
    (fmakunbound 'neovm--acp-up-fn)
    (fmakunbound 'neovm--acp-up-resource-mgr)
    (makunbound 'neovm--acp-up-cleanup-ran)
    (makunbound 'neovm--acp-up-resource)))"#;
    let expect = expect_test::expect![[
        r#""OK ((success t released) ((caught \"intentional failure\") t released))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Advice on lambda stored in a variable (non-symbol function)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_advice_on_named_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // advice-add works on symbol-function. Test with fset to a lambda,
    // then advise, call via funcall of the symbol, and verify behavior.
    // Also test re-advising after fset replaces the function.
    let form = r#"(progn
  (fset 'neovm--acp-lam (lambda (x) (+ x 10)))
  (fset 'neovm--acp-lam-adv
    (lambda (orig-fn &rest args)
      (let ((r (apply orig-fn args)))
        (* r 2))))

  (unwind-protect
      (let (results)
        ;; Bare: 5 + 10 = 15
        (setq results (cons (funcall 'neovm--acp-lam 5) results))
        ;; Advised: (5 + 10) * 2 = 30
        (advice-add 'neovm--acp-lam :around 'neovm--acp-lam-adv)
        (setq results (cons (funcall 'neovm--acp-lam 5) results))
        ;; Replace underlying function via fset
        (fset 'neovm--acp-lam (lambda (x) (* x 3)))
        ;; Advice is on the symbol, but now the base function changed
        ;; After fset the advice is lost (fset replaces symbol-function entirely)
        (setq results (cons (funcall 'neovm--acp-lam 5) results))
        ;; Re-add advice on new function
        (advice-add 'neovm--acp-lam :around 'neovm--acp-lam-adv)
        ;; (5 * 3) * 2 = 30
        (setq results (cons (funcall 'neovm--acp-lam 5) results))
        (advice-remove 'neovm--acp-lam 'neovm--acp-lam-adv)
        ;; Bare new function: 5 * 3 = 15
        (setq results (cons (funcall 'neovm--acp-lam 5) results))
        (nreverse results))
    (advice-remove 'neovm--acp-lam 'neovm--acp-lam-adv)
    (fmakunbound 'neovm--acp-lam)
    (fmakunbound 'neovm--acp-lam-adv)))"#;
    let expect = expect_test::expect![r#""OK (15 30 15 30 15)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: advice-based input validation / type checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_input_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use :filter-args to validate/coerce arguments and :around to
    // catch validation errors, implementing a type-checked function wrapper.
    let form = r#"(progn
  (fset 'neovm--acp-val-fn
    (lambda (name age score)
      (list 'record name age score)))

  ;; :filter-args that validates and coerces
  (fset 'neovm--acp-val-check
    (lambda (args)
      (let ((name (nth 0 args))
            (age (nth 1 args))
            (score (nth 2 args)))
        ;; Coerce name to string
        (unless (stringp name)
          (setq name (format "%s" name)))
        ;; Validate age is positive integer
        (unless (and (integerp age) (> age 0))
          (signal 'wrong-type-argument (list 'positive-integer age)))
        ;; Clamp score to [0, 100]
        (when (< score 0) (setq score 0))
        (when (> score 100) (setq score 100))
        (list name age score))))

  (unwind-protect
      (progn
        (advice-add 'neovm--acp-val-fn :filter-args 'neovm--acp-val-check)
        (list
          ;; Valid input
          (funcall 'neovm--acp-val-fn "Alice" 30 85)
          ;; Name coerced from symbol
          (funcall 'neovm--acp-val-fn 'bob 25 70)
          ;; Score clamped high
          (funcall 'neovm--acp-val-fn "Carol" 28 150)
          ;; Score clamped low
          (funcall 'neovm--acp-val-fn "Dave" 35 -10)
          ;; Invalid age caught
          (condition-case err
              (funcall 'neovm--acp-val-fn "Eve" -5 50)
            (wrong-type-argument (list 'error (car err) (cadr err))))))
    (advice-remove 'neovm--acp-val-fn 'neovm--acp-val-check)
    (fmakunbound 'neovm--acp-val-fn)
    (fmakunbound 'neovm--acp-val-check)))"#;
    let expect = expect_test::expect![[
        r#""OK ((record \"Alice\" 30 85) (record \"bob\" 25 70) (record \"Carol\" 28 100) (record \"Dave\" 35 0) (error wrong-type-argument positive-integer))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: mixed advice types composing on one function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_advice_comp_mixed_types_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Combine :before, :after, :around, :filter-args, :filter-return
    // all on the same function and verify the complete execution order
    // and data flow.
    let form = r#"(progn
  (defvar neovm--acp-mix-log nil)

  (fset 'neovm--acp-mix-fn
    (lambda (x)
      (setq neovm--acp-mix-log (cons (list 'orig x) neovm--acp-mix-log))
      (* x 2)))

  ;; :filter-args: add 100 to arg
  (fset 'neovm--acp-mix-fa
    (lambda (args)
      (setq neovm--acp-mix-log (cons (list 'filter-args args) neovm--acp-mix-log))
      (list (+ (car args) 100))))

  ;; :before: log entry
  (fset 'neovm--acp-mix-bef
    (lambda (&rest args)
      (setq neovm--acp-mix-log (cons (cons 'before args) neovm--acp-mix-log))))

  ;; :around: log and add 1 to result
  (fset 'neovm--acp-mix-arn
    (lambda (orig-fn &rest args)
      (setq neovm--acp-mix-log (cons (cons 'around-in args) neovm--acp-mix-log))
      (let ((r (apply orig-fn args)))
        (setq neovm--acp-mix-log (cons (list 'around-out r) neovm--acp-mix-log))
        (+ r 1))))

  ;; :after: log exit
  (fset 'neovm--acp-mix-aft
    (lambda (&rest args)
      (setq neovm--acp-mix-log (cons (cons 'after args) neovm--acp-mix-log))))

  ;; :filter-return: negate
  (fset 'neovm--acp-mix-fr
    (lambda (val)
      (setq neovm--acp-mix-log (cons (list 'filter-return val) neovm--acp-mix-log))
      (- val)))

  (unwind-protect
      (progn
        (advice-add 'neovm--acp-mix-fn :filter-args 'neovm--acp-mix-fa)
        (advice-add 'neovm--acp-mix-fn :before 'neovm--acp-mix-bef)
        (advice-add 'neovm--acp-mix-fn :around 'neovm--acp-mix-arn)
        (advice-add 'neovm--acp-mix-fn :after 'neovm--acp-mix-aft)
        (advice-add 'neovm--acp-mix-fn :filter-return 'neovm--acp-mix-fr)
        (setq neovm--acp-mix-log nil)
        (let ((result (funcall 'neovm--acp-mix-fn 5)))
          (list result (nreverse neovm--acp-mix-log))))
    (advice-remove 'neovm--acp-mix-fn 'neovm--acp-mix-fa)
    (advice-remove 'neovm--acp-mix-fn 'neovm--acp-mix-bef)
    (advice-remove 'neovm--acp-mix-fn 'neovm--acp-mix-arn)
    (advice-remove 'neovm--acp-mix-fn 'neovm--acp-mix-aft)
    (advice-remove 'neovm--acp-mix-fn 'neovm--acp-mix-fr)
    (fmakunbound 'neovm--acp-mix-fn)
    (fmakunbound 'neovm--acp-mix-fa)
    (fmakunbound 'neovm--acp-mix-bef)
    (fmakunbound 'neovm--acp-mix-arn)
    (fmakunbound 'neovm--acp-mix-aft)
    (fmakunbound 'neovm--acp-mix-fr)
    (makunbound 'neovm--acp-mix-log)))"#;
    let expect = expect_test::expect![
        r#""OK (-211 ((around-in 5) (before 5) (filter-args (5)) (orig 105) (around-out 210) (after 5) (filter-return 211)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
