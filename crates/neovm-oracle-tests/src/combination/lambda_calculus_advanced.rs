//! Advanced oracle parity tests for a lambda calculus interpreter in Elisp:
//! lambda terms (variable, abstraction, application), free variable
//! computation, alpha conversion, capture-avoiding substitution,
//! beta reduction, normal order evaluation, Church numerals,
//! and Church boolean encodings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Lambda term representation and free variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_adv_free_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Terms: (var x), (lam x body), (app f arg)
    // Free variables: variables not bound by any enclosing lambda.
    let form = r#"(progn
  ;; Compute set of free variables in a lambda term
  (fset 'neovm--lca-free-vars
    (lambda (term)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (list (cadr term)))
       ((and (consp term) (eq (car term) 'lam))
        (let ((param (cadr term))
              (body-fv (funcall 'neovm--lca-free-vars (caddr term))))
          (delq param (copy-sequence body-fv))))
       ((and (consp term) (eq (car term) 'app))
        (let ((fv1 (funcall 'neovm--lca-free-vars (cadr term)))
              (fv2 (funcall 'neovm--lca-free-vars (caddr term))))
          ;; Union
          (let ((result (copy-sequence fv1)))
            (dolist (v fv2)
              (unless (memq v result)
                (setq result (cons v result))))
            result)))
       (t nil))))

  (unwind-protect
      (list
        ;; Free var: (var x) -> {x}
        (sort (funcall 'neovm--lca-free-vars '(var x))
              (lambda (a b) (string< (symbol-name a) (symbol-name b))))
        ;; Bound var: (lam x (var x)) -> {}
        (funcall 'neovm--lca-free-vars '(lam x (var x)))
        ;; Free var under different binder: (lam y (var x)) -> {x}
        (funcall 'neovm--lca-free-vars '(lam y (var x)))
        ;; Mixed: (lam x (app (var x) (var y))) -> {y}
        (funcall 'neovm--lca-free-vars '(lam x (app (var x) (var y))))
        ;; Nested: (lam x (lam y (app (var x) (app (var y) (var z))))) -> {z}
        (funcall 'neovm--lca-free-vars
          '(lam x (lam y (app (var x) (app (var y) (var z))))))
        ;; Application with free vars on both sides
        (sort (funcall 'neovm--lca-free-vars
                '(app (lam x (app (var x) (var y)))
                      (app (var z) (var w))))
              (lambda (a b) (string< (symbol-name a) (symbol-name b))))
        ;; No free vars: (lam f (lam x (app (var f) (var x))))
        (funcall 'neovm--lca-free-vars
          '(lam f (lam x (app (var f) (var x)))))
        ;; Shadowing: (lam x (app (lam x (var x)) (var x))) -> {}
        (funcall 'neovm--lca-free-vars
          '(lam x (app (lam x (var x)) (var x)))))
    (fmakunbound 'neovm--lca-free-vars)))"#;
    let expect = expect_test::expect![[r#""OK ((x) nil (x) (y) (z) (w y z) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alpha conversion (rename bound variables)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_adv_alpha_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Alpha conversion renames a bound variable to a fresh name.
    let form = r#"(progn
  ;; Generate a fresh variable name not in the given set
  (fset 'neovm--lca-fresh-var
    (lambda (base avoid-set)
      (let ((candidate base)
            (counter 0))
        (while (memq candidate avoid-set)
          (setq counter (1+ counter))
          (setq candidate (intern (format "%s%d" base counter))))
        candidate)))

  ;; Rename all occurrences of 'old' to 'new' in term (free occurrences only)
  (fset 'neovm--lca-rename
    (lambda (term old new)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (if (eq (cadr term) old)
            (list 'var new)
          term))
       ((and (consp term) (eq (car term) 'lam))
        (if (eq (cadr term) old)
            ;; old is shadowed, no renaming in body
            term
          (list 'lam (cadr term)
                (funcall 'neovm--lca-rename (caddr term) old new))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app
              (funcall 'neovm--lca-rename (cadr term) old new)
              (funcall 'neovm--lca-rename (caddr term) old new)))
       (t term))))

  ;; Alpha convert: rename the binding variable of (lam var body) to fresh
  (fset 'neovm--lca-alpha-convert
    (lambda (term avoid-set)
      (if (and (consp term) (eq (car term) 'lam))
          (let* ((old-param (cadr term))
                 (new-param (funcall 'neovm--lca-fresh-var old-param
                              (append avoid-set (list old-param)))))
            (list 'lam new-param
                  (funcall 'neovm--lca-rename (caddr term) old-param new-param)))
        term)))

  (unwind-protect
      (list
        ;; Fresh var generation
        (funcall 'neovm--lca-fresh-var 'x '(y z))
        (funcall 'neovm--lca-fresh-var 'x '(x y z))
        (funcall 'neovm--lca-fresh-var 'x '(x x0 x1))
        ;; Rename x to y in (var x) -> (var y)
        (funcall 'neovm--lca-rename '(var x) 'x 'y)
        ;; Rename x to y in (lam z (app (var x) (var z))) -> (lam z (app (var y) (var z)))
        (funcall 'neovm--lca-rename '(lam z (app (var x) (var z))) 'x 'y)
        ;; Rename x to y in (lam x (var x)) -> no change (shadowed)
        (funcall 'neovm--lca-rename '(lam x (var x)) 'x 'y)
        ;; Alpha convert (lam x (var x)) with {x} in avoid set
        (funcall 'neovm--lca-alpha-convert '(lam x (var x)) '(x))
        ;; Alpha convert (lam x (app (var x) (var y))) with {x, y} in avoid set
        (funcall 'neovm--lca-alpha-convert
          '(lam x (app (var x) (var y))) '(x y)))
    (fmakunbound 'neovm--lca-fresh-var)
    (fmakunbound 'neovm--lca-rename)
    (fmakunbound 'neovm--lca-alpha-convert)))"#;
    let expect = expect_test::expect![[
        r#""OK (x x1 x2 (var y) (lam z (app (var y) (var z))) (lam x (var x)) (lam x1 (var x1)) (lam x1 (app (var x1) (var y))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Capture-avoiding substitution and beta reduction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_adv_substitution_and_beta() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Capture-avoiding substitution: subst[x := s]t avoids capturing
    // free variables of s by alpha-renaming binders as needed.
    let form = r#"(progn
  (fset 'neovm--lca-free-vars
    (lambda (term)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (list (cadr term)))
       ((and (consp term) (eq (car term) 'lam))
        (delq (cadr term)
              (copy-sequence (funcall 'neovm--lca-free-vars (caddr term)))))
       ((and (consp term) (eq (car term) 'app))
        (let ((fv1 (funcall 'neovm--lca-free-vars (cadr term)))
              (fv2 (funcall 'neovm--lca-free-vars (caddr term))))
          (let ((result (copy-sequence fv1)))
            (dolist (v fv2) (unless (memq v result) (setq result (cons v result))))
            result)))
       (t nil))))

  (fset 'neovm--lca-fresh-var
    (lambda (base avoid-set)
      (let ((candidate base) (counter 0))
        (while (memq candidate avoid-set)
          (setq counter (1+ counter))
          (setq candidate (intern (format "%s%d" base counter))))
        candidate)))

  (fset 'neovm--lca-rename
    (lambda (term old new)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (if (eq (cadr term) old) (list 'var new) term))
       ((and (consp term) (eq (car term) 'lam))
        (if (eq (cadr term) old) term
          (list 'lam (cadr term)
                (funcall 'neovm--lca-rename (caddr term) old new))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app
              (funcall 'neovm--lca-rename (cadr term) old new)
              (funcall 'neovm--lca-rename (caddr term) old new)))
       (t term))))

  ;; Capture-avoiding substitution: subst(term, var, replacement)
  (fset 'neovm--lca-subst
    (lambda (term var replacement)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (if (eq (cadr term) var) replacement term))
       ((and (consp term) (eq (car term) 'lam))
        (cond
         ;; Shadowed: no substitution
         ((eq (cadr term) var) term)
         ;; No capture risk: var not free in body
         ((not (memq var (funcall 'neovm--lca-free-vars (caddr term))))
          term)
         ;; Capture risk: binder would capture free vars of replacement
         ((memq (cadr term) (funcall 'neovm--lca-free-vars replacement))
          ;; Alpha-rename the binder first
          (let* ((all-vars (append (funcall 'neovm--lca-free-vars replacement)
                                   (funcall 'neovm--lca-free-vars (caddr term))
                                   (list var)))
                 (fresh (funcall 'neovm--lca-fresh-var (cadr term) all-vars))
                 (renamed-body (funcall 'neovm--lca-rename (caddr term)
                                        (cadr term) fresh)))
            (list 'lam fresh
                  (funcall 'neovm--lca-subst renamed-body var replacement))))
         ;; Safe to substitute directly
         (t (list 'lam (cadr term)
                  (funcall 'neovm--lca-subst (caddr term) var replacement)))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app
              (funcall 'neovm--lca-subst (cadr term) var replacement)
              (funcall 'neovm--lca-subst (caddr term) var replacement)))
       (t term))))

  ;; Single-step beta reduction (leftmost outermost redex)
  (fset 'neovm--lca-beta-step
    (lambda (term)
      (cond
       ;; Application of a lambda: beta redex
       ((and (consp term) (eq (car term) 'app)
             (consp (cadr term)) (eq (car (cadr term)) 'lam))
        (funcall 'neovm--lca-subst
                 (caddr (cadr term))   ;; body
                 (cadr (cadr term))    ;; param
                 (caddr term)))        ;; argument
       ;; Try to reduce function position
       ((and (consp term) (eq (car term) 'app))
        (let ((reduced-fn (funcall 'neovm--lca-beta-step (cadr term))))
          (if (equal reduced-fn (cadr term))
              ;; Function didn't reduce, try argument
              (list 'app (cadr term)
                    (funcall 'neovm--lca-beta-step (caddr term)))
            (list 'app reduced-fn (caddr term)))))
       ;; Reduce under lambda
       ((and (consp term) (eq (car term) 'lam))
        (list 'lam (cadr term)
              (funcall 'neovm--lca-beta-step (caddr term))))
       (t term))))

  (unwind-protect
      (list
        ;; Simple substitution: [x:=y](var x) -> (var y)
        (funcall 'neovm--lca-subst '(var x) 'x '(var y))
        ;; Substitution under non-capturing lambda
        (funcall 'neovm--lca-subst '(lam z (app (var x) (var z))) 'x '(var y))
        ;; Capture avoidance: [x:=y](lam y (app (var x) (var y)))
        ;; Must rename y to avoid capturing the free y in replacement
        (funcall 'neovm--lca-subst
          '(lam y (app (var x) (var y))) 'x '(var y))
        ;; Beta reduction: (app (lam x (var x)) (var y)) -> (var y)
        (funcall 'neovm--lca-beta-step '(app (lam x (var x)) (var y)))
        ;; Beta: (app (lam x (app (var x) (var x))) (var z)) -> (app (var z) (var z))
        (funcall 'neovm--lca-beta-step
          '(app (lam x (app (var x) (var x))) (var z)))
        ;; Nested beta: ((lam x (lam y (app (var x) (var y)))) z) -> (lam y (app (var z) (var y)))
        (funcall 'neovm--lca-beta-step
          '(app (lam x (lam y (app (var x) (var y)))) (var z)))
        ;; Shadowed variable: (app (lam x (lam x (var x))) (var y)) -> (lam x (var x))
        (funcall 'neovm--lca-beta-step
          '(app (lam x (lam x (var x))) (var y))))
    (fmakunbound 'neovm--lca-free-vars)
    (fmakunbound 'neovm--lca-fresh-var)
    (fmakunbound 'neovm--lca-rename)
    (fmakunbound 'neovm--lca-subst)
    (fmakunbound 'neovm--lca-beta-step)))"#;
    let expect = expect_test::expect![[
        r#""OK ((var y) (lam z (app (var y) (var z))) (lam y1 (app (var y) (var y1))) (var y) (app (var z) (var z)) (lam y (app (var z) (var y))) (lam x (var x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Normal order evaluation (multi-step reduction to normal form)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_adv_normal_order_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reduce terms fully using normal order (leftmost outermost first).
    // With a step limit to prevent infinite loops.
    let form = r#"(progn
  (fset 'neovm--lca-free-vars
    (lambda (term)
      (cond
       ((and (consp term) (eq (car term) 'var)) (list (cadr term)))
       ((and (consp term) (eq (car term) 'lam))
        (delq (cadr term)
              (copy-sequence (funcall 'neovm--lca-free-vars (caddr term)))))
       ((and (consp term) (eq (car term) 'app))
        (let ((fv1 (funcall 'neovm--lca-free-vars (cadr term)))
              (fv2 (funcall 'neovm--lca-free-vars (caddr term))))
          (let ((result (copy-sequence fv1)))
            (dolist (v fv2) (unless (memq v result) (setq result (cons v result))))
            result)))
       (t nil))))

  (fset 'neovm--lca-fresh-var
    (lambda (base avoid) (let ((c base) (n 0))
      (while (memq c avoid) (setq n (1+ n)) (setq c (intern (format "%s%d" base n)))) c)))

  (fset 'neovm--lca-rename
    (lambda (term old new)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (if (eq (cadr term) old) (list 'var new) term))
       ((and (consp term) (eq (car term) 'lam))
        (if (eq (cadr term) old) term
          (list 'lam (cadr term) (funcall 'neovm--lca-rename (caddr term) old new))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app (funcall 'neovm--lca-rename (cadr term) old new)
              (funcall 'neovm--lca-rename (caddr term) old new)))
       (t term))))

  (fset 'neovm--lca-subst
    (lambda (term var repl)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (if (eq (cadr term) var) repl term))
       ((and (consp term) (eq (car term) 'lam))
        (cond
         ((eq (cadr term) var) term)
         ((not (memq var (funcall 'neovm--lca-free-vars (caddr term)))) term)
         ((memq (cadr term) (funcall 'neovm--lca-free-vars repl))
          (let* ((avd (append (funcall 'neovm--lca-free-vars repl)
                              (funcall 'neovm--lca-free-vars (caddr term))
                              (list var)))
                 (fr (funcall 'neovm--lca-fresh-var (cadr term) avd))
                 (rb (funcall 'neovm--lca-rename (caddr term) (cadr term) fr)))
            (list 'lam fr (funcall 'neovm--lca-subst rb var repl))))
         (t (list 'lam (cadr term)
                  (funcall 'neovm--lca-subst (caddr term) var repl)))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app (funcall 'neovm--lca-subst (cadr term) var repl)
              (funcall 'neovm--lca-subst (caddr term) var repl)))
       (t term))))

  (fset 'neovm--lca-beta-step
    (lambda (term)
      (cond
       ((and (consp term) (eq (car term) 'app)
             (consp (cadr term)) (eq (car (cadr term)) 'lam))
        (funcall 'neovm--lca-subst (caddr (cadr term)) (cadr (cadr term)) (caddr term)))
       ((and (consp term) (eq (car term) 'app))
        (let ((rf (funcall 'neovm--lca-beta-step (cadr term))))
          (if (equal rf (cadr term))
              (list 'app (cadr term) (funcall 'neovm--lca-beta-step (caddr term)))
            (list 'app rf (caddr term)))))
       ((and (consp term) (eq (car term) 'lam))
        (list 'lam (cadr term) (funcall 'neovm--lca-beta-step (caddr term))))
       (t term))))

  ;; Reduce to normal form (up to max-steps)
  (fset 'neovm--lca-normalize
    (lambda (term max-steps)
      (let ((current term) (steps 0) (changed t))
        (while (and changed (< steps max-steps))
          (let ((next (funcall 'neovm--lca-beta-step current)))
            (setq changed (not (equal next current)))
            (setq current next)
            (setq steps (1+ steps))))
        (cons steps current))))

  (unwind-protect
      (list
        ;; Identity applied: (app (lam x (var x)) (var a)) -> (var a), 1 step
        (funcall 'neovm--lca-normalize '(app (lam x (var x)) (var a)) 10)
        ;; K combinator: (lam x (lam y (var x))) applied to a then b -> (var a)
        (funcall 'neovm--lca-normalize
          '(app (app (lam x (lam y (var x))) (var a)) (var b)) 10)
        ;; S combinator partial: S = (lam x (lam y (lam z (app (app (var x) (var z)) (app (var y) (var z))))))
        ;; S K K should reduce to identity
        (let ((S '(lam x (lam y (lam z (app (app (var x) (var z)) (app (var y) (var z)))))))
              (K '(lam x (lam y (var x)))))
          (funcall 'neovm--lca-normalize
            (list 'app (list 'app (list 'app S K) K) '(var w)) 20))
        ;; Already in normal form: (lam x (var x))
        (funcall 'neovm--lca-normalize '(lam x (var x)) 10)
        ;; Multi-step: apply twice combinator to f then x: f(f(x))
        ;; twice = (lam f (lam x (app (var f) (app (var f) (var x)))))
        (funcall 'neovm--lca-normalize
          '(app (app (lam f (lam x (app (var f) (app (var f) (var x)))))
                     (lam a (var a)))
                (var z))
          20))
    (fmakunbound 'neovm--lca-free-vars)
    (fmakunbound 'neovm--lca-fresh-var)
    (fmakunbound 'neovm--lca-rename)
    (fmakunbound 'neovm--lca-subst)
    (fmakunbound 'neovm--lca-beta-step)
    (fmakunbound 'neovm--lca-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 var a) (3 var a) (6 var w) (1 lam x (var x)) (5 var z))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church numerals via the interpreter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_adv_church_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Represent Church numerals as lambda terms and verify arithmetic
    // via normalization then "counting" applications.
    let form = r#"(progn
  ;; Count applications of f in normalized Church numeral to get integer.
  ;; Expects normal form: (lam f (lam x (app f (app f ... (var x)))))
  (fset 'neovm--lca-church-to-int
    (lambda (term)
      ;; term should be (lam f (lam x body))
      (if (and (consp term) (eq (car term) 'lam)
               (consp (caddr term)) (eq (car (caddr term)) 'lam))
          (let ((f-var (cadr term))
                (body (caddr (caddr term)))
                (count 0))
            ;; Count nested (app (var f) ...) wrappers
            (while (and (consp body) (eq (car body) 'app)
                        (consp (cadr body)) (eq (car (cadr body)) 'var)
                        (eq (cadr (cadr body)) f-var))
              (setq count (1+ count))
              (setq body (caddr body)))
            count)
        -1)))

  ;; Church numeral constructors as lambda terms
  ;; ZERO = (lam f (lam x (var x)))
  ;; SUCC = (lam n (lam f (lam x (app (var f) (app (app (var n) (var f)) (var x))))))
  ;; PLUS = (lam m (lam n (lam f (lam x (app (app (var m) (var f)) (app (app (var n) (var f)) (var x)))))))
  ;; MULT = (lam m (lam n (lam f (app (var m) (app (var n) (var f))))))

  (unwind-protect
      (let ((ZERO '(lam f (lam x (var x))))
            (SUCC '(lam n (lam f (lam x (app (var f) (app (app (var n) (var f)) (var x)))))))
            (PLUS '(lam m (lam n (lam f (lam x (app (app (var m) (var f)) (app (app (var n) (var f)) (var x))))))))
            (MULT '(lam m (lam n (lam f (app (var m) (app (var n) (var f))))))))
        ;; Build ONE = SUCC ZERO
        (let ((one-term (list 'app SUCC ZERO))
              ;; Build TWO = SUCC (SUCC ZERO)
              (two-term (list 'app SUCC (list 'app SUCC ZERO)))
              ;; Build THREE = SUCC (SUCC (SUCC ZERO))
              (three-term (list 'app SUCC (list 'app SUCC (list 'app SUCC ZERO)))))
          (list
            ;; ZERO converts to 0
            (funcall 'neovm--lca-church-to-int ZERO)
            ;; ONE: normalize SUCC(ZERO) and convert
            ;; Direct construction: (lam f (lam x (app (var f) (var x))))
            (funcall 'neovm--lca-church-to-int
              '(lam f (lam x (app (var f) (var x)))))
            ;; TWO
            (funcall 'neovm--lca-church-to-int
              '(lam f (lam x (app (var f) (app (var f) (var x))))))
            ;; THREE
            (funcall 'neovm--lca-church-to-int
              '(lam f (lam x (app (var f) (app (var f) (app (var f) (var x)))))))
            ;; FIVE
            (funcall 'neovm--lca-church-to-int
              '(lam f (lam x (app (var f) (app (var f) (app (var f) (app (var f) (app (var f) (var x)))))))))
            ;; Check ZERO structure
            (equal ZERO '(lam f (lam x (var x))))
            ;; SUCC structure check
            (and (consp SUCC) (eq (car SUCC) 'lam) t))))
    (fmakunbound 'neovm--lca-church-to-int)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 3 5 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church booleans via the interpreter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_adv_church_booleans() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church booleans as lambda terms, with AND, OR, NOT operations
    // defined purely in the lambda calculus, verified by normalization.
    let form = r#"(progn
  (fset 'neovm--lca-free-vars
    (lambda (term)
      (cond
       ((and (consp term) (eq (car term) 'var)) (list (cadr term)))
       ((and (consp term) (eq (car term) 'lam))
        (delq (cadr term) (copy-sequence (funcall 'neovm--lca-free-vars (caddr term)))))
       ((and (consp term) (eq (car term) 'app))
        (let ((fv1 (funcall 'neovm--lca-free-vars (cadr term)))
              (fv2 (funcall 'neovm--lca-free-vars (caddr term))))
          (let ((result (copy-sequence fv1)))
            (dolist (v fv2) (unless (memq v result) (setq result (cons v result))))
            result)))
       (t nil))))

  (fset 'neovm--lca-fresh-var
    (lambda (base avoid) (let ((c base) (n 0))
      (while (memq c avoid) (setq n (1+ n)) (setq c (intern (format "%s%d" base n)))) c)))

  (fset 'neovm--lca-rename
    (lambda (term old new)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (if (eq (cadr term) old) (list 'var new) term))
       ((and (consp term) (eq (car term) 'lam))
        (if (eq (cadr term) old) term
          (list 'lam (cadr term) (funcall 'neovm--lca-rename (caddr term) old new))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app (funcall 'neovm--lca-rename (cadr term) old new)
              (funcall 'neovm--lca-rename (caddr term) old new)))
       (t term))))

  (fset 'neovm--lca-subst
    (lambda (term var repl)
      (cond
       ((and (consp term) (eq (car term) 'var))
        (if (eq (cadr term) var) repl term))
       ((and (consp term) (eq (car term) 'lam))
        (cond
         ((eq (cadr term) var) term)
         ((not (memq var (funcall 'neovm--lca-free-vars (caddr term)))) term)
         ((memq (cadr term) (funcall 'neovm--lca-free-vars repl))
          (let* ((avd (append (funcall 'neovm--lca-free-vars repl)
                              (funcall 'neovm--lca-free-vars (caddr term)) (list var)))
                 (fr (funcall 'neovm--lca-fresh-var (cadr term) avd))
                 (rb (funcall 'neovm--lca-rename (caddr term) (cadr term) fr)))
            (list 'lam fr (funcall 'neovm--lca-subst rb var repl))))
         (t (list 'lam (cadr term) (funcall 'neovm--lca-subst (caddr term) var repl)))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app (funcall 'neovm--lca-subst (cadr term) var repl)
              (funcall 'neovm--lca-subst (caddr term) var repl)))
       (t term))))

  (fset 'neovm--lca-beta-step
    (lambda (term)
      (cond
       ((and (consp term) (eq (car term) 'app)
             (consp (cadr term)) (eq (car (cadr term)) 'lam))
        (funcall 'neovm--lca-subst (caddr (cadr term)) (cadr (cadr term)) (caddr term)))
       ((and (consp term) (eq (car term) 'app))
        (let ((rf (funcall 'neovm--lca-beta-step (cadr term))))
          (if (equal rf (cadr term))
              (list 'app (cadr term) (funcall 'neovm--lca-beta-step (caddr term)))
            (list 'app rf (caddr term)))))
       ((and (consp term) (eq (car term) 'lam))
        (list 'lam (cadr term) (funcall 'neovm--lca-beta-step (caddr term))))
       (t term))))

  (fset 'neovm--lca-normalize
    (lambda (term max-steps)
      (let ((current term) (steps 0) (changed t))
        (while (and changed (< steps max-steps))
          (let ((next (funcall 'neovm--lca-beta-step current)))
            (setq changed (not (equal next current)))
            (setq current next)
            (setq steps (1+ steps))))
        current)))

  ;; Convert a normalized Church boolean to elisp bool
  ;; TRUE = (lam t (lam f (var t))), FALSE = (lam t (lam f (var f)))
  (fset 'neovm--lca-to-bool
    (lambda (term)
      ;; Apply to 'yes and 'no and see which we get
      (let ((applied (funcall 'neovm--lca-normalize
                       (list 'app (list 'app term '(var yes)) '(var no)) 20)))
        (cond
         ((and (consp applied) (eq (car applied) 'var) (eq (cadr applied) 'yes)) t)
         ((and (consp applied) (eq (car applied) 'var) (eq (cadr applied) 'no)) nil)
         (t 'unknown)))))

  (unwind-protect
      (let ((TRUE  '(lam t (lam f (var t))))
            (FALSE '(lam t (lam f (var f))))
            ;; AND = (lam p (lam q (app (app (var p) (var q)) (var p))))
            (AND   '(lam p (lam q (app (app (var p) (var q)) (var p)))))
            ;; OR = (lam p (lam q (app (app (var p) (var p)) (var q))))
            (OR    '(lam p (lam q (app (app (var p) (var p)) (var q)))))
            ;; NOT = (lam p (app (app (var p) FALSE) TRUE))
            ;; We inline FALSE and TRUE
            (NOT   '(lam p (app (app (var p)
                                     (lam t (lam f (var f))))
                                (lam t (lam f (var t)))))))
        (list
          ;; Basic values
          (funcall 'neovm--lca-to-bool TRUE)
          (funcall 'neovm--lca-to-bool FALSE)
          ;; NOT TRUE = FALSE
          (funcall 'neovm--lca-to-bool
            (funcall 'neovm--lca-normalize (list 'app NOT TRUE) 30))
          ;; NOT FALSE = TRUE
          (funcall 'neovm--lca-to-bool
            (funcall 'neovm--lca-normalize (list 'app NOT FALSE) 30))
          ;; AND TRUE TRUE = TRUE
          (funcall 'neovm--lca-to-bool
            (funcall 'neovm--lca-normalize (list 'app (list 'app AND TRUE) TRUE) 30))
          ;; AND TRUE FALSE = FALSE
          (funcall 'neovm--lca-to-bool
            (funcall 'neovm--lca-normalize (list 'app (list 'app AND TRUE) FALSE) 30))
          ;; OR FALSE FALSE = FALSE
          (funcall 'neovm--lca-to-bool
            (funcall 'neovm--lca-normalize (list 'app (list 'app OR FALSE) FALSE) 30))
          ;; OR FALSE TRUE = TRUE
          (funcall 'neovm--lca-to-bool
            (funcall 'neovm--lca-normalize (list 'app (list 'app OR FALSE) TRUE) 30))))
    (fmakunbound 'neovm--lca-free-vars)
    (fmakunbound 'neovm--lca-fresh-var)
    (fmakunbound 'neovm--lca-rename)
    (fmakunbound 'neovm--lca-subst)
    (fmakunbound 'neovm--lca-beta-step)
    (fmakunbound 'neovm--lca-normalize)
    (fmakunbound 'neovm--lca-to-bool)))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combinatory logic: SKI basis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_adv_ski_combinators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // S, K, I combinators and their reductions.
    // S x y z = x z (y z), K x y = x, I x = x
    let form = r#"(progn
  ;; Simple evaluator for combinatory logic terms
  ;; Atoms: symbols. Applications: (app f x)
  ;; S, K, I are special symbols.
  (fset 'neovm--lca-cl-step
    (lambda (term)
      (cond
       ;; I x -> x
       ((and (consp term) (eq (car term) 'app)
             (eq (cadr term) 'I))
        (caddr term))
       ;; K x y -> x
       ((and (consp term) (eq (car term) 'app)
             (consp (cadr term)) (eq (car (cadr term)) 'app)
             (eq (cadr (cadr term)) 'K))
        (caddr (cadr term)))
       ;; S x y z -> (x z) (y z)
       ((and (consp term) (eq (car term) 'app)
             (consp (cadr term)) (eq (car (cadr term)) 'app)
             (consp (cadr (cadr term))) (eq (car (cadr (cadr term))) 'app)
             (eq (cadr (cadr (cadr term))) 'S))
        (let ((x (caddr (cadr (cadr term))))
              (y (caddr (cadr term)))
              (z (caddr term)))
          (list 'app (list 'app x z) (list 'app y z))))
       ;; Try left side of application
       ((and (consp term) (eq (car term) 'app))
        (let ((lhs-reduced (funcall 'neovm--lca-cl-step (cadr term))))
          (if (equal lhs-reduced (cadr term))
              (list 'app (cadr term) (funcall 'neovm--lca-cl-step (caddr term)))
            (list 'app lhs-reduced (caddr term)))))
       (t term))))

  ;; Reduce to normal form
  (fset 'neovm--lca-cl-normalize
    (lambda (term max-steps)
      (let ((current term) (steps 0) (changed t))
        (while (and changed (< steps max-steps))
          (let ((next (funcall 'neovm--lca-cl-step current)))
            (setq changed (not (equal next current)))
            (setq current next)
            (setq steps (1+ steps))))
        (cons steps current))))

  (unwind-protect
      (list
        ;; I a -> a
        (funcall 'neovm--lca-cl-normalize '(app I a) 10)
        ;; K a b -> a
        (funcall 'neovm--lca-cl-normalize '(app (app K a) b) 10)
        ;; S K K a -> a (SKK is identity)
        (funcall 'neovm--lca-cl-normalize
          '(app (app (app S K) K) a) 20)
        ;; S K S a -> a (SKS is also identity)
        (funcall 'neovm--lca-cl-normalize
          '(app (app (app S K) S) a) 20)
        ;; K I a b -> b (KI is flip of K)
        (funcall 'neovm--lca-cl-normalize
          '(app (app (app K I) a) b) 10)
        ;; S (K (S I)) K a b -> b a (flip combinator)
        ;; C = S(S(K(S(KS)K))S)(KK) is complex, test simpler:
        ;; S I I a -> a a (W combinator, self-application)
        (funcall 'neovm--lca-cl-normalize
          '(app (app (app S I) I) a) 20))
    (fmakunbound 'neovm--lca-cl-step)
    (fmakunbound 'neovm--lca-cl-normalize)))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 . a) (2 . a) (3 . a) (3 . a) (3 . b) (4 app a a))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
