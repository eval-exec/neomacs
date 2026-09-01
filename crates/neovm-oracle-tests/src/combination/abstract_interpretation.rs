//! Oracle parity tests for abstract interpretation in Elisp:
//! abstract domain (sign analysis: positive/negative/zero/top/bottom),
//! abstract arithmetic operations, transfer functions for assignments,
//! widening operator, fixed-point computation, and program analysis
//! for simple imperative programs.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Extended sign domain with interval narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abs_interp_extended_sign_domain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extended sign domain: bot < {neg, zero, pos, non-neg, non-pos, non-zero} < top
    // non-neg = zero | pos, non-pos = zero | neg, non-zero = neg | pos
    let form = r#"(progn
  ;; Extended join for richer domain
  (fset 'neovm--esd-join
    (lambda (a b)
      (cond
       ((eq a 'bot) b)
       ((eq b 'bot) a)
       ((eq a 'top) 'top)
       ((eq b 'top) 'top)
       ((eq a b) a)
       ;; zero + pos = non-neg
       ((or (and (eq a 'zero) (eq b 'pos)) (and (eq a 'pos) (eq b 'zero))) 'non-neg)
       ;; zero + neg = non-pos
       ((or (and (eq a 'zero) (eq b 'neg)) (and (eq a 'neg) (eq b 'zero))) 'non-pos)
       ;; neg + pos = non-zero
       ((or (and (eq a 'neg) (eq b 'pos)) (and (eq a 'pos) (eq b 'neg))) 'non-zero)
       ;; non-neg + neg = top, non-neg + non-pos = top, etc.
       ((or (and (eq a 'non-neg) (memq b '(neg non-pos non-zero)))
            (and (eq b 'non-neg) (memq a '(neg non-pos non-zero)))) 'top)
       ((or (and (eq a 'non-pos) (memq b '(pos non-neg non-zero)))
            (and (eq b 'non-pos) (memq a '(pos non-neg non-zero)))) 'top)
       ;; non-neg + zero = non-neg, non-neg + pos = non-neg
       ((or (and (eq a 'non-neg) (memq b '(zero pos)))
            (and (eq b 'non-neg) (memq a '(zero pos)))) 'non-neg)
       ;; non-pos + zero = non-pos, non-pos + neg = non-pos
       ((or (and (eq a 'non-pos) (memq b '(zero neg)))
            (and (eq b 'non-pos) (memq a '(zero neg)))) 'non-pos)
       ;; non-zero + pos = non-zero, non-zero + neg = non-zero
       ((or (and (eq a 'non-zero) (memq b '(pos neg)))
            (and (eq b 'non-zero) (memq a '(pos neg)))) 'non-zero)
       ;; non-zero + zero = top
       ((or (and (eq a 'non-zero) (eq b 'zero))
            (and (eq b 'non-zero) (eq a 'zero))) 'top)
       (t 'top))))

  ;; Extended meet
  (fset 'neovm--esd-meet
    (lambda (a b)
      (cond
       ((eq a 'top) b)
       ((eq b 'top) a)
       ((eq a 'bot) 'bot)
       ((eq b 'bot) 'bot)
       ((eq a b) a)
       ;; non-neg meet pos = pos, non-neg meet zero = zero, non-neg meet neg = bot
       ((or (and (eq a 'non-neg) (eq b 'pos)) (and (eq b 'non-neg) (eq a 'pos))) 'pos)
       ((or (and (eq a 'non-neg) (eq b 'zero)) (and (eq b 'non-neg) (eq a 'zero))) 'zero)
       ((or (and (eq a 'non-neg) (eq b 'neg)) (and (eq b 'non-neg) (eq a 'neg))) 'bot)
       ;; non-pos meet neg = neg, non-pos meet zero = zero, non-pos meet pos = bot
       ((or (and (eq a 'non-pos) (eq b 'neg)) (and (eq b 'non-pos) (eq a 'neg))) 'neg)
       ((or (and (eq a 'non-pos) (eq b 'zero)) (and (eq b 'non-pos) (eq a 'zero))) 'zero)
       ((or (and (eq a 'non-pos) (eq b 'pos)) (and (eq b 'non-pos) (eq a 'pos))) 'bot)
       ;; non-zero meet pos = pos, non-zero meet neg = neg, non-zero meet zero = bot
       ((or (and (eq a 'non-zero) (eq b 'pos)) (and (eq b 'non-zero) (eq a 'pos))) 'pos)
       ((or (and (eq a 'non-zero) (eq b 'neg)) (and (eq b 'non-zero) (eq a 'neg))) 'neg)
       ((or (and (eq a 'non-zero) (eq b 'zero)) (and (eq b 'non-zero) (eq a 'zero))) 'bot)
       ;; non-neg meet non-pos = zero
       ((or (and (eq a 'non-neg) (eq b 'non-pos)) (and (eq b 'non-neg) (eq a 'non-pos))) 'zero)
       ;; non-neg meet non-zero = pos
       ((or (and (eq a 'non-neg) (eq b 'non-zero)) (and (eq b 'non-neg) (eq a 'non-zero))) 'pos)
       ;; non-pos meet non-zero = neg
       ((or (and (eq a 'non-pos) (eq b 'non-zero)) (and (eq b 'non-pos) (eq a 'non-zero))) 'neg)
       ;; Different base signs
       ((and (memq a '(pos neg zero)) (memq b '(pos neg zero))) 'bot)
       (t 'bot))))

  (list
    ;; Join tests
    (funcall 'neovm--esd-join 'zero 'pos)      ;; non-neg
    (funcall 'neovm--esd-join 'zero 'neg)      ;; non-pos
    (funcall 'neovm--esd-join 'pos 'neg)       ;; non-zero
    (funcall 'neovm--esd-join 'non-neg 'neg)   ;; top
    (funcall 'neovm--esd-join 'non-neg 'zero)  ;; non-neg
    (funcall 'neovm--esd-join 'non-neg 'pos)   ;; non-neg
    (funcall 'neovm--esd-join 'bot 'non-zero)  ;; non-zero
    (funcall 'neovm--esd-join 'non-pos 'pos)   ;; top
    ;; Meet tests
    (funcall 'neovm--esd-meet 'non-neg 'pos)   ;; pos
    (funcall 'neovm--esd-meet 'non-neg 'zero)  ;; zero
    (funcall 'neovm--esd-meet 'non-neg 'neg)   ;; bot
    (funcall 'neovm--esd-meet 'non-neg 'non-pos) ;; zero
    (funcall 'neovm--esd-meet 'non-neg 'non-zero) ;; pos
    (funcall 'neovm--esd-meet 'non-pos 'non-zero) ;; neg
    (funcall 'neovm--esd-meet 'top 'non-zero)  ;; non-zero
    (funcall 'neovm--esd-meet 'bot 'non-neg)   ;; bot
    ;; Idempotence
    (funcall 'neovm--esd-join 'pos 'pos)       ;; pos
    (funcall 'neovm--esd-meet 'neg 'neg)       ;; neg
    ;; Absorption: join(a, meet(a,b)) = a
    (funcall 'neovm--esd-join 'non-neg (funcall 'neovm--esd-meet 'non-neg 'pos))))"#;
    let expect = expect_test::expect![[
        r#""OK (non-neg non-pos non-zero top non-neg non-neg non-zero top pos zero bot zero pos neg non-zero bot pos neg non-neg)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Abstract arithmetic with extended domain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abs_interp_extended_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Abstract add/mul/sub over the extended sign domain
    let form = r#"(progn
  ;; Abstract addition
  (fset 'neovm--ea-add
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((eq a 'zero) b)
       ((eq b 'zero) a)
       ((and (eq a 'pos) (eq b 'pos)) 'pos)
       ((and (eq a 'neg) (eq b 'neg)) 'neg)
       ((and (eq a 'pos) (eq b 'neg)) 'top)
       ((and (eq a 'neg) (eq b 'pos)) 'top)
       ;; non-neg + non-neg = non-neg
       ((and (memq a '(pos zero non-neg)) (memq b '(pos zero non-neg))) 'non-neg)
       ;; non-pos + non-pos = non-pos
       ((and (memq a '(neg zero non-pos)) (memq b '(neg zero non-pos))) 'non-pos)
       (t 'top))))

  ;; Abstract multiplication
  (fset 'neovm--ea-mul
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'zero) (eq b 'zero)) 'zero)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((and (eq a 'pos) (eq b 'pos)) 'pos)
       ((and (eq a 'neg) (eq b 'neg)) 'pos)
       ((and (eq a 'pos) (eq b 'neg)) 'neg)
       ((and (eq a 'neg) (eq b 'pos)) 'neg)
       ;; non-neg * non-neg = non-neg
       ((and (memq a '(pos zero non-neg)) (memq b '(pos zero non-neg))) 'non-neg)
       ;; non-neg * non-pos = non-pos
       ((or (and (memq a '(pos zero non-neg)) (memq b '(neg zero non-pos)))
            (and (memq a '(neg zero non-pos)) (memq b '(pos zero non-neg)))) 'non-pos)
       ;; non-pos * non-pos = non-neg
       ((and (memq a '(neg zero non-pos)) (memq b '(neg zero non-pos))) 'non-neg)
       ;; non-zero * non-zero = non-zero
       ((and (memq a '(pos neg non-zero)) (memq b '(pos neg non-zero))) 'non-zero)
       (t 'top))))

  ;; Abstract division (integer)
  (fset 'neovm--ea-div
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((eq b 'zero) 'bot)  ;; division by zero => bottom
       ((eq a 'zero) 'zero)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((and (eq a 'pos) (eq b 'pos)) 'non-neg)   ;; could be 0 if a < b
       ((and (eq a 'neg) (eq b 'neg)) 'non-neg)
       ((and (eq a 'pos) (eq b 'neg)) 'non-pos)
       ((and (eq a 'neg) (eq b 'pos)) 'non-pos)
       (t 'top))))

  (list
    ;; Addition
    (funcall 'neovm--ea-add 'pos 'pos)           ;; pos
    (funcall 'neovm--ea-add 'neg 'neg)           ;; neg
    (funcall 'neovm--ea-add 'pos 'neg)           ;; top
    (funcall 'neovm--ea-add 'non-neg 'non-neg)   ;; non-neg
    (funcall 'neovm--ea-add 'non-pos 'non-pos)   ;; non-pos
    (funcall 'neovm--ea-add 'zero 'non-neg)       ;; non-neg
    (funcall 'neovm--ea-add 'non-neg 'neg)        ;; top

    ;; Multiplication
    (funcall 'neovm--ea-mul 'pos 'pos)           ;; pos
    (funcall 'neovm--ea-mul 'neg 'neg)           ;; pos
    (funcall 'neovm--ea-mul 'pos 'neg)           ;; neg
    (funcall 'neovm--ea-mul 'zero 'top)          ;; zero
    (funcall 'neovm--ea-mul 'non-neg 'non-neg)   ;; non-neg
    (funcall 'neovm--ea-mul 'non-neg 'non-pos)   ;; non-pos
    (funcall 'neovm--ea-mul 'non-pos 'non-pos)   ;; non-neg
    (funcall 'neovm--ea-mul 'non-zero 'non-zero) ;; non-zero
    (funcall 'neovm--ea-mul 'bot 'pos)           ;; bot

    ;; Division
    (funcall 'neovm--ea-div 'pos 'pos)           ;; non-neg
    (funcall 'neovm--ea-div 'neg 'neg)           ;; non-neg
    (funcall 'neovm--ea-div 'pos 'neg)           ;; non-pos
    (funcall 'neovm--ea-div 'neg 'pos)           ;; non-pos
    (funcall 'neovm--ea-div 'zero 'pos)          ;; zero
    (funcall 'neovm--ea-div 'pos 'zero)          ;; bot (div by zero)
    (funcall 'neovm--ea-div 'bot 'pos)))"#;
    let expect = expect_test::expect![[
        r#""OK (pos neg top non-neg non-pos non-neg top pos pos neg zero non-neg non-pos non-neg non-zero bot non-neg non-neg non-pos non-pos zero bot bot)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transfer functions and abstract state operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abs_interp_transfer_functions_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Abstract interpreter for a simple imperative language:
    // statements are (assign var expr) or (if-pos var then-stmts else-stmts)
    let form = r#"(progn
  ;; State operations
  (fset 'neovm--ait-get
    (lambda (state var)
      (let ((p (assq var state))) (if p (cdr p) 'bot))))

  (fset 'neovm--ait-set
    (lambda (state var val)
      (cons (cons var val)
            (let ((r nil)) (dolist (p state) (unless (eq (car p) var) (setq r (cons p r)))) (nreverse r)))))

  ;; Join and add/mul/sub
  (fset 'neovm--ait-join
    (lambda (a b)
      (cond ((eq a 'bot) b) ((eq b 'bot) a) ((eq a 'top) 'top) ((eq b 'top) 'top)
            ((eq a b) a) (t 'top))))

  (fset 'neovm--ait-add
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq a 'zero) b) ((eq b 'zero) a)
            ((and (eq a 'pos) (eq b 'pos)) 'pos) ((and (eq a 'neg) (eq b 'neg)) 'neg) (t 'top))))

  (fset 'neovm--ait-mul
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'zero) (eq b 'zero)) 'zero)
            ((or (eq a 'top) (eq b 'top)) 'top)
            ((and (eq a 'pos) (eq b 'pos)) 'pos) ((and (eq a 'neg) (eq b 'neg)) 'pos)
            ((or (and (eq a 'pos) (eq b 'neg)) (and (eq a 'neg) (eq b 'pos))) 'neg) (t 'top))))

  (fset 'neovm--ait-sub
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq b 'zero) a)
            ((and (eq a 'zero) (eq b 'pos)) 'neg) ((and (eq a 'zero) (eq b 'neg)) 'pos)
            ((and (eq a 'pos) (eq b 'neg)) 'pos) ((and (eq a 'neg) (eq b 'pos)) 'neg) (t 'top))))

  ;; Evaluate abstract expression
  (fset 'neovm--ait-eval
    (lambda (state expr)
      (cond
       ((memq expr '(pos neg zero top bot)) expr)
       ((symbolp expr) (funcall 'neovm--ait-get state expr))
       ((and (listp expr) (eq (car expr) '+))
        (funcall 'neovm--ait-add (funcall 'neovm--ait-eval state (cadr expr))
                                  (funcall 'neovm--ait-eval state (caddr expr))))
       ((and (listp expr) (eq (car expr) '*))
        (funcall 'neovm--ait-mul (funcall 'neovm--ait-eval state (cadr expr))
                                  (funcall 'neovm--ait-eval state (caddr expr))))
       ((and (listp expr) (eq (car expr) '-))
        (funcall 'neovm--ait-sub (funcall 'neovm--ait-eval state (cadr expr))
                                  (funcall 'neovm--ait-eval state (caddr expr))))
       (t 'top))))

  ;; State join (point-wise)
  (fset 'neovm--ait-state-join
    (lambda (s1 s2)
      (let ((vars nil))
        (dolist (p s1) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p s2) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (let ((r nil))
          (dolist (v vars)
            (setq r (cons (cons v (funcall 'neovm--ait-join
                                           (funcall 'neovm--ait-get s1 v)
                                           (funcall 'neovm--ait-get s2 v)))
                          r)))
          r))))

  ;; Execute statements: (assign var expr) or (if-pos var then-stmts else-stmts)
  (fset 'neovm--ait-exec
    (lambda (state stmts)
      (dolist (s stmts)
        (cond
         ((eq (car s) 'assign)
          (setq state (funcall 'neovm--ait-set state (cadr s)
                               (funcall 'neovm--ait-eval state (caddr s)))))
         ((eq (car s) 'if-pos)
          ;; If var is positive: run then-stmts, else: run else-stmts
          ;; Abstract: join both branches
          (let* ((var-val (funcall 'neovm--ait-get state (cadr s)))
                 (then-reachable (memq var-val '(pos top)))
                 (else-reachable (memq var-val '(neg zero top)))
                 (then-state (if then-reachable
                                 (funcall 'neovm--ait-exec
                                          (funcall 'neovm--ait-set state (cadr s) 'pos)
                                          (caddr s))
                               nil))
                 (else-state (if else-reachable
                                 (funcall 'neovm--ait-exec state (cadddr s))
                               nil)))
            (cond
             ((and then-state else-state) (setq state (funcall 'neovm--ait-state-join then-state else-state)))
             (then-state (setq state then-state))
             (else-state (setq state else-state)))))))
      state))

  ;; Test program:
  ;; a = pos; b = neg; c = a + b;
  ;; if a > 0: d = a * a; e = b * b
  ;; else: d = zero; e = zero
  ;; f = d + e
  (let ((final (funcall 'neovm--ait-exec nil
                '((assign a pos)
                  (assign b neg)
                  (assign c (+ a b))
                  (if-pos a
                    ((assign d (* a a)) (assign e (* b b)))
                    ((assign d zero) (assign e zero)))
                  (assign f (+ d e))))))
    (list
      (funcall 'neovm--ait-get final 'a)   ;; pos (refined in then-branch, joined)
      (funcall 'neovm--ait-get final 'b)   ;; neg
      (funcall 'neovm--ait-get final 'c)   ;; top (pos + neg)
      (funcall 'neovm--ait-get final 'd)   ;; pos (then: pos*pos=pos, else unreachable since a=pos)
      (funcall 'neovm--ait-get final 'e)   ;; pos (then: neg*neg=pos)
      (funcall 'neovm--ait-get final 'f)   ;; pos (pos + pos)
      ;; Second test: unknown input
      (let ((final2 (funcall 'neovm--ait-exec nil
                      '((assign x top)
                        (if-pos x
                          ((assign y pos))
                          ((assign y neg)))
                        (assign z (* y y))))))
        (list
          (funcall 'neovm--ait-get final2 'x)   ;; top
          (funcall 'neovm--ait-get final2 'y)   ;; top (join pos and neg)
          (funcall 'neovm--ait-get final2 'z))))))"#;
    let expect = expect_test::expect![[r#""OK (pos neg top pos pos pos (top top top))""#]];
    // top (top * top)
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Widening operator and fixed-point computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abs_interp_widening_fixpoint() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Fixed-point iteration with widening for loop analysis.
    // Analyze multiple loop patterns.
    let form = r#"(progn
  ;; Lattice ops
  (fset 'neovm--aiw-join
    (lambda (a b)
      (cond ((eq a 'bot) b) ((eq b 'bot) a) ((eq a 'top) 'top) ((eq b 'top) 'top)
            ((eq a b) a) (t 'top))))

  (fset 'neovm--aiw-get
    (lambda (s v) (let ((p (assq v s))) (if p (cdr p) 'bot))))

  (fset 'neovm--aiw-set
    (lambda (s v val) (cons (cons v val) (let ((r nil)) (dolist (p s) (unless (eq (car p) v) (setq r (cons p r)))) (nreverse r)))))

  (fset 'neovm--aiw-add
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq a 'zero) b) ((eq b 'zero) a)
            ((and (eq a 'pos) (eq b 'pos)) 'pos) ((and (eq a 'neg) (eq b 'neg)) 'neg) (t 'top))))

  (fset 'neovm--aiw-sub
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq b 'zero) a)
            ((and (eq a 'zero) (eq b 'pos)) 'neg) ((and (eq a 'zero) (eq b 'neg)) 'pos)
            ((and (eq a 'pos) (eq b 'neg)) 'pos) ((and (eq a 'neg) (eq b 'pos)) 'neg) (t 'top))))

  (fset 'neovm--aiw-state-join
    (lambda (s1 s2)
      (let ((vars nil))
        (dolist (p s1) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p s2) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (let ((r nil))
          (dolist (v vars)
            (setq r (cons (cons v (funcall 'neovm--aiw-join
                                           (funcall 'neovm--aiw-get s1 v)
                                           (funcall 'neovm--aiw-get s2 v))) r)))
          r))))

  (fset 'neovm--aiw-state-eq
    (lambda (s1 s2)
      (let ((vars nil) (eq-p t))
        (dolist (p s1) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p s2) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (v vars)
          (unless (eq (funcall 'neovm--aiw-get s1 v) (funcall 'neovm--aiw-get s2 v))
            (setq eq-p nil)))
        eq-p)))

  ;; Widening: if old != new for any var, widen to top
  (fset 'neovm--aiw-widen
    (lambda (old new-state)
      (let ((vars nil))
        (dolist (p old) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p new-state) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (let ((r nil))
          (dolist (v vars)
            (let ((ov (funcall 'neovm--aiw-get old v))
                  (nv (funcall 'neovm--aiw-get new-state v)))
              (setq r (cons (cons v (if (eq ov nv) ov 'top)) r))))
          r))))

  ;; Analyze a loop: init-state, body-fn (state -> state), max-iters
  (fset 'neovm--aiw-analyze-loop
    (lambda (init body-fn max-iters)
      (let ((loop-state init) (converged nil) (iters 0))
        (while (and (not converged) (< iters max-iters))
          (setq iters (1+ iters))
          (let* ((body-state (funcall body-fn loop-state))
                 (joined (funcall 'neovm--aiw-state-join init body-state))
                 (widened (funcall 'neovm--aiw-widen loop-state joined)))
            (if (funcall 'neovm--aiw-state-eq widened loop-state)
                (setq converged t)
              (setq loop-state widened))))
        (list 'state loop-state 'converged converged 'iterations iters))))

  (list
    ;; Loop 1: x = 0; while: x = x + 1
    (funcall 'neovm--aiw-analyze-loop
      (list (cons 'x 'zero))
      (lambda (s) (funcall 'neovm--aiw-set s 'x (funcall 'neovm--aiw-add (funcall 'neovm--aiw-get s 'x) 'pos)))
      20)

    ;; Loop 2: x = pos; while: x = x - 1 (could go negative)
    (funcall 'neovm--aiw-analyze-loop
      (list (cons 'x 'pos))
      (lambda (s) (funcall 'neovm--aiw-set s 'x (funcall 'neovm--aiw-sub (funcall 'neovm--aiw-get s 'x) 'pos)))
      20)

    ;; Loop 3: x = pos, y = neg; while: x = x + y (stays top)
    (funcall 'neovm--aiw-analyze-loop
      (list (cons 'x 'pos) (cons 'y 'neg))
      (lambda (s) (funcall 'neovm--aiw-set s 'x (funcall 'neovm--aiw-add (funcall 'neovm--aiw-get s 'x) (funcall 'neovm--aiw-get s 'y))))
      20)

    ;; Loop 4: x = pos, y = pos; while: x = x + y (always pos + pos = pos)
    (funcall 'neovm--aiw-analyze-loop
      (list (cons 'x 'pos) (cons 'y 'pos))
      (lambda (s) (funcall 'neovm--aiw-set s 'x (funcall 'neovm--aiw-add (funcall 'neovm--aiw-get s 'x) (funcall 'neovm--aiw-get s 'y))))
      20)))"#;
    let expect = expect_test::expect![[
        r#""OK ((state ((x . top)) converged t iterations 2) (state ((x . top)) converged t iterations 2) (state ((x . top) (y . neg)) converged t iterations 2) (state ((x . pos) (y . pos)) converged t iterations 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complete program analysis: multi-block CFG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abs_interp_cfg_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze a multi-block control flow graph with forward abstract interpretation.
    let form = r#"(progn
  ;; Lattice and state ops
  (fset 'neovm--aicfg-join
    (lambda (a b)
      (cond ((eq a 'bot) b) ((eq b 'bot) a) ((eq a 'top) 'top) ((eq b 'top) 'top) ((eq a b) a) (t 'top))))

  (fset 'neovm--aicfg-get
    (lambda (s v) (let ((p (assq v s))) (if p (cdr p) 'bot))))

  (fset 'neovm--aicfg-set
    (lambda (s v val) (cons (cons v val) (let ((r nil)) (dolist (p s) (unless (eq (car p) v) (setq r (cons p r)))) (nreverse r)))))

  (fset 'neovm--aicfg-add
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq a 'zero) b) ((eq b 'zero) a)
            ((and (eq a 'pos) (eq b 'pos)) 'pos) ((and (eq a 'neg) (eq b 'neg)) 'neg) (t 'top))))

  (fset 'neovm--aicfg-mul
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'zero) (eq b 'zero)) 'zero)
            ((or (eq a 'top) (eq b 'top)) 'top)
            ((and (eq a 'pos) (eq b 'pos)) 'pos) ((and (eq a 'neg) (eq b 'neg)) 'pos)
            ((or (and (eq a 'pos) (eq b 'neg)) (and (eq a 'neg) (eq b 'pos))) 'neg) (t 'top))))

  (fset 'neovm--aicfg-eval
    (lambda (s expr)
      (cond
       ((memq expr '(pos neg zero top bot)) expr)
       ((symbolp expr) (funcall 'neovm--aicfg-get s expr))
       ((and (listp expr) (eq (car expr) '+))
        (funcall 'neovm--aicfg-add (funcall 'neovm--aicfg-eval s (cadr expr)) (funcall 'neovm--aicfg-eval s (caddr expr))))
       ((and (listp expr) (eq (car expr) '*))
        (funcall 'neovm--aicfg-mul (funcall 'neovm--aicfg-eval s (cadr expr)) (funcall 'neovm--aicfg-eval s (caddr expr))))
       (t 'top))))

  (fset 'neovm--aicfg-state-join
    (lambda (s1 s2)
      (let ((vars nil))
        (dolist (p s1) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p s2) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (let ((r nil))
          (dolist (v vars)
            (setq r (cons (cons v (funcall 'neovm--aicfg-join
                                           (funcall 'neovm--aicfg-get s1 v)
                                           (funcall 'neovm--aicfg-get s2 v))) r)))
          r))))

  (fset 'neovm--aicfg-state-eq
    (lambda (s1 s2)
      (let ((vars nil) (res t))
        (dolist (p s1) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p s2) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (v vars)
          (unless (eq (funcall 'neovm--aicfg-get s1 v) (funcall 'neovm--aicfg-get s2 v))
            (setq res nil)))
        res)))

  ;; Execute a block's statements
  (fset 'neovm--aicfg-exec-block
    (lambda (state stmts)
      (dolist (s stmts)
        (when (eq (car s) 'assign)
          (setq state (funcall 'neovm--aicfg-set state (cadr s) (funcall 'neovm--aicfg-eval state (caddr s))))))
      state))

  ;; CFG analysis with worklist algorithm
  ;; blocks: list of (label stmts successors)
  ;; Returns: alist of (label . final-state)
  (fset 'neovm--aicfg-analyze
    (lambda (blocks init-state entry-label)
      ;; Initialize: IN[entry] = init-state, IN[others] = nil
      (let ((in-map (list (cons entry-label init-state)))
            (out-map nil)
            (worklist (list entry-label))
            (max-iters 50)
            (iters 0))
        ;; Process worklist
        (while (and worklist (< iters max-iters))
          (setq iters (1+ iters))
          (let* ((label (car worklist))
                 (block (let ((b nil)) (dolist (bl blocks) (when (eq (car bl) label) (setq b bl))) b))
                 (in-state (cdr (assq label in-map)))
                 (out-state (if in-state (funcall 'neovm--aicfg-exec-block in-state (cadr block)) nil)))
            (setq worklist (cdr worklist))
            ;; Update OUT[label]
            (let ((old-out (cdr (assq label out-map))))
              (setq out-map (cons (cons label out-state)
                                  (let ((r nil)) (dolist (p out-map) (unless (eq (car p) label) (setq r (cons p r)))) (nreverse r))))
              ;; For each successor, update IN and add to worklist if changed
              (dolist (succ (caddr block))
                (let* ((old-in (cdr (assq succ in-map)))
                       (new-in (if old-in (funcall 'neovm--aicfg-state-join old-in out-state) out-state)))
                  (unless (and old-in (funcall 'neovm--aicfg-state-eq old-in new-in))
                    (setq in-map (cons (cons succ new-in)
                                       (let ((r nil)) (dolist (p in-map) (unless (eq (car p) succ) (setq r (cons p r)))) (nreverse r))))
                    (unless (memq succ worklist)
                      (setq worklist (append worklist (list succ))))))))))
        ;; Return final OUT states
        (let ((result nil))
          (dolist (block blocks)
            (let ((label (car block)))
              (setq result (cons (cons label (cdr (assq label out-map))) result))))
          (list 'results (nreverse result) 'iterations iters)))))

  ;; Test CFG:
  ;; B1: x = pos, y = neg -> B2
  ;; B2: z = x + y -> B3
  ;; B3: w = z * z -> B4
  ;; B4: (end)
  (let ((r1 (funcall 'neovm--aicfg-analyze
              '((B1 ((assign x pos) (assign y neg)) (B2))
                (B2 ((assign z (+ x y))) (B3))
                (B3 ((assign w (* z z))) (B4))
                (B4 () ()))
              nil
              'B1)))
    ;; Extract final values at B4
    (let ((b4-state (cdr (assq 'B4 (cadr r1)))))
      (list
        (funcall 'neovm--aicfg-get b4-state 'x)   ;; pos
        (funcall 'neovm--aicfg-get b4-state 'y)   ;; neg
        (funcall 'neovm--aicfg-get b4-state 'z)   ;; top (pos + neg)
        (funcall 'neovm--aicfg-get b4-state 'w)   ;; top (top * top)
        (caddr r1)  ;; iterations count
        ;; Second test: diamond CFG
        ;; B1: x = pos -> B2, B3
        ;; B2: y = pos -> B4
        ;; B3: y = neg -> B4
        ;; B4: z = x + y
        (let ((r2 (funcall 'neovm--aicfg-analyze
                    '((B1 ((assign x pos)) (B2 B3))
                      (B2 ((assign y pos)) (B4))
                      (B3 ((assign y neg)) (B4))
                      (B4 ((assign z (+ x y))) ()))
                    nil
                    'B1)))
          (let ((b4-2 (cdr (assq 'B4 (cadr r2)))))
            (list
              (funcall 'neovm--aicfg-get b4-2 'x)  ;; pos
              (funcall 'neovm--aicfg-get b4-2 'y)  ;; top (join pos neg)
              (funcall 'neovm--aicfg-get b4-2 'z)  ;; top (pos + top)
              )))))))"#;
    let expect = expect_test::expect![[r#""OK (bot bot bot bot iterations (bot bot bot))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Abstract interpretation of a real-ish program: factorial sign analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abs_interp_factorial_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze the sign of factorial(n) for different abstract inputs.
    // Also analyze Fibonacci sign patterns.
    let form = r#"(progn
  (fset 'neovm--aif-join
    (lambda (a b) (cond ((eq a 'bot) b) ((eq b 'bot) a) ((eq a 'top) 'top) ((eq b 'top) 'top) ((eq a b) a) (t 'top))))
  (fset 'neovm--aif-mul
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'zero) (eq b 'zero)) 'zero)
            ((or (eq a 'top) (eq b 'top)) 'top)
            ((and (eq a 'pos) (eq b 'pos)) 'pos) ((and (eq a 'neg) (eq b 'neg)) 'pos)
            ((or (and (eq a 'pos) (eq b 'neg)) (and (eq a 'neg) (eq b 'pos))) 'neg) (t 'top))))
  (fset 'neovm--aif-add
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq a 'zero) b) ((eq b 'zero) a)
            ((and (eq a 'pos) (eq b 'pos)) 'pos) ((and (eq a 'neg) (eq b 'neg)) 'neg) (t 'top))))
  (fset 'neovm--aif-sub
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq b 'zero) a)
            ((and (eq a 'pos) (eq b 'neg)) 'pos) ((and (eq a 'neg) (eq b 'pos)) 'neg) (t 'top))))

  ;; Abstract factorial: fact(n-sign) computes sign of n!
  ;; fact(zero) = 1 (pos)
  ;; fact(pos) = pos (product of positives)
  ;; fact(neg) = bot (undefined for negative)
  ;; fact(top) = top (could be anything)
  (fset 'neovm--aif-factorial
    (lambda (n-sign)
      (cond
       ((eq n-sign 'bot) 'bot)
       ((eq n-sign 'zero) 'pos)     ;; 0! = 1
       ((eq n-sign 'pos) 'pos)      ;; n! for n>0 is positive
       ((eq n-sign 'neg) 'bot)      ;; undefined
       ((eq n-sign 'top) 'top))))   ;; unknown

  ;; Abstract Fibonacci: fib(n-sign)
  ;; fib(zero) = zero (fib(0)=0)
  ;; fib(pos) = pos (all fib(n) for n>0 are positive...except fib(1)=1, fib(2)=1, all pos)
  ;; Actually: fib(0)=0, fib(1)=1, fib(n)=fib(n-1)+fib(n-2)
  ;; So fib is always non-negative for non-negative input
  (fset 'neovm--aif-fibonacci
    (lambda (n-sign)
      (cond
       ((eq n-sign 'bot) 'bot)
       ((eq n-sign 'zero) 'zero)    ;; fib(0) = 0
       ((eq n-sign 'pos) 'pos)      ;; fib(n) > 0 for n > 0
       ((eq n-sign 'neg) 'bot)
       ((eq n-sign 'top) 'top))))

  ;; Analyze a sequence of computations
  ;; a = pos input
  ;; b = fact(a)        -> pos
  ;; c = fib(a)         -> pos
  ;; d = b * c          -> pos
  ;; e = fact(zero)     -> pos
  ;; f = fib(zero)      -> zero
  ;; g = e * f          -> zero
  ;; h = fact(neg)      -> bot
  ;; i = fact(top)      -> top
  ;; j = d + g          -> pos (pos + zero)
  ;; k = b - c          -> top (pos - pos)
  (let ((a 'pos)
        (b (funcall 'neovm--aif-factorial 'pos))
        (c (funcall 'neovm--aif-fibonacci 'pos))
        (e (funcall 'neovm--aif-factorial 'zero))
        (f (funcall 'neovm--aif-fibonacci 'zero))
        (h (funcall 'neovm--aif-factorial 'neg))
        (i (funcall 'neovm--aif-factorial 'top)))
    (let ((d (funcall 'neovm--aif-mul b c))
          (g (funcall 'neovm--aif-mul e f)))
      (let ((j (funcall 'neovm--aif-add d g))
            (k (funcall 'neovm--aif-sub b c)))
        (list
          a b c d e f g h i j k
          ;; Verify some algebraic properties
          ;; fact(pos) * fact(pos) = pos
          (funcall 'neovm--aif-mul (funcall 'neovm--aif-factorial 'pos) (funcall 'neovm--aif-factorial 'pos))
          ;; fib(pos) + fib(pos) = pos
          (funcall 'neovm--aif-add (funcall 'neovm--aif-fibonacci 'pos) (funcall 'neovm--aif-fibonacci 'pos))
          ;; fact(zero) + fib(zero) = pos (1 + 0 = 1)
          (funcall 'neovm--aif-add (funcall 'neovm--aif-factorial 'zero) (funcall 'neovm--aif-fibonacci 'zero)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (pos pos pos pos pos zero zero bot top pos top pos pos pos)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
