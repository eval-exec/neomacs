//! Oracle parity tests for a lambda calculus interpreter in Elisp:
//! term representation as (var x), (abs x body), (app fn arg),
//! free variables computation, capture-avoiding substitution,
//! beta reduction (one step), normal-order evaluation,
//! Church numerals (encode/decode, successor, addition),
//! and Church booleans (true, false, if-then-else).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Free variables computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_free_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Terms: (var x), (abs x body), (app fn arg)
    // Free variables: variables not bound by any enclosing abs
    let form = r#"(progn
  (fset 'neovm--lc-free-vars
    (lambda (term)
      (cond
       ((eq (car term) 'var)
        (list (cadr term)))
       ((eq (car term) 'abs)
        (let ((param (cadr term))
              (body-fv (funcall 'neovm--lc-free-vars (caddr term))))
          (delq param (copy-sequence body-fv))))
       ((eq (car term) 'app)
        (let ((fn-fv (funcall 'neovm--lc-free-vars (cadr term)))
              (arg-fv (funcall 'neovm--lc-free-vars (caddr term))))
          ;; Union without duplicates
          (let ((result (copy-sequence fn-fv)))
            (dolist (v arg-fv)
              (unless (memq v result)
                (setq result (cons v result))))
            (sort result (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))
       (t nil))))

  (unwind-protect
      (list
       ;; Free variable in (var x) is {x}
       (funcall 'neovm--lc-free-vars '(var x))
       ;; Bound variable in (abs x (var x)) is {} (empty)
       (funcall 'neovm--lc-free-vars '(abs x (var x)))
       ;; (abs x (var y)) has free var {y}
       (funcall 'neovm--lc-free-vars '(abs x (var y)))
       ;; (app (var f) (var x)) has free vars {f, x}
       (funcall 'neovm--lc-free-vars '(app (var f) (var x)))
       ;; (abs x (app (var x) (var y))) has free var {y}
       (funcall 'neovm--lc-free-vars '(abs x (app (var x) (var y))))
       ;; Nested: (abs x (abs y (app (var x) (app (var y) (var z))))) has {z}
       (funcall 'neovm--lc-free-vars
                '(abs x (abs y (app (var x) (app (var y) (var z))))))
       ;; No free vars: (abs f (abs x (app (var f) (var x))))
       (funcall 'neovm--lc-free-vars
                '(abs f (abs x (app (var f) (var x)))))
       ;; Multiple free: (app (app (var a) (var b)) (app (var c) (var a)))
       (funcall 'neovm--lc-free-vars
                '(app (app (var a) (var b)) (app (var c) (var a)))))
    (fmakunbound 'neovm--lc-free-vars)))"#;
    let expect = expect_test::expect![[r#""OK ((x) nil (y) (f x) (y) (z) nil (a b c))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Capture-avoiding substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_substitution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lc-free-vars
    (lambda (term)
      (cond
       ((eq (car term) 'var) (list (cadr term)))
       ((eq (car term) 'abs)
        (delq (cadr term)
              (copy-sequence (funcall 'neovm--lc-free-vars (caddr term)))))
       ((eq (car term) 'app)
        (let ((result (copy-sequence (funcall 'neovm--lc-free-vars (cadr term)))))
          (dolist (v (funcall 'neovm--lc-free-vars (caddr term)))
            (unless (memq v result) (setq result (cons v result))))
          result))
       (t nil))))

  ;; Fresh variable generator
  (fset 'neovm--lc-fresh
    (lambda (base avoid)
      (let ((candidate base) (n 0))
        (while (memq candidate avoid)
          (setq n (1+ n))
          (setq candidate (intern (format "%s%d" (symbol-name base) n))))
        candidate)))

  ;; Capture-avoiding substitution: subst[x := s] in term
  (fset 'neovm--lc-subst
    (lambda (term x s)
      (cond
       ((eq (car term) 'var)
        (if (eq (cadr term) x) s term))
       ((eq (car term) 'abs)
        (let ((param (cadr term))
              (body (caddr term)))
          (cond
           ;; If param = x, x is shadowed, no substitution in body
           ((eq param x) term)
           ;; If param is free in s, rename param to avoid capture
           ((memq param (funcall 'neovm--lc-free-vars s))
            (let* ((all-vars (append (funcall 'neovm--lc-free-vars body)
                                    (funcall 'neovm--lc-free-vars s)
                                    (list x)))
                   (fresh (funcall 'neovm--lc-fresh param all-vars))
                   (renamed-body (funcall 'neovm--lc-subst body param
                                          (list 'var fresh))))
              (list 'abs fresh
                    (funcall 'neovm--lc-subst renamed-body x s))))
           ;; Otherwise safe to substitute directly
           (t (list 'abs param
                    (funcall 'neovm--lc-subst body x s))))))
       ((eq (car term) 'app)
        (list 'app
              (funcall 'neovm--lc-subst (cadr term) x s)
              (funcall 'neovm--lc-subst (caddr term) x s)))
       (t term))))

  (unwind-protect
      (list
       ;; Simple: subst x:=y in (var x) => (var y)
       (funcall 'neovm--lc-subst '(var x) 'x '(var y))
       ;; No-op: subst x:=y in (var z) => (var z)
       (funcall 'neovm--lc-subst '(var z) 'x '(var y))
       ;; Shadowing: subst x:=y in (abs x (var x)) => (abs x (var x))
       (funcall 'neovm--lc-subst '(abs x (var x)) 'x '(var y))
       ;; No capture needed: subst x:=y in (abs z (var x)) => (abs z (var y))
       (funcall 'neovm--lc-subst '(abs z (var x)) 'x '(var y))
       ;; Capture avoidance: subst x:=(var y) in (abs y (app (var x) (var y)))
       ;; y is free in replacement (var y), so param y must be renamed
       (funcall 'neovm--lc-subst '(abs y (app (var x) (var y)))
                'x '(var y))
       ;; In app: subst x:=z in (app (var x) (var x)) => (app (var z) (var z))
       (funcall 'neovm--lc-subst '(app (var x) (var x)) 'x '(var z))
       ;; Nested abs: subst x:=a in (abs y (abs z (var x)))
       (funcall 'neovm--lc-subst '(abs y (abs z (var x))) 'x '(var a)))
    (fmakunbound 'neovm--lc-free-vars)
    (fmakunbound 'neovm--lc-fresh)
    (fmakunbound 'neovm--lc-subst)))"#;
    let expect = expect_test::expect![[
        r#""OK ((var y) (var z) (abs x (var x)) (abs z (var y)) (abs y1 (app (var y) (var y1))) (app (var z) (var z)) (abs y (abs z (var a))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Beta reduction (one step)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_beta_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lc-free-vars
    (lambda (term)
      (cond
       ((eq (car term) 'var) (list (cadr term)))
       ((eq (car term) 'abs)
        (delq (cadr term)
              (copy-sequence (funcall 'neovm--lc-free-vars (caddr term)))))
       ((eq (car term) 'app)
        (let ((result (copy-sequence (funcall 'neovm--lc-free-vars (cadr term)))))
          (dolist (v (funcall 'neovm--lc-free-vars (caddr term)))
            (unless (memq v result) (setq result (cons v result))))
          result))
       (t nil))))

  (fset 'neovm--lc-fresh
    (lambda (base avoid)
      (let ((candidate base) (n 0))
        (while (memq candidate avoid)
          (setq n (1+ n))
          (setq candidate (intern (format "%s%d" (symbol-name base) n))))
        candidate)))

  (fset 'neovm--lc-subst
    (lambda (term x s)
      (cond
       ((eq (car term) 'var)
        (if (eq (cadr term) x) s term))
       ((eq (car term) 'abs)
        (let ((param (cadr term)) (body (caddr term)))
          (cond
           ((eq param x) term)
           ((memq param (funcall 'neovm--lc-free-vars s))
            (let* ((all-vars (append (funcall 'neovm--lc-free-vars body)
                                    (funcall 'neovm--lc-free-vars s)
                                    (list x)))
                   (fresh (funcall 'neovm--lc-fresh param all-vars))
                   (renamed (funcall 'neovm--lc-subst body param (list 'var fresh))))
              (list 'abs fresh (funcall 'neovm--lc-subst renamed x s))))
           (t (list 'abs param (funcall 'neovm--lc-subst body x s))))))
       ((eq (car term) 'app)
        (list 'app
              (funcall 'neovm--lc-subst (cadr term) x s)
              (funcall 'neovm--lc-subst (caddr term) x s)))
       (t term))))

  ;; One-step beta reduction: returns (reduced . new-term) or (not-reduced . term)
  (fset 'neovm--lc-beta-step
    (lambda (term)
      (cond
       ((eq (car term) 'var) (cons nil term))
       ((eq (car term) 'abs)
        (let ((result (funcall 'neovm--lc-beta-step (caddr term))))
          (if (car result)
              (cons t (list 'abs (cadr term) (cdr result)))
            (cons nil term))))
       ((eq (car term) 'app)
        ;; If function position is a lambda, do beta reduction
        (if (and (consp (cadr term)) (eq (car (cadr term)) 'abs))
            (let ((param (cadr (cadr term)))
                  (body (caddr (cadr term)))
                  (arg (caddr term)))
              (cons t (funcall 'neovm--lc-subst body param arg)))
          ;; Otherwise try reducing function first, then argument
          (let ((fn-result (funcall 'neovm--lc-beta-step (cadr term))))
            (if (car fn-result)
                (cons t (list 'app (cdr fn-result) (caddr term)))
              (let ((arg-result (funcall 'neovm--lc-beta-step (caddr term))))
                (if (car arg-result)
                    (cons t (list 'app (cadr term) (cdr arg-result)))
                  (cons nil term)))))))
       (t (cons nil term)))))

  (unwind-protect
      (list
       ;; ((\x. x) y) => y
       (funcall 'neovm--lc-beta-step
                '(app (abs x (var x)) (var y)))
       ;; ((\x. \y. x) a) => \y. a
       (funcall 'neovm--lc-beta-step
                '(app (abs x (abs y (var x))) (var a)))
       ;; No redex: (var x) => not reduced
       (funcall 'neovm--lc-beta-step '(var x))
       ;; Redex inside abs: (\z. (\x. x) y) => (\z. y)
       (funcall 'neovm--lc-beta-step
                '(abs z (app (abs x (var x)) (var y))))
       ;; Nested application: ((\x. x) (\y. y)) => (\y. y)
       (funcall 'neovm--lc-beta-step
                '(app (abs x (var x)) (abs y (var y))))
       ;; Application of non-lambda: (f x) => not reduced
       (funcall 'neovm--lc-beta-step
                '(app (var f) (var x))))
    (fmakunbound 'neovm--lc-free-vars)
    (fmakunbound 'neovm--lc-fresh)
    (fmakunbound 'neovm--lc-subst)
    (fmakunbound 'neovm--lc-beta-step)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t var y) (t abs y (var a)) (nil var x) (t abs z (var y)) (t abs y (var y)) (nil app (var f) (var x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Normal-order evaluation (leftmost-outermost, multi-step)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_normal_order_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--lc-free-vars
    (lambda (term)
      (cond
       ((eq (car term) 'var) (list (cadr term)))
       ((eq (car term) 'abs)
        (delq (cadr term)
              (copy-sequence (funcall 'neovm--lc-free-vars (caddr term)))))
       ((eq (car term) 'app)
        (let ((result (copy-sequence (funcall 'neovm--lc-free-vars (cadr term)))))
          (dolist (v (funcall 'neovm--lc-free-vars (caddr term)))
            (unless (memq v result) (setq result (cons v result))))
          result))
       (t nil))))

  (fset 'neovm--lc-fresh
    (lambda (base avoid)
      (let ((candidate base) (n 0))
        (while (memq candidate avoid)
          (setq n (1+ n))
          (setq candidate (intern (format "%s%d" (symbol-name base) n))))
        candidate)))

  (fset 'neovm--lc-subst
    (lambda (term x s)
      (cond
       ((eq (car term) 'var) (if (eq (cadr term) x) s term))
       ((eq (car term) 'abs)
        (let ((param (cadr term)) (body (caddr term)))
          (cond
           ((eq param x) term)
           ((memq param (funcall 'neovm--lc-free-vars s))
            (let* ((all-vars (append (funcall 'neovm--lc-free-vars body)
                                    (funcall 'neovm--lc-free-vars s) (list x)))
                   (fresh (funcall 'neovm--lc-fresh param all-vars))
                   (renamed (funcall 'neovm--lc-subst body param (list 'var fresh))))
              (list 'abs fresh (funcall 'neovm--lc-subst renamed x s))))
           (t (list 'abs param (funcall 'neovm--lc-subst body x s))))))
       ((eq (car term) 'app)
        (list 'app
              (funcall 'neovm--lc-subst (cadr term) x s)
              (funcall 'neovm--lc-subst (caddr term) x s)))
       (t term))))

  (fset 'neovm--lc-beta-step
    (lambda (term)
      (cond
       ((eq (car term) 'var) (cons nil term))
       ((eq (car term) 'abs)
        (let ((result (funcall 'neovm--lc-beta-step (caddr term))))
          (if (car result)
              (cons t (list 'abs (cadr term) (cdr result)))
            (cons nil term))))
       ((eq (car term) 'app)
        (if (and (consp (cadr term)) (eq (car (cadr term)) 'abs))
            (cons t (funcall 'neovm--lc-subst
                             (caddr (cadr term)) (cadr (cadr term)) (caddr term)))
          (let ((fn-r (funcall 'neovm--lc-beta-step (cadr term))))
            (if (car fn-r)
                (cons t (list 'app (cdr fn-r) (caddr term)))
              (let ((arg-r (funcall 'neovm--lc-beta-step (caddr term))))
                (if (car arg-r)
                    (cons t (list 'app (cadr term) (cdr arg-r)))
                  (cons nil term)))))))
       (t (cons nil term)))))

  ;; Evaluate to normal form with step limit
  (fset 'neovm--lc-eval
    (lambda (term max-steps)
      (let ((current term) (steps 0))
        (while (< steps max-steps)
          (let ((result (funcall 'neovm--lc-beta-step current)))
            (if (car result)
                (progn (setq current (cdr result))
                       (setq steps (1+ steps)))
              (setq steps max-steps))))
        (cons steps current))))

  (unwind-protect
      (list
       ;; Identity applied: ((\x. x) a) => a in 1 step
       (funcall 'neovm--lc-eval '(app (abs x (var x)) (var a)) 10)
       ;; Double application: ((\x. \y. x) a b) => a in 2 steps
       (funcall 'neovm--lc-eval
                '(app (app (abs x (abs y (var x))) (var a)) (var b)) 10)
       ;; Compose then apply: ((\f. \g. \x. f(g(x))) (\a. a) (\b. b)) c
       ;; Should reduce to c
       (funcall 'neovm--lc-eval
                '(app (app (app (abs f (abs g (abs x (app (var f) (app (var g) (var x))))))
                                (abs a (var a)))
                           (abs b (var b)))
                      (var c))
                20)
       ;; Self-application of identity: ((\x. x) (\y. y)) => (\y. y)
       (funcall 'neovm--lc-eval
                '(app (abs x (var x)) (abs y (var y))) 10)
       ;; Already in normal form: (abs x (var x))
       (funcall 'neovm--lc-eval '(abs x (var x)) 10))
    (fmakunbound 'neovm--lc-free-vars)
    (fmakunbound 'neovm--lc-fresh)
    (fmakunbound 'neovm--lc-subst)
    (fmakunbound 'neovm--lc-beta-step)
    (fmakunbound 'neovm--lc-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 var a) (10 var a) (20 var c) (10 abs y (var y)) (10 abs x (var x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church numerals via the interpreter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_church_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Helpers to build Church numeral ASTs
  (fset 'neovm--lc-church-num
    (lambda (n)
      ;; Church numeral n = \f. \x. f^n(x)
      (let ((body '(var x)))
        (dotimes (_ n)
          (setq body (list 'app '(var f) body)))
        (list 'abs 'f (list 'abs 'x body)))))

  ;; Decode: apply church numeral to (lambda (n) (+ n 1)) and 0
  ;; We do this symbolically: apply to successor-like term and count
  (fset 'neovm--lc-decode
    (lambda (church-term)
      ;; Count how many (app (var f) ...) wrappers around (var x)
      (if (and (eq (car church-term) 'abs)
               (eq (car (caddr church-term)) 'abs))
          (let ((body (caddr (caddr church-term)))
                (f-var (cadr church-term))
                (x-var (cadr (caddr church-term)))
                (count 0))
            (while (and (consp body) (eq (car body) 'app)
                        (consp (cadr body)) (eq (car (cadr body)) 'var)
                        (eq (cadr (cadr body)) f-var))
              (setq count (1+ count))
              (setq body (caddr body)))
            (if (and (consp body) (eq (car body) 'var)
                     (eq (cadr body) x-var))
                count
              'not-a-numeral))
        'not-a-numeral)))

  ;; Successor: \n. \f. \x. f (n f x)
  (fset 'neovm--lc-succ-term
    (lambda ()
      '(abs n (abs f (abs x
         (app (var f) (app (app (var n) (var f)) (var x))))))))

  ;; Addition: \m. \n. \f. \x. m f (n f x)
  (fset 'neovm--lc-add-term
    (lambda ()
      '(abs m (abs n (abs f (abs x
         (app (app (var m) (var f))
              (app (app (var n) (var f)) (var x)))))))))

  (unwind-protect
      (list
       ;; Encode and decode
       (funcall 'neovm--lc-decode (funcall 'neovm--lc-church-num 0))
       (funcall 'neovm--lc-decode (funcall 'neovm--lc-church-num 1))
       (funcall 'neovm--lc-decode (funcall 'neovm--lc-church-num 5))
       ;; Church numerals are structurally correct
       (funcall 'neovm--lc-church-num 0)
       (funcall 'neovm--lc-church-num 1)
       (funcall 'neovm--lc-church-num 2)
       (funcall 'neovm--lc-church-num 3)
       ;; Successor term structure
       (funcall 'neovm--lc-succ-term)
       ;; Addition term structure
       (funcall 'neovm--lc-add-term))
    (fmakunbound 'neovm--lc-church-num)
    (fmakunbound 'neovm--lc-decode)
    (fmakunbound 'neovm--lc-succ-term)
    (fmakunbound 'neovm--lc-add-term)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 1 5 (abs f (abs x (var x))) (abs f (abs x (app (var f) (var x)))) (abs f (abs x (app (var f) (app (var f) (var x))))) (abs f (abs x (app (var f) (app (var f) (app (var f) (var x)))))) (abs n (abs f (abs x (app (var f) (app (app (var n) (var f)) (var x)))))) (abs m (abs n (abs f (abs x (app (app (var m) (var f)) (app (app (var n) (var f)) (var x))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church booleans via the interpreter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lc_church_booleans() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Church booleans as lambda calculus AST terms
  ;; TRUE  = \t. \f. t
  ;; FALSE = \t. \f. f
  (fset 'neovm--lc-true  (lambda () '(abs t (abs f (var t)))))
  (fset 'neovm--lc-false (lambda () '(abs t (abs f (var f)))))

  ;; AND = \p. \q. p q p
  (fset 'neovm--lc-and
    (lambda ()
      '(abs p (abs q (app (app (var p) (var q)) (var p))))))

  ;; OR = \p. \q. p p q
  (fset 'neovm--lc-or
    (lambda ()
      '(abs p (abs q (app (app (var p) (var p)) (var q))))))

  ;; NOT = \p. p FALSE TRUE
  (fset 'neovm--lc-not
    (lambda ()
      (list 'abs 'p
            (list 'app
                  (list 'app '(var p) (funcall 'neovm--lc-false))
                  (funcall 'neovm--lc-true)))))

  ;; IF = \c. \a. \b. c a b
  (fset 'neovm--lc-if
    (lambda ()
      '(abs c (abs a (abs b (app (app (var c) (var a)) (var b)))))))

  ;; Decode: apply church bool to 'yes and 'no via term structure check
  (fset 'neovm--lc-decode-bool
    (lambda (term)
      ;; A normalized church bool is either (abs t (abs f (var t))) or (abs t (abs f (var f)))
      (if (and (eq (car term) 'abs)
               (eq (car (caddr term)) 'abs))
          (let ((t-param (cadr term))
                (f-param (cadr (caddr term)))
                (body (caddr (caddr term))))
            (cond
             ((and (eq (car body) 'var) (eq (cadr body) t-param)) 'true)
             ((and (eq (car body) 'var) (eq (cadr body) f-param)) 'false)
             (t 'unknown)))
        'not-a-bool)))

  (unwind-protect
      (list
       ;; Structure of TRUE and FALSE
       (funcall 'neovm--lc-true)
       (funcall 'neovm--lc-false)
       ;; Decode
       (funcall 'neovm--lc-decode-bool (funcall 'neovm--lc-true))
       (funcall 'neovm--lc-decode-bool (funcall 'neovm--lc-false))
       ;; AND structure
       (funcall 'neovm--lc-and)
       ;; OR structure
       (funcall 'neovm--lc-or)
       ;; NOT structure
       (funcall 'neovm--lc-not)
       ;; IF structure
       (funcall 'neovm--lc-if)
       ;; Build application: AND TRUE FALSE = (app (app AND TRUE) FALSE)
       (let ((and-term (funcall 'neovm--lc-and))
             (t-term (funcall 'neovm--lc-true))
             (f-term (funcall 'neovm--lc-false)))
         (list 'app (list 'app and-term t-term) f-term))
       ;; Build: NOT TRUE = (app NOT TRUE)
       (list 'app (funcall 'neovm--lc-not) (funcall 'neovm--lc-true))
       ;; Build: IF TRUE a b = (app (app (app IF TRUE) a) b)
       (let ((if-term (funcall 'neovm--lc-if))
             (t-term (funcall 'neovm--lc-true)))
         (list 'app
               (list 'app
                     (list 'app if-term t-term)
                     '(var a))
               '(var b))))
    (fmakunbound 'neovm--lc-true)
    (fmakunbound 'neovm--lc-false)
    (fmakunbound 'neovm--lc-and)
    (fmakunbound 'neovm--lc-or)
    (fmakunbound 'neovm--lc-not)
    (fmakunbound 'neovm--lc-if)
    (fmakunbound 'neovm--lc-decode-bool)))"#;
    let expect = expect_test::expect![[
        r#""OK ((abs t (abs f (var t))) (abs t (abs f (var f))) true false (abs p (abs q (app (app (var p) (var q)) (var p)))) (abs p (abs q (app (app (var p) (var p)) (var q)))) (abs p (app (app (var p) (abs t (abs f (var f)))) (abs t (abs f (var t))))) (abs c (abs a (abs b (app (app (var c) (var a)) (var b))))) (app (app (abs p (abs q (app (app (var p) (var q)) (var p)))) (abs t (abs f (var t)))) (abs t (abs f (var f)))) (app (abs p (app (app (var p) (abs t (abs f (var f)))) (abs t (abs f (var t))))) (abs t (abs f (var t)))) (app (app (app (abs c (abs a (abs b (app (app (var c) (var a)) (var b))))) (abs t (abs f (var t)))) (var a)) (var b)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
