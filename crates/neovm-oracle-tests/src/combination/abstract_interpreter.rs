//! Oracle parity tests for an abstract interpreter implementing sign-domain
//! abstract interpretation in Elisp. Covers abstract domains (pos/neg/zero/top/bot),
//! abstract arithmetic, abstract comparison, transfer functions for assignments,
//! abstract interpretation of loops with widening, and reaching definitions analysis.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Abstract domain: sign lattice and lattice operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_interp_sign_domain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sign domain: bot < {neg, zero, pos} < top
    // Test join (least upper bound) and meet (greatest lower bound)
    let form = r#"(progn
  ;; Abstract values: 'bot, 'neg, 'zero, 'pos, 'top
  ;; Lattice order: bot <= neg,zero,pos <= top

  ;; Join (least upper bound)
  (fset 'neovm--abs-join
    (lambda (a b)
      (cond
       ((eq a 'bot) b)
       ((eq b 'bot) a)
       ((eq a 'top) 'top)
       ((eq b 'top) 'top)
       ((eq a b) a)
       (t 'top))))

  ;; Meet (greatest lower bound)
  (fset 'neovm--abs-meet
    (lambda (a b)
      (cond
       ((eq a 'top) b)
       ((eq b 'top) a)
       ((eq a 'bot) 'bot)
       ((eq b 'bot) 'bot)
       ((eq a b) a)
       (t 'bot))))

  ;; Less-or-equal in lattice
  (fset 'neovm--abs-leq
    (lambda (a b)
      (eq (funcall 'neovm--abs-join a b) b)))

  (list
    ;; Join tests - exhaustive
    (funcall 'neovm--abs-join 'bot 'pos)
    (funcall 'neovm--abs-join 'pos 'bot)
    (funcall 'neovm--abs-join 'bot 'bot)
    (funcall 'neovm--abs-join 'top 'pos)
    (funcall 'neovm--abs-join 'neg 'top)
    (funcall 'neovm--abs-join 'pos 'pos)
    (funcall 'neovm--abs-join 'neg 'neg)
    (funcall 'neovm--abs-join 'zero 'zero)
    (funcall 'neovm--abs-join 'pos 'neg)
    (funcall 'neovm--abs-join 'pos 'zero)
    (funcall 'neovm--abs-join 'neg 'zero)
    (funcall 'neovm--abs-join 'top 'top)

    ;; Meet tests - exhaustive
    (funcall 'neovm--abs-meet 'top 'pos)
    (funcall 'neovm--abs-meet 'pos 'top)
    (funcall 'neovm--abs-meet 'bot 'pos)
    (funcall 'neovm--abs-meet 'pos 'bot)
    (funcall 'neovm--abs-meet 'pos 'neg)
    (funcall 'neovm--abs-meet 'pos 'zero)
    (funcall 'neovm--abs-meet 'neg 'zero)
    (funcall 'neovm--abs-meet 'pos 'pos)
    (funcall 'neovm--abs-meet 'top 'top)
    (funcall 'neovm--abs-meet 'bot 'bot)

    ;; Leq tests
    (funcall 'neovm--abs-leq 'bot 'pos)
    (funcall 'neovm--abs-leq 'bot 'top)
    (funcall 'neovm--abs-leq 'pos 'top)
    (funcall 'neovm--abs-leq 'pos 'pos)
    (funcall 'neovm--abs-leq 'pos 'neg)
    (funcall 'neovm--abs-leq 'top 'pos)
    (funcall 'neovm--abs-leq 'bot 'bot)
    (funcall 'neovm--abs-leq 'top 'top)))"#;
    let expect = expect_test::expect![[
        r#""OK (pos pos bot top top pos neg zero top top top top pos pos bot bot bot bot bot pos top bot t t t t nil nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Abstract arithmetic: add, sub, mul, div
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_interp_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Abstract addition, subtraction, multiplication over sign domain
    let form = r#"(progn
  ;; Abstract addition
  (fset 'neovm--abs-add
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((eq a 'zero) b)
       ((eq b 'zero) a)
       ((and (eq a 'pos) (eq b 'pos)) 'pos)
       ((and (eq a 'neg) (eq b 'neg)) 'neg)
       ;; pos + neg or neg + pos => top (could be anything)
       (t 'top))))

  ;; Abstract subtraction
  (fset 'neovm--abs-sub
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((eq b 'zero) a)
       ((and (eq a 'zero) (eq b 'pos)) 'neg)
       ((and (eq a 'zero) (eq b 'neg)) 'pos)
       ((and (eq a 'pos) (eq b 'neg)) 'pos)
       ((and (eq a 'neg) (eq b 'pos)) 'neg)
       ;; pos - pos or neg - neg => top
       (t 'top))))

  ;; Abstract multiplication
  (fset 'neovm--abs-mul
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'zero) (eq b 'zero)) 'zero)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((and (eq a 'pos) (eq b 'pos)) 'pos)
       ((and (eq a 'neg) (eq b 'neg)) 'pos)
       ((and (eq a 'pos) (eq b 'neg)) 'neg)
       ((and (eq a 'neg) (eq b 'pos)) 'neg)
       (t 'top))))

  ;; Abstract negation
  (fset 'neovm--abs-neg
    (lambda (a)
      (cond
       ((eq a 'bot) 'bot)
       ((eq a 'top) 'top)
       ((eq a 'zero) 'zero)
       ((eq a 'pos) 'neg)
       ((eq a 'neg) 'pos))))

  (list
    ;; Addition truth table
    (funcall 'neovm--abs-add 'pos 'pos)     ;; pos
    (funcall 'neovm--abs-add 'neg 'neg)     ;; neg
    (funcall 'neovm--abs-add 'pos 'neg)     ;; top
    (funcall 'neovm--abs-add 'neg 'pos)     ;; top
    (funcall 'neovm--abs-add 'zero 'pos)    ;; pos
    (funcall 'neovm--abs-add 'pos 'zero)    ;; pos
    (funcall 'neovm--abs-add 'zero 'zero)   ;; zero
    (funcall 'neovm--abs-add 'bot 'pos)     ;; bot
    (funcall 'neovm--abs-add 'top 'pos)     ;; top

    ;; Subtraction
    (funcall 'neovm--abs-sub 'pos 'neg)     ;; pos
    (funcall 'neovm--abs-sub 'neg 'pos)     ;; neg
    (funcall 'neovm--abs-sub 'pos 'pos)     ;; top
    (funcall 'neovm--abs-sub 'neg 'neg)     ;; top
    (funcall 'neovm--abs-sub 'zero 'pos)    ;; neg
    (funcall 'neovm--abs-sub 'zero 'neg)    ;; pos

    ;; Multiplication
    (funcall 'neovm--abs-mul 'pos 'pos)     ;; pos
    (funcall 'neovm--abs-mul 'neg 'neg)     ;; pos
    (funcall 'neovm--abs-mul 'pos 'neg)     ;; neg
    (funcall 'neovm--abs-mul 'neg 'pos)     ;; neg
    (funcall 'neovm--abs-mul 'zero 'pos)    ;; zero
    (funcall 'neovm--abs-mul 'zero 'neg)    ;; zero
    (funcall 'neovm--abs-mul 'top 'pos)     ;; top
    (funcall 'neovm--abs-mul 'bot 'neg)     ;; bot

    ;; Negation
    (funcall 'neovm--abs-neg 'pos)
    (funcall 'neovm--abs-neg 'neg)
    (funcall 'neovm--abs-neg 'zero)
    (funcall 'neovm--abs-neg 'top)
    (funcall 'neovm--abs-neg 'bot)))"#;
    let expect = expect_test::expect![[
        r#""OK (pos neg top top pos pos zero bot top pos neg top top neg pos pos pos neg neg zero zero top bot neg pos zero top bot)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Abstract comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_interp_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Abstract comparison operators: <, >, <=, >=, =, /=
    // Returns 'true, 'false, or 'maybe
    let form = r#"(progn
  ;; Abstract less-than
  (fset 'neovm--abs-lt
    (lambda (a b)
      "Is a < b?"
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'maybe)
       ;; neg < pos is always true
       ((and (eq a 'neg) (eq b 'pos)) 'true)
       ;; pos < neg is always false
       ((and (eq a 'pos) (eq b 'neg)) 'false)
       ;; zero < pos
       ((and (eq a 'zero) (eq b 'pos)) 'true)
       ;; neg < zero
       ((and (eq a 'neg) (eq b 'zero)) 'true)
       ;; zero < neg
       ((and (eq a 'zero) (eq b 'neg)) 'false)
       ;; pos < zero
       ((and (eq a 'pos) (eq b 'zero)) 'false)
       ;; zero < zero is false
       ((and (eq a 'zero) (eq b 'zero)) 'false)
       ;; pos < pos or neg < neg => could be either
       (t 'maybe))))

  ;; Abstract greater-than (symmetric)
  (fset 'neovm--abs-gt
    (lambda (a b) (funcall 'neovm--abs-lt b a)))

  ;; Abstract equals
  (fset 'neovm--abs-eq
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'maybe)
       ((and (eq a 'zero) (eq b 'zero)) 'true)
       ;; Different concrete signs can't be equal
       ((and (eq a 'pos) (eq b 'neg)) 'false)
       ((and (eq a 'neg) (eq b 'pos)) 'false)
       ((and (eq a 'pos) (eq b 'zero)) 'false)
       ((and (eq a 'zero) (eq b 'pos)) 'false)
       ((and (eq a 'neg) (eq b 'zero)) 'false)
       ((and (eq a 'zero) (eq b 'neg)) 'false)
       ;; pos = pos or neg = neg: could be (different values)
       (t 'maybe))))

  (list
    ;; Less-than
    (funcall 'neovm--abs-lt 'neg 'pos)      ;; true
    (funcall 'neovm--abs-lt 'pos 'neg)      ;; false
    (funcall 'neovm--abs-lt 'neg 'zero)     ;; true
    (funcall 'neovm--abs-lt 'zero 'pos)     ;; true
    (funcall 'neovm--abs-lt 'pos 'zero)     ;; false
    (funcall 'neovm--abs-lt 'zero 'neg)     ;; false
    (funcall 'neovm--abs-lt 'zero 'zero)    ;; false
    (funcall 'neovm--abs-lt 'pos 'pos)      ;; maybe
    (funcall 'neovm--abs-lt 'neg 'neg)      ;; maybe
    (funcall 'neovm--abs-lt 'top 'pos)      ;; maybe
    (funcall 'neovm--abs-lt 'bot 'pos)      ;; bot

    ;; Greater-than (symmetric)
    (funcall 'neovm--abs-gt 'pos 'neg)      ;; true
    (funcall 'neovm--abs-gt 'neg 'pos)      ;; false
    (funcall 'neovm--abs-gt 'pos 'zero)     ;; true
    (funcall 'neovm--abs-gt 'zero 'pos)     ;; false

    ;; Equality
    (funcall 'neovm--abs-eq 'zero 'zero)    ;; true
    (funcall 'neovm--abs-eq 'pos 'neg)      ;; false
    (funcall 'neovm--abs-eq 'neg 'pos)      ;; false
    (funcall 'neovm--abs-eq 'pos 'pos)      ;; maybe
    (funcall 'neovm--abs-eq 'neg 'neg)      ;; maybe
    (funcall 'neovm--abs-eq 'top 'zero)     ;; maybe
    (funcall 'neovm--abs-eq 'bot 'zero)))"#;
    let expect = expect_test::expect![[
        r#""OK (true false true true false false false maybe maybe maybe bot true false true false true false false maybe maybe maybe bot)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transfer functions for assignments in an abstract state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_interp_transfer_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An abstract state maps variables to abstract values.
    // Transfer functions update the state for assignment statements.
    let form = r#"(progn
  ;; State is an alist: ((var . abs-val) ...)
  (fset 'neovm--abs-state-get
    (lambda (state var)
      (let ((pair (assq var state)))
        (if pair (cdr pair) 'bot))))

  (fset 'neovm--abs-state-set
    (lambda (state var val)
      (cons (cons var val)
            (let ((result nil))
              (dolist (p state)
                (unless (eq (car p) var)
                  (setq result (cons p result))))
              (nreverse result)))))

  ;; Join two states (point-wise join)
  (fset 'neovm--abs-join
    (lambda (a b)
      (cond
       ((eq a 'bot) b) ((eq b 'bot) a) ((eq a 'top) 'top) ((eq b 'top) 'top)
       ((eq a b) a) (t 'top))))

  (fset 'neovm--abs-state-join
    (lambda (s1 s2)
      "Point-wise join of two states."
      (let ((all-vars nil))
        (dolist (p s1) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (dolist (p s2) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (let ((result nil))
          (dolist (v all-vars)
            (setq result (cons (cons v (funcall 'neovm--abs-join
                                                (funcall 'neovm--abs-state-get s1 v)
                                                (funcall 'neovm--abs-state-get s2 v)))
                               result)))
          result))))

  ;; Abstract eval: evaluate an expression in an abstract state
  (fset 'neovm--abs-eval-expr
    (lambda (state expr)
      "Evaluate EXPR in abstract STATE. EXPR is (op arg1 arg2) or a symbol or 'pos/'neg/'zero."
      (cond
       ((eq expr 'pos) 'pos)
       ((eq expr 'neg) 'neg)
       ((eq expr 'zero) 'zero)
       ((symbolp expr) (funcall 'neovm--abs-state-get state expr))
       ((listp expr)
        (let ((op (car expr))
              (a (funcall 'neovm--abs-eval-expr state (cadr expr)))
              (b (funcall 'neovm--abs-eval-expr state (caddr expr))))
          (cond
           ((eq op '+)
            (cond
             ((or (eq a 'bot) (eq b 'bot)) 'bot)
             ((or (eq a 'top) (eq b 'top)) 'top)
             ((eq a 'zero) b) ((eq b 'zero) a)
             ((and (eq a 'pos) (eq b 'pos)) 'pos)
             ((and (eq a 'neg) (eq b 'neg)) 'neg)
             (t 'top)))
           ((eq op '*)
            (cond
             ((or (eq a 'bot) (eq b 'bot)) 'bot)
             ((or (eq a 'zero) (eq b 'zero)) 'zero)
             ((or (eq a 'top) (eq b 'top)) 'top)
             ((and (eq a 'pos) (eq b 'pos)) 'pos)
             ((and (eq a 'neg) (eq b 'neg)) 'pos)
             ((or (and (eq a 'pos) (eq b 'neg))
                  (and (eq a 'neg) (eq b 'pos))) 'neg)
             (t 'top)))
           (t 'top))))
       (t 'top))))

  ;; Transfer function: (assign var expr) updates state
  (fset 'neovm--abs-transfer
    (lambda (state stmt)
      (if (eq (car stmt) 'assign)
          (let ((var (cadr stmt))
                (expr (caddr stmt)))
            (funcall 'neovm--abs-state-set state var
                     (funcall 'neovm--abs-eval-expr state expr)))
        state)))

  ;; Test: sequence of assignments
  (let* ((s0 nil)
         ;; x = pos
         (s1 (funcall 'neovm--abs-transfer s0 '(assign x pos)))
         ;; y = neg
         (s2 (funcall 'neovm--abs-transfer s1 '(assign y neg)))
         ;; z = x + y
         (s3 (funcall 'neovm--abs-transfer s2 '(assign z (+ x y))))
         ;; w = x * x
         (s4 (funcall 'neovm--abs-transfer s3 '(assign w (* x x))))
         ;; v = y * y
         (s5 (funcall 'neovm--abs-transfer s4 '(assign v (* y y))))
         ;; u = x * y
         (s6 (funcall 'neovm--abs-transfer s5 '(assign u (* x y)))))
    (list
      ;; Check each variable's abstract value
      (funcall 'neovm--abs-state-get s6 'x)   ;; pos
      (funcall 'neovm--abs-state-get s6 'y)   ;; neg
      (funcall 'neovm--abs-state-get s6 'z)   ;; top (pos + neg)
      (funcall 'neovm--abs-state-get s6 'w)   ;; pos (pos * pos)
      (funcall 'neovm--abs-state-get s6 'v)   ;; pos (neg * neg)
      (funcall 'neovm--abs-state-get s6 'u)   ;; neg (pos * neg)
      ;; Unbound variable
      (funcall 'neovm--abs-state-get s6 'q)   ;; bot

      ;; State join test
      (let* ((sa (list (cons 'x 'pos) (cons 'y 'neg)))
             (sb (list (cons 'x 'neg) (cons 'y 'neg) (cons 'z 'pos)))
             (joined (funcall 'neovm--abs-state-join sa sb)))
        (list
          (funcall 'neovm--abs-state-get joined 'x)   ;; top (pos join neg)
          (funcall 'neovm--abs-state-get joined 'y)   ;; neg (neg join neg)
          (funcall 'neovm--abs-state-get joined 'z))))))"#;
    let expect = expect_test::expect![[r#""OK (pos neg top pos pos neg bot (top neg pos))""#]];
    // pos (bot join pos)
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: abstract interpretation of loops with widening
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_interp_loop_widening() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate abstract interpretation of a simple loop with widening.
    // Program:
    //   x = 0
    //   while (x < 10):
    //     x = x + 1
    // Abstract interpretation should converge to: x = top (or non-negative)
    let form = r#"(progn
  ;; Lattice operations
  (fset 'neovm--ai-join
    (lambda (a b)
      (cond
       ((eq a 'bot) b) ((eq b 'bot) a) ((eq a 'top) 'top) ((eq b 'top) 'top)
       ((eq a b) a) (t 'top))))

  (fset 'neovm--ai-state-get
    (lambda (state var)
      (let ((pair (assq var state)))
        (if pair (cdr pair) 'bot))))

  (fset 'neovm--ai-state-set
    (lambda (state var val)
      (cons (cons var val)
            (let ((result nil))
              (dolist (p state)
                (unless (eq (car p) var)
                  (setq result (cons p result))))
              (nreverse result)))))

  (fset 'neovm--ai-state-join
    (lambda (s1 s2)
      (let ((all-vars nil))
        (dolist (p s1) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (dolist (p s2) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (let ((result nil))
          (dolist (v all-vars)
            (setq result (cons (cons v (funcall 'neovm--ai-join
                                                (funcall 'neovm--ai-state-get s1 v)
                                                (funcall 'neovm--ai-state-get s2 v)))
                               result)))
          result))))

  ;; States equal?
  (fset 'neovm--ai-state-eq
    (lambda (s1 s2)
      (let ((all-vars nil))
        (dolist (p s1) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (dolist (p s2) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (let ((equal t))
          (dolist (v all-vars)
            (unless (eq (funcall 'neovm--ai-state-get s1 v)
                        (funcall 'neovm--ai-state-get s2 v))
              (setq equal nil)))
          equal))))

  ;; Widening: if value changed, jump to top
  (fset 'neovm--ai-widen
    (lambda (old-state new-state)
      (let ((all-vars nil))
        (dolist (p old-state) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (dolist (p new-state) (unless (memq (car p) all-vars) (setq all-vars (cons (car p) all-vars))))
        (let ((result nil))
          (dolist (v all-vars)
            (let ((old-val (funcall 'neovm--ai-state-get old-state v))
                  (new-val (funcall 'neovm--ai-state-get new-state v)))
              (setq result
                    (cons (cons v
                                (if (eq old-val new-val)
                                    old-val
                                  'top))
                          result))))
          result))))

  ;; Abstract addition for state
  (fset 'neovm--ai-add-sign
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((eq a 'zero) b) ((eq b 'zero) a)
       ((and (eq a 'pos) (eq b 'pos)) 'pos)
       ((and (eq a 'neg) (eq b 'neg)) 'neg)
       (t 'top))))

  ;; Simulate: x = 0; while (x < 10): x = x + 1
  ;; Loop body in abstract domain: x_new = x_old + pos
  ;; Use fixed-point iteration with widening (max 10 iterations)
  (let* ((init-state (list (cons 'x 'zero)))
         ;; Loop entry: join init with loop back-edge
         (loop-state init-state)
         (converged nil)
         (iteration 0))
    (while (and (not converged) (< iteration 10))
      (setq iteration (1+ iteration))
      ;; Loop body: x = x + 1 (abstractly: x + pos)
      (let* ((x-val (funcall 'neovm--ai-state-get loop-state 'x))
             (new-x (funcall 'neovm--ai-add-sign x-val 'pos))
             (body-state (funcall 'neovm--ai-state-set loop-state 'x new-x))
             ;; Join init-state with body-state (loop header)
             (joined (funcall 'neovm--ai-state-join init-state body-state))
             ;; Apply widening
             (widened (funcall 'neovm--ai-widen loop-state joined)))
        (if (funcall 'neovm--ai-state-eq widened loop-state)
            (setq converged t)
          (setq loop-state widened))))

    (list
      ;; Final abstract value of x after loop convergence
      (funcall 'neovm--ai-state-get loop-state 'x)
      ;; Did it converge?
      converged
      ;; How many iterations?
      iteration
      ;; Should be <= 10
      (<= iteration 10)

      ;; Second test: y = pos; while: y = y * y
      ;; Should converge to y = pos (pos * pos = pos)
      (let* ((init2 (list (cons 'y 'pos)))
             (loop2 init2)
             (conv2 nil)
             (iter2 0))
        (while (and (not conv2) (< iter2 10))
          (setq iter2 (1+ iter2))
          (let* ((y-val (funcall 'neovm--ai-state-get loop2 'y))
                 (new-y (cond
                         ((or (eq y-val 'bot) (eq y-val 'zero)) 'zero)
                         ((or (eq y-val 'pos) (eq y-val 'neg)) 'pos)
                         (t 'top)))
                 (body2 (funcall 'neovm--ai-state-set loop2 'y new-y))
                 (joined2 (funcall 'neovm--ai-state-join init2 body2))
                 (widened2 (funcall 'neovm--ai-widen loop2 joined2)))
            (if (funcall 'neovm--ai-state-eq widened2 loop2)
                (setq conv2 t)
              (setq loop2 widened2))))
        (list
          (funcall 'neovm--ai-state-get loop2 'y)
          conv2
          iter2)))))"#;
    let expect = expect_test::expect![[r#""OK (top t 2 t (pos t 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: reaching definitions analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_interp_reaching_defs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reaching definitions: track which definitions (assignment sites) can
    // reach each program point. Forward dataflow analysis.
    let form = r#"(progn
  ;; Set operations
  (fset 'neovm--rd-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (member x result)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--rd-diff
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (member x b)
            (setq result (cons x result))))
        (sort result (lambda (x y) (string< (format "%s" x) (format "%s" y)))))))

  (fset 'neovm--rd-set-equal
    (lambda (a b)
      (and (= (length a) (length b))
           (null (funcall 'neovm--rd-diff a b)))))

  ;; A program is a list of basic blocks:
  ;; ((label stmts successors) ...)
  ;; Each stmt is (def var site-label) or (use var)
  ;; site-label identifies the definition site

  ;; Compute GEN[block]: definitions generated (last def of each var)
  (fset 'neovm--rd-gen
    (lambda (stmts)
      (let ((defs nil))
        (dolist (s stmts)
          (when (eq (car s) 'def)
            (let ((var (cadr s))
                  (site (caddr s)))
              ;; Remove previous defs of this var, add new
              (setq defs (cons (cons var site)
                               (let ((r nil))
                                 (dolist (d defs)
                                   (unless (eq (car d) var)
                                     (setq r (cons d r))))
                                 (nreverse r)))))))
        defs)))

  ;; KILL[block]: all defs of vars defined here, from other blocks
  (fset 'neovm--rd-kill
    (lambda (stmts all-defs)
      (let ((my-vars nil))
        (dolist (s stmts)
          (when (eq (car s) 'def)
            (unless (memq (cadr s) my-vars)
              (setq my-vars (cons (cadr s) my-vars)))))
        (let ((killed nil))
          (dolist (d all-defs)
            (when (and (memq (car d) my-vars)
                       (not (member d (funcall 'neovm--rd-gen stmts))))
              (setq killed (cons d killed))))
          killed))))

  ;; Fixed-point iteration for reaching definitions
  ;; OUT[B] = GEN[B] union (IN[B] - KILL[B])
  ;; IN[B] = union of OUT[pred] for all predecessors
  (fset 'neovm--rd-analyze
    (lambda (program)
      (let* (;; Collect all definition sites
             (all-defs nil)
             (block-data nil))
        ;; First pass: compute GEN for each block, collect all defs
        (dolist (block program)
          (let* ((label (car block))
                 (stmts (cadr block))
                 (succs (caddr block))
                 (gen (funcall 'neovm--rd-gen stmts)))
            (dolist (d gen)
              (unless (member d all-defs)
                (setq all-defs (cons d all-defs))))
            (setq block-data
                  (cons (list label stmts succs gen nil nil) block-data))))
        (setq block-data (nreverse block-data))
        ;; Compute KILL for each block
        (let ((updated-data nil))
          (dolist (bd block-data)
            (let ((kill (funcall 'neovm--rd-kill (nth 1 bd) all-defs)))
              (setq updated-data
                    (cons (list (nth 0 bd) (nth 1 bd) (nth 2 bd)
                                (nth 3 bd) kill nil)
                          updated-data))))
          (setq block-data (nreverse updated-data)))
        ;; Initialize OUT to GEN for each block
        (let ((out-map nil))
          (dolist (bd block-data)
            (setq out-map (cons (cons (car bd) (nth 3 bd)) out-map)))
          ;; Fixed-point iteration
          (let ((changed t) (iters 0))
            (while (and changed (< iters 20))
              (setq changed nil iters (1+ iters))
              (dolist (bd block-data)
                (let* ((label (car bd))
                       (gen (nth 3 bd))
                       (kill (nth 4 bd))
                       ;; Compute IN = union of OUT[pred]
                       (in-set nil))
                  ;; Find predecessors
                  (dolist (bd2 block-data)
                    (when (memq label (nth 2 bd2))
                      (let ((pred-out (cdr (assq (car bd2) out-map))))
                        (setq in-set (funcall 'neovm--rd-union in-set pred-out)))))
                  ;; OUT = GEN union (IN - KILL)
                  (let ((new-out (funcall 'neovm--rd-union gen
                                          (funcall 'neovm--rd-diff in-set kill))))
                    (unless (funcall 'neovm--rd-set-equal
                                     new-out
                                     (cdr (assq label out-map)))
                      (setq changed t)
                      (setq out-map
                            (cons (cons label new-out)
                                  (let ((r nil))
                                    (dolist (p out-map)
                                      (unless (eq (car p) label)
                                        (setq r (cons p r))))
                                    (nreverse r)))))))))
          ;; Return sorted OUT sets for each block
          (let ((result nil))
            (dolist (bd block-data)
              (let ((label (car bd)))
                (setq result (cons (list label
                                        (sort (copy-sequence (cdr (assq label out-map)))
                                              (lambda (x y)
                                                (string< (format "%s" x) (format "%s" y)))))
                                   result))))
            (nreverse result))))))

  ;; Test program:
  ;; B1: x = 1 (site d1), y = 2 (site d2) -> B2
  ;; B2: z = x + y (site d3) -> B3
  ;; B3: x = 3 (site d4), use z -> B2, B4
  ;; B4: use x, use z
  (let ((program '((B1 ((def x d1) (def y d2)) (B2))
                   (B2 ((def z d3)) (B3))
                   (B3 ((def x d4) (use z)) (B2 B4))
                   (B4 ((use x) (use z)) ()))))
    (funcall 'neovm--rd-analyze program)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Abstract interpretation of a multi-variable program
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_interp_multi_var_program() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interpret a small program abstractly and check final state
    let form = r#"(progn
  ;; Abstract arithmetic
  (fset 'neovm--ai2-add
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((eq a 'zero) b) ((eq b 'zero) a)
       ((and (eq a 'pos) (eq b 'pos)) 'pos)
       ((and (eq a 'neg) (eq b 'neg)) 'neg)
       (t 'top))))

  (fset 'neovm--ai2-mul
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'zero) (eq b 'zero)) 'zero)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((and (eq a 'pos) (eq b 'pos)) 'pos)
       ((and (eq a 'neg) (eq b 'neg)) 'pos)
       ((or (and (eq a 'pos) (eq b 'neg))
            (and (eq a 'neg) (eq b 'pos))) 'neg)
       (t 'top))))

  (fset 'neovm--ai2-sub
    (lambda (a b)
      (cond
       ((or (eq a 'bot) (eq b 'bot)) 'bot)
       ((or (eq a 'top) (eq b 'top)) 'top)
       ((eq b 'zero) a)
       ((and (eq a 'zero) (eq b 'pos)) 'neg)
       ((and (eq a 'zero) (eq b 'neg)) 'pos)
       ((and (eq a 'pos) (eq b 'neg)) 'pos)
       ((and (eq a 'neg) (eq b 'pos)) 'neg)
       (t 'top))))

  ;; State operations
  (fset 'neovm--ai2-get
    (lambda (state var)
      (let ((pair (assq var state)))
        (if pair (cdr pair) 'bot))))

  (fset 'neovm--ai2-set
    (lambda (state var val)
      (cons (cons var val)
            (let ((r nil))
              (dolist (p state) (unless (eq (car p) var) (setq r (cons p r))))
              (nreverse r)))))

  ;; Evaluate expression
  (fset 'neovm--ai2-eval
    (lambda (state expr)
      (cond
       ((memq expr '(pos neg zero top bot)) expr)
       ((symbolp expr) (funcall 'neovm--ai2-get state expr))
       ((and (listp expr) (eq (car expr) '+))
        (funcall 'neovm--ai2-add
                 (funcall 'neovm--ai2-eval state (cadr expr))
                 (funcall 'neovm--ai2-eval state (caddr expr))))
       ((and (listp expr) (eq (car expr) '-))
        (funcall 'neovm--ai2-sub
                 (funcall 'neovm--ai2-eval state (cadr expr))
                 (funcall 'neovm--ai2-eval state (caddr expr))))
       ((and (listp expr) (eq (car expr) '*))
        (funcall 'neovm--ai2-mul
                 (funcall 'neovm--ai2-eval state (cadr expr))
                 (funcall 'neovm--ai2-eval state (caddr expr))))
       (t 'top))))

  ;; Execute a list of (assign var expr) statements
  (fset 'neovm--ai2-exec
    (lambda (state stmts)
      (dolist (s stmts)
        (when (eq (car s) 'assign)
          (setq state (funcall 'neovm--ai2-set state (cadr s)
                               (funcall 'neovm--ai2-eval state (caddr s))))))
      state))

  ;; Program:
  ;; a = pos (e.g., 5)
  ;; b = neg (e.g., -3)
  ;; c = a + b            -> top
  ;; d = a * b            -> neg
  ;; e = d * d            -> pos (neg * neg)
  ;; f = a - b            -> pos (pos - neg)
  ;; g = b - a            -> neg (neg - pos)
  ;; h = c + e            -> top (top + pos)
  ;; i = e * f            -> pos (pos * pos)
  ;; j = a + a            -> pos (pos + pos)
  ;; k = b + b            -> neg (neg + neg)
  ;; l = j * k            -> neg (pos * neg)
  ;; m = l - l            -> top (neg - neg could be anything)
  (let ((final (funcall 'neovm--ai2-exec nil
                '((assign a pos)
                  (assign b neg)
                  (assign c (+ a b))
                  (assign d (* a b))
                  (assign e (* d d))
                  (assign f (- a b))
                  (assign g (- b a))
                  (assign h (+ c e))
                  (assign i (* e f))
                  (assign j (+ a a))
                  (assign k (+ b b))
                  (assign l (* j k))
                  (assign m (- l l))))))
    (list
      (funcall 'neovm--ai2-get final 'a)   ;; pos
      (funcall 'neovm--ai2-get final 'b)   ;; neg
      (funcall 'neovm--ai2-get final 'c)   ;; top
      (funcall 'neovm--ai2-get final 'd)   ;; neg
      (funcall 'neovm--ai2-get final 'e)   ;; pos
      (funcall 'neovm--ai2-get final 'f)   ;; pos
      (funcall 'neovm--ai2-get final 'g)   ;; neg
      (funcall 'neovm--ai2-get final 'h)   ;; top
      (funcall 'neovm--ai2-get final 'i)   ;; pos
      (funcall 'neovm--ai2-get final 'j)   ;; pos
      (funcall 'neovm--ai2-get final 'k)   ;; neg
      (funcall 'neovm--ai2-get final 'l)   ;; neg
      (funcall 'neovm--ai2-get final 'm)   ;; top
      ;; Unbound
      (funcall 'neovm--ai2-get final 'z))))"#;
    let expect =
        expect_test::expect![[r#""OK (pos neg top neg pos pos neg top pos pos neg neg top bot)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
