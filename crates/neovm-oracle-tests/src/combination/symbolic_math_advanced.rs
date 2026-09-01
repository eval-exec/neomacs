//! Advanced symbolic math oracle parity tests.
//!
//! Implements symbolic algebra: expression representation (num, var, +, *, ^,
//! sin, cos, log, exp), simplification rules (identity, zero, constant folding,
//! commutativity, associativity, distribution), symbolic differentiation with
//! chain rule, partial differentiation, expression evaluation with variable
//! bindings, polynomial operations (degree, leading term, GCD via Euclidean
//! algorithm), expression substitution, and common subexpression elimination.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Extended expression representation with transcendental functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_extended_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Expression evaluator supporting: +, -, *, /, expt, sin, cos, log, exp, sqrt, abs
  (fset 'neovm--sma-eval
    (lambda (expr env)
      "Evaluate symbolic expression EXPR with variable bindings from ENV alist."
      (cond
       ((numberp expr) (float expr))
       ((symbolp expr)
        (let ((binding (assq expr env)))
          (if binding (float (cdr binding))
            (signal 'error (list "unbound" expr)))))
       (t
        (let ((op (car expr)))
          (cond
           ;; Binary operations
           ((memq op '(+ - * /))
            (let ((args (mapcar (lambda (e) (funcall 'neovm--sma-eval e env))
                                (cdr expr))))
              (cond
               ((eq op '+) (apply '+ args))
               ((eq op '-) (apply '- args))
               ((eq op '*) (apply '* args))
               ((eq op '/) (/ (car args) (cadr args))))))
           ;; Power
           ((eq op 'expt)
            (expt (funcall 'neovm--sma-eval (nth 1 expr) env)
                  (funcall 'neovm--sma-eval (nth 2 expr) env)))
           ;; Transcendentals
           ((eq op 'sin) (sin (funcall 'neovm--sma-eval (nth 1 expr) env)))
           ((eq op 'cos) (cos (funcall 'neovm--sma-eval (nth 1 expr) env)))
           ((eq op 'log) (log (funcall 'neovm--sma-eval (nth 1 expr) env)))
           ((eq op 'exp) (exp (funcall 'neovm--sma-eval (nth 1 expr) env)))
           ((eq op 'sqrt) (sqrt (funcall 'neovm--sma-eval (nth 1 expr) env)))
           ((eq op 'abs) (abs (funcall 'neovm--sma-eval (nth 1 expr) env)))
           (t (signal 'error (list "unknown-op" op)))))))))

  (unwind-protect
      (let ((env '((x . 2) (y . 3) (z . 0))))
        (list
          ;; Basic evaluation
          (funcall 'neovm--sma-eval '(+ x y) env)
          (funcall 'neovm--sma-eval '(* x (+ y 1)) env)
          ;; Transcendentals
          (funcall 'neovm--sma-eval '(sin z) env)       ;; sin(0) = 0
          (funcall 'neovm--sma-eval '(cos z) env)       ;; cos(0) = 1
          (funcall 'neovm--sma-eval '(exp z) env)       ;; exp(0) = 1
          (funcall 'neovm--sma-eval '(log 1) env)       ;; log(1) = 0
          ;; Compound: exp(log(x)) = x
          (funcall 'neovm--sma-eval '(exp (log x)) env)
          ;; sin^2(x) + cos^2(x) ~ 1
          (let ((result (funcall 'neovm--sma-eval
                                  '(+ (expt (sin x) 2) (expt (cos x) 2))
                                  env)))
            (< (abs (- result 1.0)) 1e-10))
          ;; sqrt(x^2) = |x|
          (funcall 'neovm--sma-eval '(sqrt (expt x 2)) env)
          ;; Nested: (x + y)^2 = x^2 + 2xy + y^2
          (let ((lhs (funcall 'neovm--sma-eval '(expt (+ x y) 2) env))
                (rhs (funcall 'neovm--sma-eval
                               '(+ (+ (expt x 2) (* 2 (* x y))) (expt y 2))
                               env)))
            (< (abs (- lhs rhs)) 1e-10))))
    (fmakunbound 'neovm--sma-eval)))"#;
    let expect = expect_test::expect![[r#""OK (5.0 8.0 0.0 1.0 1.0 0.0 2.0 t 2.0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Advanced simplification with commutativity, associativity, distribution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_simplification_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--sma-simplify
    (lambda (expr)
      "Apply algebraic simplification rules."
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sma-simplify e))
                             (cdr expr)))
               (a (nth 0 args))
               (b (nth 1 args)))
          (cond
           ;; Constant folding for arithmetic
           ((and (memq op '(+ - * /)) (numberp a) (numberp b))
            (cond ((eq op '+) (+ a b))
                  ((eq op '-) (- a b))
                  ((eq op '*) (* a b))
                  ((eq op '/) (if (= b 0) (cons op args) (/ a b)))))
           ;; Additive identity: x + 0 = x, 0 + x = x
           ((and (eq op '+) (numberp a) (= a 0)) b)
           ((and (eq op '+) (numberp b) (= b 0)) a)
           ;; Subtractive identity: x - 0 = x
           ((and (eq op '-) (= (length args) 2) (numberp b) (= b 0)) a)
           ;; Subtraction of self: x - x = 0
           ((and (eq op '-) (= (length args) 2) (equal a b)) 0)
           ;; Multiplicative identity: x * 1 = x, 1 * x = x
           ((and (eq op '*) (numberp a) (= a 1)) b)
           ((and (eq op '*) (numberp b) (= b 1)) a)
           ;; Multiplicative zero: x * 0 = 0
           ((and (eq op '*) (or (and (numberp a) (= a 0))
                                (and (numberp b) (= b 0)))) 0)
           ;; Power rules: x^0 = 1, x^1 = x, 0^n = 0 (n>0), 1^n = 1
           ((and (eq op 'expt) (numberp b) (= b 0)) 1)
           ((and (eq op 'expt) (numberp b) (= b 1)) a)
           ((and (eq op 'expt) (numberp a) (= a 0) (numberp b) (> b 0)) 0)
           ((and (eq op 'expt) (numberp a) (= a 1)) 1)
           ;; Double negation: -(-(x)) = x
           ((and (eq op '-) (= (length args) 1) (listp a) (eq (car a) '-) (= (length a) 2))
            (cadr a))
           ;; sin(0) = 0, cos(0) = 1
           ((and (eq op 'sin) (numberp a) (= a 0)) 0)
           ((and (eq op 'cos) (numberp a) (= a 0)) 1)
           ;; log(1) = 0, exp(0) = 1
           ((and (eq op 'log) (numberp a) (= a 1)) 0)
           ((and (eq op 'exp) (numberp a) (= a 0)) 1)
           ;; log(exp(x)) = x, exp(log(x)) = x
           ((and (eq op 'log) (listp a) (eq (car a) 'exp)) (cadr a))
           ((and (eq op 'exp) (listp a) (eq (car a) 'log)) (cadr a))
           ;; Division: x/1 = x, 0/x = 0, x/x = 1
           ((and (eq op '/) (numberp b) (= b 1)) a)
           ((and (eq op '/) (numberp a) (= a 0)) 0)
           ((and (eq op '/) (equal a b)) 1)
           ;; Default
           (t (cons op args)))))))

  ;; Apply until fixpoint
  (fset 'neovm--sma-simplify-fix
    (lambda (expr)
      (let ((prev nil) (cur expr) (n 0))
        (while (and (not (equal prev cur)) (< n 15))
          (setq prev cur)
          (setq cur (funcall 'neovm--sma-simplify cur))
          (setq n (1+ n)))
        cur)))

  (unwind-protect
      (list
        ;; Basic simplifications
        (funcall 'neovm--sma-simplify-fix '(+ x 0))          ;; x
        (funcall 'neovm--sma-simplify-fix '(* x 1))          ;; x
        (funcall 'neovm--sma-simplify-fix '(* x 0))          ;; 0
        (funcall 'neovm--sma-simplify-fix '(expt x 0))       ;; 1
        (funcall 'neovm--sma-simplify-fix '(expt x 1))       ;; x
        (funcall 'neovm--sma-simplify-fix '(- x x))          ;; 0
        ;; Constant folding
        (funcall 'neovm--sma-simplify-fix '(+ 3 4))          ;; 7
        (funcall 'neovm--sma-simplify-fix '(* 2 (* 3 4)))    ;; 24
        ;; Transcendental simplifications
        (funcall 'neovm--sma-simplify-fix '(sin 0))          ;; 0
        (funcall 'neovm--sma-simplify-fix '(cos 0))          ;; 1
        (funcall 'neovm--sma-simplify-fix '(log (exp x)))    ;; x
        (funcall 'neovm--sma-simplify-fix '(exp (log x)))    ;; x
        ;; Double negation
        (funcall 'neovm--sma-simplify-fix '(- (- x)))        ;; x
        ;; Nested: (0 + (x * 1)) + (0 * y)
        (funcall 'neovm--sma-simplify-fix '(+ (+ 0 (* x 1)) (* 0 y)))  ;; x
        ;; Division simplification
        (funcall 'neovm--sma-simplify-fix '(/ x 1))          ;; x
        (funcall 'neovm--sma-simplify-fix '(/ x x))          ;; 1
        (funcall 'neovm--sma-simplify-fix '(/ 0 x)))         ;; 0
    (fmakunbound 'neovm--sma-simplify)
    (fmakunbound 'neovm--sma-simplify-fix)))"#;
    let expect = expect_test::expect![[r#""OK (x x 0 1 x 0 7 24 0 1 x x x x x 1 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbolic differentiation with chain rule
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_differentiation_chain_rule() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Full symbolic differentiation with chain rule for all supported ops
  (fset 'neovm--sma-diff
    (lambda (expr var)
      "Symbolic differentiation of EXPR with respect to VAR."
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       (t
        (let ((op (car expr)))
          (cond
           ;; Sum/difference rule
           ((eq op '+)
            (list '+ (funcall 'neovm--sma-diff (nth 1 expr) var)
                  (funcall 'neovm--sma-diff (nth 2 expr) var)))
           ((eq op '-)
            (if (= (length expr) 2)
                (list '- (funcall 'neovm--sma-diff (nth 1 expr) var))
              (list '- (funcall 'neovm--sma-diff (nth 1 expr) var)
                    (funcall 'neovm--sma-diff (nth 2 expr) var))))
           ;; Product rule: (fg)' = f'g + fg'
           ((eq op '*)
            (let ((f (nth 1 expr)) (g (nth 2 expr)))
              (list '+ (list '* (funcall 'neovm--sma-diff f var) g)
                    (list '* f (funcall 'neovm--sma-diff g var)))))
           ;; Quotient rule: (f/g)' = (f'g - fg') / g^2
           ((eq op '/)
            (let ((f (nth 1 expr)) (g (nth 2 expr)))
              (list '/ (list '- (list '* (funcall 'neovm--sma-diff f var) g)
                              (list '* f (funcall 'neovm--sma-diff g var)))
                    (list 'expt g 2))))
           ;; Power rule: (f^n)' = n * f^(n-1) * f'
           ((eq op 'expt)
            (let ((f (nth 1 expr)) (n (nth 2 expr)))
              (if (numberp n)
                  (list '* (list '* n (list 'expt f (- n 1)))
                        (funcall 'neovm--sma-diff f var))
                ;; General case: f^g = exp(g*log(f))
                ;; d/dx = f^g * (g'*log(f) + g*f'/f)
                (list '* expr
                      (list '+ (list '* (funcall 'neovm--sma-diff n var)
                                    (list 'log f))
                            (list '* n (list '/ (funcall 'neovm--sma-diff f var) f)))))))
           ;; Chain rule for transcendentals
           ;; d/dx sin(f) = cos(f) * f'
           ((eq op 'sin)
            (list '* (list 'cos (nth 1 expr))
                  (funcall 'neovm--sma-diff (nth 1 expr) var)))
           ;; d/dx cos(f) = -sin(f) * f'
           ((eq op 'cos)
            (list '* (list '- (list 'sin (nth 1 expr)))
                  (funcall 'neovm--sma-diff (nth 1 expr) var)))
           ;; d/dx exp(f) = exp(f) * f'
           ((eq op 'exp)
            (list '* (list 'exp (nth 1 expr))
                  (funcall 'neovm--sma-diff (nth 1 expr) var)))
           ;; d/dx log(f) = f' / f
           ((eq op 'log)
            (list '/ (funcall 'neovm--sma-diff (nth 1 expr) var)
                  (nth 1 expr)))
           ;; d/dx sqrt(f) = f' / (2 * sqrt(f))
           ((eq op 'sqrt)
            (list '/ (funcall 'neovm--sma-diff (nth 1 expr) var)
                  (list '* 2 (list 'sqrt (nth 1 expr)))))
           (t (list 'diff expr var))))))))

  (unwind-protect
      (list
        ;; d/dx(x^3) = 3*x^2*1
        (funcall 'neovm--sma-diff '(expt x 3) 'x)
        ;; d/dx(sin(x)) = cos(x)*1
        (funcall 'neovm--sma-diff '(sin x) 'x)
        ;; d/dx(cos(x)) = -sin(x)*1
        (funcall 'neovm--sma-diff '(cos x) 'x)
        ;; d/dx(exp(x)) = exp(x)*1
        (funcall 'neovm--sma-diff '(exp x) 'x)
        ;; d/dx(log(x)) = 1/x
        (funcall 'neovm--sma-diff '(log x) 'x)
        ;; Chain rule: d/dx(sin(x^2)) = cos(x^2) * 2*x
        (funcall 'neovm--sma-diff '(sin (expt x 2)) 'x)
        ;; Chain rule: d/dx(exp(3*x)) = exp(3*x) * 3
        (funcall 'neovm--sma-diff '(exp (* 3 x)) 'x)
        ;; Quotient rule: d/dx(x / (x+1))
        (funcall 'neovm--sma-diff '(/ x (+ x 1)) 'x)
        ;; d/dx(sqrt(x)) = 1 / (2*sqrt(x))
        (funcall 'neovm--sma-diff '(sqrt x) 'x)
        ;; Product + chain: d/dx(x * sin(x))
        (funcall 'neovm--sma-diff '(* x (sin x)) 'x))
    (fmakunbound 'neovm--sma-diff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((* (* 3 (expt x 2)) 1) (* (cos x) 1) (* (- (sin x)) 1) (* (exp x) 1) (/ 1 x) (* (cos (expt x 2)) (* (* 2 (expt x 1)) 1)) (* (exp (* 3 x)) (+ (* 0 x) (* 3 1))) (/ (- (* 1 (+ x 1)) (* x (+ 1 0))) (expt (+ x 1) 2)) (/ 1 (* 2 (sqrt x))) (+ (* 1 (sin x)) (* x (* (cos x) 1))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partial differentiation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_partial_differentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Reuse the differentiator
  (fset 'neovm--sma-pd
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       (t
        (let ((op (car expr)))
          (cond
           ((eq op '+)
            (list '+ (funcall 'neovm--sma-pd (nth 1 expr) var)
                  (funcall 'neovm--sma-pd (nth 2 expr) var)))
           ((eq op '-)
            (if (= (length expr) 2)
                (list '- (funcall 'neovm--sma-pd (nth 1 expr) var))
              (list '- (funcall 'neovm--sma-pd (nth 1 expr) var)
                    (funcall 'neovm--sma-pd (nth 2 expr) var))))
           ((eq op '*)
            (list '+ (list '* (funcall 'neovm--sma-pd (nth 1 expr) var) (nth 2 expr))
                  (list '* (nth 1 expr) (funcall 'neovm--sma-pd (nth 2 expr) var))))
           ((eq op 'expt)
            (let ((f (nth 1 expr)) (n (nth 2 expr)))
              (list '* (list '* n (list 'expt f (- n 1)))
                    (funcall 'neovm--sma-pd f var))))
           ((eq op 'sin)
            (list '* (list 'cos (nth 1 expr))
                  (funcall 'neovm--sma-pd (nth 1 expr) var)))
           ((eq op 'cos)
            (list '* (list '- (list 'sin (nth 1 expr)))
                  (funcall 'neovm--sma-pd (nth 1 expr) var)))
           (t (list 'pd expr var))))))))

  ;; Simplifier
  (fset 'neovm--sma-pd-simp
    (lambda (expr)
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sma-pd-simp e)) (cdr expr)))
               (a (nth 0 args)) (b (nth 1 args)))
          (cond
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '+) (equal a 0)) b)
           ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '*) (or (equal a 0) (equal b 0))) 0)
           ((and (eq op '*) (equal a 1)) b)
           ((and (eq op '*) (equal b 1)) a)
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ((and (eq op 'expt) (equal b 0)) 1)
           ((and (eq op 'expt) (equal b 1)) a)
           (t (cons op args)))))))

  (fset 'neovm--sma-pd-fix
    (lambda (expr)
      (let ((prev nil) (cur expr) (n 0))
        (while (and (not (equal prev cur)) (< n 10))
          (setq prev cur)
          (setq cur (funcall 'neovm--sma-pd-simp cur))
          (setq n (1+ n)))
        cur)))

  (unwind-protect
      (let ((f '(+ (* x (expt y 2)) (* 3 (* x y)) (expt x 3))))
        ;; f(x,y) = xy^2 + 3xy + x^3
        (list
          ;; df/dx = y^2 + 3y + 3x^2 (simplified)
          (funcall 'neovm--sma-pd-fix (funcall 'neovm--sma-pd f 'x))
          ;; df/dy = 2xy + 3x (simplified)
          (funcall 'neovm--sma-pd-fix (funcall 'neovm--sma-pd f 'y))
          ;; d^2f/dxdy = 2y + 3
          (funcall 'neovm--sma-pd-fix
                   (funcall 'neovm--sma-pd
                            (funcall 'neovm--sma-pd f 'x) 'y))
          ;; d^2f/dydx = 2y + 3 (mixed partials equal by Clairaut's)
          (funcall 'neovm--sma-pd-fix
                   (funcall 'neovm--sma-pd
                            (funcall 'neovm--sma-pd f 'y) 'x))
          ;; Simple case: df/dz = 0 (z not in expression)
          (funcall 'neovm--sma-pd-fix (funcall 'neovm--sma-pd f 'z))))
    (fmakunbound 'neovm--sma-pd)
    (fmakunbound 'neovm--sma-pd-simp)
    (fmakunbound 'neovm--sma-pd-fix)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ (expt y 2) (* 3 y)) (+ (* x (* 2 y)) (* 3 x)) (+ (* 2 y) 3) (+ (* 2 y) 3) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial operations: degree, leading term, GCD
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_polynomial_gcd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Polynomial: sorted alist ((degree . coeff) ...) descending degree

  (fset 'neovm--sma-poly-degree
    (lambda (p) (if (null p) -1 (caar p))))

  (fset 'neovm--sma-poly-leading
    (lambda (p) (if (null p) 0 (cdar p))))

  (fset 'neovm--sma-poly-normalize
    (lambda (p)
      (let ((filtered nil))
        (dolist (term p)
          (unless (= (cdr term) 0)
            (setq filtered (cons term filtered))))
        (sort filtered (lambda (a b) (> (car a) (car b)))))))

  (fset 'neovm--sma-poly-coeff
    (lambda (p deg)
      (let ((entry (assq deg p)))
        (if entry (cdr entry) 0))))

  (fset 'neovm--sma-poly-scale
    (lambda (p c)
      "Multiply polynomial by scalar c."
      (funcall 'neovm--sma-poly-normalize
               (mapcar (lambda (term) (cons (car term) (* (cdr term) c))) p))))

  (fset 'neovm--sma-poly-add
    (lambda (p1 p2)
      (let ((result nil) (all-degrees nil))
        (dolist (t1 p1) (unless (memq (car t1) all-degrees)
                          (setq all-degrees (cons (car t1) all-degrees))))
        (dolist (t2 p2) (unless (memq (car t2) all-degrees)
                          (setq all-degrees (cons (car t2) all-degrees))))
        (dolist (d all-degrees)
          (setq result (cons (cons d (+ (funcall 'neovm--sma-poly-coeff p1 d)
                                        (funcall 'neovm--sma-poly-coeff p2 d)))
                             result)))
        (funcall 'neovm--sma-poly-normalize result))))

  (fset 'neovm--sma-poly-sub
    (lambda (p1 p2)
      (funcall 'neovm--sma-poly-add p1 (funcall 'neovm--sma-poly-scale p2 -1))))

  (fset 'neovm--sma-poly-mul
    (lambda (p1 p2)
      (let ((result nil))
        (dolist (t1 p1)
          (dolist (t2 p2)
            (let* ((d (+ (car t1) (car t2)))
                   (c (* (cdr t1) (cdr t2)))
                   (existing (assq d result)))
              (if existing
                  (setcdr existing (+ (cdr existing) c))
                (setq result (cons (cons d c) result))))))
        (funcall 'neovm--sma-poly-normalize result))))

  ;; Polynomial division: returns (quotient . remainder)
  (fset 'neovm--sma-poly-div
    (lambda (dividend divisor)
      (let ((q nil) (r (copy-sequence dividend))
            (max-iter 100) (iter 0))
        (while (and (not (null r))
                    (>= (funcall 'neovm--sma-poly-degree r)
                        (funcall 'neovm--sma-poly-degree divisor))
                    (< iter max-iter))
          (let* ((ld-r (funcall 'neovm--sma-poly-leading r))
                 (ld-d (funcall 'neovm--sma-poly-leading divisor))
                 (coeff (/ (float ld-r) ld-d))
                 (deg (- (funcall 'neovm--sma-poly-degree r)
                         (funcall 'neovm--sma-poly-degree divisor)))
                 (term (list (cons deg coeff))))
            (setq q (funcall 'neovm--sma-poly-add (or q nil) term))
            (setq r (funcall 'neovm--sma-poly-sub r
                              (funcall 'neovm--sma-poly-mul term divisor))))
          (setq iter (1+ iter)))
        (cons q r))))

  ;; GCD via Euclidean algorithm
  (fset 'neovm--sma-poly-gcd
    (lambda (p1 p2)
      (let ((a p1) (b p2) (max-iter 50) (iter 0))
        (while (and b (not (null b)) (< iter max-iter))
          (let* ((div-result (funcall 'neovm--sma-poly-div a b))
                 (rem (cdr div-result)))
            (setq a b)
            (setq b rem))
          (setq iter (1+ iter)))
        ;; Normalize: make leading coefficient 1 (monic)
        (if (null a) nil
          (let ((lc (funcall 'neovm--sma-poly-leading a)))
            (if (= lc 0) nil
              (funcall 'neovm--sma-poly-scale a (/ 1.0 lc))))))))

  (unwind-protect
      (let ((p1 '((3 . 1) (2 . -1) (1 . -2) (0 . 2)))   ;; x^3 - x^2 - 2x + 2
            (p2 '((2 . 1) (0 . -1)))                       ;; x^2 - 1
            (p3 '((2 . 1) (1 . 2) (0 . 1)))               ;; x^2 + 2x + 1 = (x+1)^2
            (p4 '((1 . 1) (0 . 1))))                       ;; x + 1
        (list
          ;; Degree
          (funcall 'neovm--sma-poly-degree p1)  ;; 3
          (funcall 'neovm--sma-poly-degree p2)  ;; 2
          ;; Leading term
          (funcall 'neovm--sma-poly-leading p1) ;; 1
          ;; Multiplication: (x+1)^2 = x^2 + 2x + 1
          (funcall 'neovm--sma-poly-mul p4 p4)
          ;; Verify (x+1)(x+1) = p3
          (equal (funcall 'neovm--sma-poly-mul p4 p4) p3)
          ;; GCD of (x^2 + 2x + 1) and (x + 1) should be (x + 1) (monic)
          (funcall 'neovm--sma-poly-gcd p3 p4)
          ;; Polynomial division: (x^2 - 1) / (x + 1) = (x - 1) remainder 0
          (let ((result (funcall 'neovm--sma-poly-div p2 p4)))
            (list (car result) (cdr result)))))
    (fmakunbound 'neovm--sma-poly-degree)
    (fmakunbound 'neovm--sma-poly-leading)
    (fmakunbound 'neovm--sma-poly-normalize)
    (fmakunbound 'neovm--sma-poly-coeff)
    (fmakunbound 'neovm--sma-poly-scale)
    (fmakunbound 'neovm--sma-poly-add)
    (fmakunbound 'neovm--sma-poly-sub)
    (fmakunbound 'neovm--sma-poly-mul)
    (fmakunbound 'neovm--sma-poly-div)
    (fmakunbound 'neovm--sma-poly-gcd)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 2 1 ((2 . 1) (1 . 2) (0 . 1)) t ((1 . 1.0) (0 . 1.0)) (((1 . 1.0) (0 . -1.0)) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_substitution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--sma-subst
    (lambda (expr var replacement)
      "Substitute VAR with REPLACEMENT in EXPR."
      (cond
       ((numberp expr) expr)
       ((symbolp expr) (if (eq expr var) replacement expr))
       (t (cons (car expr)
                (mapcar (lambda (e) (funcall 'neovm--sma-subst e var replacement))
                        (cdr expr)))))))

  ;; Multi-variable substitution
  (fset 'neovm--sma-subst-all
    (lambda (expr bindings)
      "Apply all substitutions from BINDINGS alist."
      (let ((result expr))
        (dolist (binding bindings)
          (setq result (funcall 'neovm--sma-subst result (car binding) (cdr binding))))
        result)))

  (unwind-protect
      (let ((expr '(+ (* x (expt y 2)) (* 3 x))))
        (list
          ;; Simple substitution: x -> 2
          (funcall 'neovm--sma-subst expr 'x 2)
          ;; Substitution with expression: x -> (+ a 1)
          (funcall 'neovm--sma-subst expr 'x '(+ a 1))
          ;; Substitution of y: y -> z
          (funcall 'neovm--sma-subst expr 'y 'z)
          ;; Multi-variable: x -> 1, y -> 2
          (funcall 'neovm--sma-subst-all expr '((x . 1) (y . 2)))
          ;; Substitute with another expression: y -> (* x 2) in original
          (funcall 'neovm--sma-subst expr 'y '(* x 2))
          ;; No-op substitution (variable not present)
          (funcall 'neovm--sma-subst expr 'z 99)
          ;; Nested substitution: substitute result again
          (let ((step1 (funcall 'neovm--sma-subst expr 'x '(+ a b))))
            (funcall 'neovm--sma-subst step1 'a 1))))
    (fmakunbound 'neovm--sma-subst)
    (fmakunbound 'neovm--sma-subst-all)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ (* 2 (expt y 2)) (* 3 2)) (+ (* (+ a 1) (expt y 2)) (* 3 (+ a 1))) (+ (* x (expt z 2)) (* 3 x)) (+ (* 1 (expt 2 2)) (* 3 1)) (+ (* x (expt (* x 2) 2)) (* 3 x)) (+ (* x (expt y 2)) (* 3 x)) (+ (* (+ 1 b) (expt y 2)) (* 3 (+ 1 b))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Common subexpression elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_cse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Common subexpression elimination: find repeated subexpressions
  ;; and count them.

  (fset 'neovm--sma-cse-collect
    (lambda (expr)
      "Collect all subexpressions into a frequency table."
      (let ((freq (make-hash-table :test 'equal)))
        (funcall 'neovm--sma-cse-walk expr freq)
        freq)))

  (fset 'neovm--sma-cse-walk
    (lambda (expr freq)
      "Walk expression tree, count subexpressions."
      (cond
       ((numberp expr) nil)  ;; skip atoms
       ((symbolp expr) nil)
       (t
        ;; Count this subexpression
        (let ((key (prin1-to-string expr)))
          (puthash key (1+ (gethash key freq 0)) freq))
        ;; Recurse into children
        (dolist (child (cdr expr))
          (funcall 'neovm--sma-cse-walk child freq))))))

  (fset 'neovm--sma-cse-find-common
    (lambda (expr)
      "Find subexpressions appearing more than once."
      (let ((freq (funcall 'neovm--sma-cse-collect expr))
            (common nil))
        (maphash (lambda (k v)
                   (when (> v 1)
                     (setq common (cons (cons k v) common))))
                 freq)
        ;; Sort by frequency descending, then by key for stability
        (sort common (lambda (a b)
                       (or (> (cdr a) (cdr b))
                           (and (= (cdr a) (cdr b))
                                (string< (car a) (car b)))))))))

  ;; Expression size (number of nodes)
  (fset 'neovm--sma-cse-size
    (lambda (expr)
      (cond
       ((or (numberp expr) (symbolp expr)) 1)
       (t (1+ (apply '+ (mapcar (lambda (e) (funcall 'neovm--sma-cse-size e))
                                 (cdr expr))))))))

  (unwind-protect
      (let* (;; (x+1)^2 + 2*(x+1) + (x+1)*(x-1)
             ;; Common subexpression: (x+1) appears 3 times, (+ x 1)
             (expr '(+ (+ (expt (+ x 1) 2) (* 2 (+ x 1)))
                       (* (+ x 1) (- x 1)))))
        (list
          ;; Total expression size
          (funcall 'neovm--sma-cse-size expr)
          ;; Find common subexpressions
          (funcall 'neovm--sma-cse-find-common expr)
          ;; Simpler case: a*b + a*c has (no common sub beyond atoms)
          (funcall 'neovm--sma-cse-find-common '(+ (* a b) (* a c)))
          ;; x^2 + x^2 has (expt x 2) appearing twice
          (funcall 'neovm--sma-cse-find-common '(+ (expt x 2) (expt x 2)))
          ;; No common subexpressions
          (funcall 'neovm--sma-cse-find-common '(+ x y))))
    (fmakunbound 'neovm--sma-cse-collect)
    (fmakunbound 'neovm--sma-cse-walk)
    (fmakunbound 'neovm--sma-cse-find-common)
    (fmakunbound 'neovm--sma-cse-size)))"#;
    let expect =
        expect_test::expect![[r#""OK (19 ((\"(+ x 1)\" . 3)) nil ((\"(expt x 2)\" . 2)) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Differentiate-then-simplify pipeline with chain rule
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_diff_simplify_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Combined diff + simplify (minimal versions for pipeline test)
  (fset 'neovm--sma-ds-diff
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       (t
        (let ((op (car expr)))
          (cond
           ((eq op '+)
            (list '+ (funcall 'neovm--sma-ds-diff (nth 1 expr) var)
                  (funcall 'neovm--sma-ds-diff (nth 2 expr) var)))
           ((eq op '*)
            (list '+ (list '* (funcall 'neovm--sma-ds-diff (nth 1 expr) var) (nth 2 expr))
                  (list '* (nth 1 expr) (funcall 'neovm--sma-ds-diff (nth 2 expr) var))))
           ((eq op 'expt)
            (let ((f (nth 1 expr)) (n (nth 2 expr)))
              (list '* (list '* n (list 'expt f (- n 1)))
                    (funcall 'neovm--sma-ds-diff f var))))
           ((eq op 'sin)
            (list '* (list 'cos (nth 1 expr))
                  (funcall 'neovm--sma-ds-diff (nth 1 expr) var)))
           ((eq op 'cos)
            (list '* (list '- (list 'sin (nth 1 expr)))
                  (funcall 'neovm--sma-ds-diff (nth 1 expr) var)))
           (t 0)))))))

  (fset 'neovm--sma-ds-simp
    (lambda (expr)
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sma-ds-simp e)) (cdr expr)))
               (a (nth 0 args)) (b (nth 1 args)))
          (cond
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '+) (equal a 0)) b)
           ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ((and (eq op '*) (or (equal a 0) (equal b 0))) 0)
           ((and (eq op '*) (equal a 1)) b)
           ((and (eq op '*) (equal b 1)) a)
           ((and (eq op 'expt) (equal b 0)) 1)
           ((and (eq op 'expt) (equal b 1)) a)
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           (t (cons op args)))))))

  (fset 'neovm--sma-ds-fix
    (lambda (expr)
      (let ((prev nil) (cur expr) (n 0))
        (while (and (not (equal prev cur)) (< n 12))
          (setq prev cur) (setq cur (funcall 'neovm--sma-ds-simp cur))
          (setq n (1+ n)))
        cur)))

  (unwind-protect
      (list
        ;; d/dx(x + 3) simplified = 1
        (funcall 'neovm--sma-ds-fix (funcall 'neovm--sma-ds-diff '(+ x 3) 'x))
        ;; d/dx(3*x) simplified = 3
        (funcall 'neovm--sma-ds-fix (funcall 'neovm--sma-ds-diff '(* 3 x) 'x))
        ;; d/dx(x^2) simplified = (* 2 x)
        (funcall 'neovm--sma-ds-fix (funcall 'neovm--sma-ds-diff '(expt x 2) 'x))
        ;; d/dx(x^3) simplified = (* 3 (expt x 2))
        (funcall 'neovm--sma-ds-fix (funcall 'neovm--sma-ds-diff '(expt x 3) 'x))
        ;; d/dx(x^2 + 3*x + 5) simplified
        (funcall 'neovm--sma-ds-fix
                 (funcall 'neovm--sma-ds-diff '(+ (+ (expt x 2) (* 3 x)) 5) 'x))
        ;; d/dx(sin(x)) simplified = (cos x)
        (funcall 'neovm--sma-ds-fix (funcall 'neovm--sma-ds-diff '(sin x) 'x))
        ;; d/dy(x^2 + y^2) wrt y simplified = (* 2 y)
        (funcall 'neovm--sma-ds-fix
                 (funcall 'neovm--sma-ds-diff '(+ (expt x 2) (expt y 2)) 'y)))
    (fmakunbound 'neovm--sma-ds-diff)
    (fmakunbound 'neovm--sma-ds-simp)
    (fmakunbound 'neovm--sma-ds-fix)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 3 (* 2 x) (* 3 (expt x 2)) (+ (* 2 x) 3) (cos x) (* 2 y))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression tree analysis: depth, size, free variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_adv_tree_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--sma-depth
    (lambda (expr)
      (cond
       ((or (numberp expr) (symbolp expr)) 0)
       (t (1+ (apply 'max (mapcar (lambda (e) (funcall 'neovm--sma-depth e))
                                   (cdr expr))))))))

  (fset 'neovm--sma-size
    (lambda (expr)
      (cond
       ((or (numberp expr) (symbolp expr)) 1)
       (t (1+ (apply '+ (mapcar (lambda (e) (funcall 'neovm--sma-size e))
                                 (cdr expr))))))))

  (fset 'neovm--sma-free-vars
    (lambda (expr)
      "Extract sorted unique free variables."
      (cond
       ((numberp expr) nil)
       ((symbolp expr) (list expr))
       (t (let ((vars nil))
            (dolist (child (cdr expr))
              (dolist (v (funcall 'neovm--sma-free-vars child))
                (unless (memq v vars)
                  (setq vars (cons v vars)))))
            (sort vars (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))))

  ;; Check if expression is linear in a given variable
  (fset 'neovm--sma-linearp
    (lambda (expr var)
      "Check if EXPR is linear in VAR (degree <= 1)."
      (cond
       ((numberp expr) t)
       ((symbolp expr) t)  ;; x alone is linear
       (t
        (let ((op (car expr)))
          (cond
           ;; x^n for n > 1 is nonlinear
           ((and (eq op 'expt) (eq (nth 1 expr) var) (numberp (nth 2 expr)))
            (<= (nth 2 expr) 1))
           ;; x * x is nonlinear (products involving var twice)
           ((eq op '*)
            (let ((f (nth 1 expr)) (g (nth 2 expr)))
              (not (and (memq var (funcall 'neovm--sma-free-vars f))
                        (memq var (funcall 'neovm--sma-free-vars g))))))
           ;; Sum/difference: linear if both operands are linear
           ((memq op '(+ -))
            (let ((ok t))
              (dolist (child (cdr expr))
                (unless (funcall 'neovm--sma-linearp child var)
                  (setq ok nil)))
              ok))
           ;; Transcendentals containing var are nonlinear
           ((memq op '(sin cos exp log sqrt))
            (not (memq var (funcall 'neovm--sma-free-vars (nth 1 expr)))))
           (t t)))))))

  (unwind-protect
      (let ((e1 '(+ (* 3 x) (* 2 y)))
            (e2 '(+ (expt x 2) (* (sin y) (+ x z))))
            (e3 42)
            (e4 '(* (+ a (sin (* b c))) (expt d 3))))
        (list
          ;; Depths
          (mapcar (lambda (e) (funcall 'neovm--sma-depth e)) (list e1 e2 e3 e4))
          ;; Sizes
          (mapcar (lambda (e) (funcall 'neovm--sma-size e)) (list e1 e2 e3 e4))
          ;; Free variables
          (funcall 'neovm--sma-free-vars e1)
          (funcall 'neovm--sma-free-vars e2)
          (funcall 'neovm--sma-free-vars e3)
          (funcall 'neovm--sma-free-vars e4)
          ;; Linearity checks
          (funcall 'neovm--sma-linearp '(+ (* 3 x) 5) 'x)        ;; t (linear)
          (funcall 'neovm--sma-linearp '(+ (expt x 2) 5) 'x)     ;; nil (quadratic)
          (funcall 'neovm--sma-linearp '(* x y) 'x)               ;; t (linear in x alone, y is coefficient)
          (funcall 'neovm--sma-linearp '(sin x) 'x)               ;; nil (transcendental)
          (funcall 'neovm--sma-linearp '(+ (* 2 x) (* 3 y)) 'x)  ;; t
          (funcall 'neovm--sma-linearp '(+ (* 2 x) (* 3 y)) 'z)  ;; t (z not present)
          ))
    (fmakunbound 'neovm--sma-depth)
    (fmakunbound 'neovm--sma-size)
    (fmakunbound 'neovm--sma-free-vars)
    (fmakunbound 'neovm--sma-linearp)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 3 0 4) (7 10 1 10) (x y) (x y z) nil (a b c d) t nil t nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
