//! Oracle parity tests for symbolic differentiation in Elisp:
//! expression representation with (+, *, ^, sin, cos, ln, exp),
//! differentiation rules for each operator, chain rule for composition,
//! simplification of derivative expressions, partial derivatives,
//! and gradient computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core differentiation rules for algebraic and transcendental functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbolic_diff_core_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement d/dx for: constants, variables, +, *, ^, sin, cos, ln, exp
    // with chain rule applied automatically.
    let form = r#"(progn
  (fset 'neovm--sd-deriv
    (lambda (expr var)
      (cond
       ;; Constants
       ((numberp expr) 0)
       ;; Variables
       ((symbolp expr) (if (eq expr var) 1 0))
       ;; (+ a b): sum rule
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sd-deriv (nth 1 expr) var)
              (funcall 'neovm--sd-deriv (nth 2 expr) var)))
       ;; (- a b) or (- a): difference/negation
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (list '- (funcall 'neovm--sd-deriv (nth 1 expr) var))
          (list '- (funcall 'neovm--sd-deriv (nth 1 expr) var)
                (funcall 'neovm--sd-deriv (nth 2 expr) var))))
       ;; (* a b): product rule
       ((eq (car expr) '*)
        (let ((a (nth 1 expr)) (b (nth 2 expr)))
          (list '+ (list '* (funcall 'neovm--sd-deriv a var) b)
                (list '* a (funcall 'neovm--sd-deriv b var)))))
       ;; (/ a b): quotient rule
       ((eq (car expr) '/)
        (let ((a (nth 1 expr)) (b (nth 2 expr)))
          (list '/ (list '- (list '* (funcall 'neovm--sd-deriv a var) b)
                         (list '* a (funcall 'neovm--sd-deriv b var)))
                (list '^ b 2))))
       ;; (^ base power): power rule with chain rule
       ((eq (car expr) '^)
        (let ((base (nth 1 expr)) (power (nth 2 expr)))
          (cond
           ;; constant exponent: n*f^(n-1)*f'
           ((numberp power)
            (list '* (list '* power (list '^ base (list '- power 1)))
                  (funcall 'neovm--sd-deriv base var)))
           ;; constant base: a^g * ln(a) * g'
           ((numberp base)
            (list '* (list '* (list '^ base power) (list 'ln base))
                  (funcall 'neovm--sd-deriv power var)))
           ;; general: f^g = e^(g*ln(f)), use logarithmic diff
           (t (list '* expr
                    (list '+ (list '* (funcall 'neovm--sd-deriv power var)
                                   (list 'ln base))
                          (list '* power
                                (list '/ (funcall 'neovm--sd-deriv base var) base))))))))
       ;; (sin f): cos(f) * f'
       ((eq (car expr) 'sin)
        (list '* (list 'cos (nth 1 expr))
              (funcall 'neovm--sd-deriv (nth 1 expr) var)))
       ;; (cos f): -sin(f) * f'
       ((eq (car expr) 'cos)
        (list '* (list '- (list 'sin (nth 1 expr)))
              (funcall 'neovm--sd-deriv (nth 1 expr) var)))
       ;; (ln f): f'/f
       ((eq (car expr) 'ln)
        (list '/ (funcall 'neovm--sd-deriv (nth 1 expr) var)
              (nth 1 expr)))
       ;; (exp f): exp(f) * f'
       ((eq (car expr) 'exp)
        (list '* (list 'exp (nth 1 expr))
              (funcall 'neovm--sd-deriv (nth 1 expr) var)))
       (t (list 'diff expr var)))))

  (unwind-protect
      (list
       ;; d/dx(x) = 1
       (funcall 'neovm--sd-deriv 'x 'x)
       ;; d/dx(5) = 0
       (funcall 'neovm--sd-deriv 5 'x)
       ;; d/dx(x + y) = 1
       (funcall 'neovm--sd-deriv '(+ x y) 'x)
       ;; d/dx(x * y) = y (product rule unsimplified)
       (funcall 'neovm--sd-deriv '(* x y) 'x)
       ;; d/dx(x^3) = 3*x^2
       (funcall 'neovm--sd-deriv '(^ x 3) 'x)
       ;; d/dx(sin(x)) = cos(x)
       (funcall 'neovm--sd-deriv '(sin x) 'x)
       ;; d/dx(cos(x)) = -sin(x)
       (funcall 'neovm--sd-deriv '(cos x) 'x)
       ;; d/dx(ln(x)) = 1/x
       (funcall 'neovm--sd-deriv '(ln x) 'x)
       ;; d/dx(exp(x)) = exp(x)
       (funcall 'neovm--sd-deriv '(exp x) 'x)
       ;; d/dx(sin(x^2)) -- chain rule
       (funcall 'neovm--sd-deriv '(sin (^ x 2)) 'x)
       ;; d/dx(exp(3*x))
       (funcall 'neovm--sd-deriv '(exp (* 3 x)) 'x)
       ;; d/dx(ln(x^2 + 1))
       (funcall 'neovm--sd-deriv '(ln (+ (^ x 2) 1)) 'x))
    (fmakunbound 'neovm--sd-deriv)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 0 (+ 1 0) (+ (* 1 y) (* x 0)) (* (* 3 (^ x (- 3 1))) 1) (* (cos x) 1) (* (- (sin x)) 1) (/ 1 x) (* (exp x) 1) (* (cos (^ x 2)) (* (* 2 (^ x (- 2 1))) 1)) (* (exp (* 3 x)) (+ (* 0 x) (* 3 1))) (/ (+ (* (* 2 (^ x (- 2 1))) 1) 0) (+ (^ x 2) 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simplification of derivative expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbolic_diff_simplification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplify symbolic expressions produced by differentiation
    let form = r#"(progn
  (fset 'neovm--sd-simplify
    (lambda (expr)
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sd-simplify e))
                             (cdr expr)))
               (a (nth 0 args))
               (b (nth 1 args)))
          (cond
           ;; Arithmetic constants
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           ((and (eq op '-) (= (length args) 1) (numberp a)) (- a))
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ;; Additive identity
           ((and (eq op '+) (equal a 0)) b)
           ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 2) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 2) (equal a b)) 0)
           ;; Multiplicative identity/zero
           ((and (eq op '*) (equal a 0)) 0)
           ((and (eq op '*) (equal b 0)) 0)
           ((and (eq op '*) (equal a 1)) b)
           ((and (eq op '*) (equal b 1)) a)
           ;; Power rules
           ((and (eq op '^) (equal b 0)) 1)
           ((and (eq op '^) (equal b 1)) a)
           ;; Division by 1
           ((and (eq op '/) (equal b 1)) a)
           ;; Division of 0
           ((and (eq op '/) (equal a 0)) 0)
           ;; Double negation: (- (- x)) -> x
           ((and (eq op '-) (= (length args) 1)
                 (consp a) (eq (car a) '-) (= (length a) 2))
            (cadr a))
           ;; Default
           (t (cons op args)))))))

  ;; Apply simplification to fixed point
  (fset 'neovm--sd-simplify-fix
    (lambda (expr)
      (let ((prev nil) (current expr) (n 0))
        (while (and (not (equal prev current)) (< n 15))
          (setq prev current)
          (setq current (funcall 'neovm--sd-simplify current))
          (setq n (1+ n)))
        current)))

  ;; Differentiation engine
  (fset 'neovm--sd-deriv
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sd-deriv (nth 1 expr) var)
              (funcall 'neovm--sd-deriv (nth 2 expr) var)))
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (list '- (funcall 'neovm--sd-deriv (nth 1 expr) var))
          (list '- (funcall 'neovm--sd-deriv (nth 1 expr) var)
                (funcall 'neovm--sd-deriv (nth 2 expr) var))))
       ((eq (car expr) '*)
        (let ((a (nth 1 expr)) (b (nth 2 expr)))
          (list '+ (list '* (funcall 'neovm--sd-deriv a var) b)
                (list '* a (funcall 'neovm--sd-deriv b var)))))
       ((eq (car expr) '^)
        (let ((base (nth 1 expr)) (power (nth 2 expr)))
          (when (numberp power)
            (list '* (list '* power (list '^ base (list '- power 1)))
                  (funcall 'neovm--sd-deriv base var)))))
       ((eq (car expr) 'sin)
        (list '* (list 'cos (nth 1 expr))
              (funcall 'neovm--sd-deriv (nth 1 expr) var)))
       ((eq (car expr) 'cos)
        (list '* (list '- (list 'sin (nth 1 expr)))
              (funcall 'neovm--sd-deriv (nth 1 expr) var)))
       ((eq (car expr) 'ln)
        (list '/ (funcall 'neovm--sd-deriv (nth 1 expr) var)
              (nth 1 expr)))
       ((eq (car expr) 'exp)
        (list '* (list 'exp (nth 1 expr))
              (funcall 'neovm--sd-deriv (nth 1 expr) var)))
       (t (list 'diff expr var)))))

  (unwind-protect
      (list
       ;; d/dx(x + 3) simplified = 1
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(+ x 3) 'x))
       ;; d/dx(3*x) simplified = 3
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(* 3 x) 'x))
       ;; d/dx(x^2) simplified = (* 2 x)
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(^ x 2) 'x))
       ;; d/dx(x^3) simplified
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(^ x 3) 'x))
       ;; d/dx(sin(x)) simplified
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(sin x) 'x))
       ;; d/dx(cos(x)) simplified
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(cos x) 'x))
       ;; d/dx(ln(x)) simplified = (/ 1 x)
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(ln x) 'x))
       ;; d/dx(5*x^2 + 3*x + 7) simplified
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(+ (+ (* 5 (^ x 2)) (* 3 x)) 7) 'x))
       ;; d/dx(exp(x)) simplified = (exp x)
       (funcall 'neovm--sd-simplify-fix
                (funcall 'neovm--sd-deriv '(exp x) 'x)))
    (fmakunbound 'neovm--sd-simplify)
    (fmakunbound 'neovm--sd-simplify-fix)
    (fmakunbound 'neovm--sd-deriv)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 3 (* 2 x) (* 3 (^ x 2)) (cos x) (- (sin x)) (/ 1 x) (+ (* 5 (* 2 x)) 3) (exp x))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chain rule for composition of functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbolic_diff_chain_rule() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test chain rule on complex composed expressions
    let form = r#"(progn
  (fset 'neovm--sd-d
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sd-d (nth 1 expr) var)
              (funcall 'neovm--sd-d (nth 2 expr) var)))
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (list '- (funcall 'neovm--sd-d (nth 1 expr) var))
          (list '- (funcall 'neovm--sd-d (nth 1 expr) var)
                (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '*)
        (list '+ (list '* (funcall 'neovm--sd-d (nth 1 expr) var) (nth 2 expr))
              (list '* (nth 1 expr) (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '^)
        (when (numberp (nth 2 expr))
          (list '* (list '* (nth 2 expr) (list '^ (nth 1 expr) (- (nth 2 expr) 1)))
                (funcall 'neovm--sd-d (nth 1 expr) var))))
       ((eq (car expr) 'sin)
        (list '* (list 'cos (nth 1 expr))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'cos)
        (list '* (list '- (list 'sin (nth 1 expr)))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'exp)
        (list '* (list 'exp (nth 1 expr))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'ln)
        (list '/ (funcall 'neovm--sd-d (nth 1 expr) var)
              (nth 1 expr)))
       (t (list 'diff expr var)))))

  (unwind-protect
      (list
       ;; d/dx(sin(x^2)) = cos(x^2) * 2x
       (funcall 'neovm--sd-d '(sin (^ x 2)) 'x)
       ;; d/dx(cos(3*x)) = -sin(3*x) * 3
       (funcall 'neovm--sd-d '(cos (* 3 x)) 'x)
       ;; d/dx(exp(x^2 + x)) = exp(x^2+x) * (2x+1)
       (funcall 'neovm--sd-d '(exp (+ (^ x 2) x)) 'x)
       ;; d/dx(ln(sin(x))) = cos(x)/sin(x) = cot(x) (unsimplified)
       (funcall 'neovm--sd-d '(ln (sin x)) 'x)
       ;; d/dx((x^2 + 1)^3) = 3*(x^2+1)^2 * 2x
       (funcall 'neovm--sd-d '(^ (+ (^ x 2) 1) 3) 'x)
       ;; d/dx(sin(cos(x))) = cos(cos(x)) * (-sin(x))
       (funcall 'neovm--sd-d '(sin (cos x)) 'x)
       ;; d/dx(exp(sin(x))) = exp(sin(x)) * cos(x)
       (funcall 'neovm--sd-d '(exp (sin x)) 'x))
    (fmakunbound 'neovm--sd-d)))"#;
    let expect = expect_test::expect![[
        r#""OK ((* (cos (^ x 2)) (* (* 2 (^ x 1)) 1)) (* (- (sin (* 3 x))) (+ (* 0 x) (* 3 1))) (* (exp (+ (^ x 2) x)) (+ (* (* 2 (^ x 1)) 1) 1)) (/ (* (cos x) 1) (sin x)) (* (* 3 (^ (+ (^ x 2) 1) 2)) (+ (* (* 2 (^ x 1)) 1) 0)) (* (cos (cos x)) (* (- (sin x)) 1)) (* (exp (sin x)) (* (cos x) 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partial derivatives with multiple variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbolic_diff_partial_derivatives() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute partial derivatives of multi-variable expressions
    let form = r#"(progn
  (fset 'neovm--sd-d
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sd-d (nth 1 expr) var)
              (funcall 'neovm--sd-d (nth 2 expr) var)))
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (list '- (funcall 'neovm--sd-d (nth 1 expr) var))
          (list '- (funcall 'neovm--sd-d (nth 1 expr) var)
                (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '*)
        (list '+ (list '* (funcall 'neovm--sd-d (nth 1 expr) var) (nth 2 expr))
              (list '* (nth 1 expr) (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '^)
        (when (numberp (nth 2 expr))
          (list '* (list '* (nth 2 expr) (list '^ (nth 1 expr) (- (nth 2 expr) 1)))
                (funcall 'neovm--sd-d (nth 1 expr) var))))
       ((eq (car expr) 'sin)
        (list '* (list 'cos (nth 1 expr))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'cos)
        (list '* (list '- (list 'sin (nth 1 expr)))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'exp)
        (list '* (list 'exp (nth 1 expr))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'ln)
        (list '/ (funcall 'neovm--sd-d (nth 1 expr) var)
              (nth 1 expr)))
       (t 0))))

  ;; Simplifier
  (fset 'neovm--sd-s
    (lambda (expr)
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sd-s e)) (cdr expr)))
               (a (nth 0 args)) (b (nth 1 args)))
          (cond
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           ((and (eq op '-) (= (length args) 1) (numberp a)) (- a))
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ((and (eq op '+) (equal a 0)) b) ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 2) (equal b 0)) a)
           ((and (eq op '*) (equal a 0)) 0) ((and (eq op '*) (equal b 0)) 0)
           ((and (eq op '*) (equal a 1)) b) ((and (eq op '*) (equal b 1)) a)
           ((and (eq op '^) (equal b 0)) 1) ((and (eq op '^) (equal b 1)) a)
           ((and (eq op '/) (equal a 0)) 0) ((and (eq op '/) (equal b 1)) a)
           (t (cons op args)))))))

  (fset 'neovm--sd-sf
    (lambda (expr)
      (let ((prev nil) (cur expr) (n 0))
        (while (and (not (equal prev cur)) (< n 10))
          (setq prev cur) (setq cur (funcall 'neovm--sd-s cur)) (setq n (1+ n)))
        cur)))

  (unwind-protect
      (let ((f1 '(+ (* x x) (* y y)))           ;; x^2 + y^2
            (f2 '(* x (* y z)))                   ;; x*y*z
            (f3 '(+ (* 3 (* x x)) (* 2 (* x y))));; 3x^2 + 2xy
            (f4 '(sin (+ (* x x) (* y y)))))      ;; sin(x^2 + y^2)
        (list
         ;; df1/dx = 2x (simplified)
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f1 'x))
         ;; df1/dy = 2y
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f1 'y))
         ;; df2/dx = y*z
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f2 'x))
         ;; df2/dy = x*z
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f2 'y))
         ;; df2/dz = x*y
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f2 'z))
         ;; df3/dx = 6x + 2y
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f3 'x))
         ;; df3/dy = 2x
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f3 'y))
         ;; df4/dx = cos(x^2+y^2)*2x (unsimplified chain rule)
         (funcall 'neovm--sd-sf (funcall 'neovm--sd-d f4 'x))
         ;; Second partial: d^2f1/dx^2 = 2
         (funcall 'neovm--sd-sf
                  (funcall 'neovm--sd-d
                           (funcall 'neovm--sd-d f1 'x) 'x))
         ;; Mixed partial: d^2f3/dxdy
         (funcall 'neovm--sd-sf
                  (funcall 'neovm--sd-d
                           (funcall 'neovm--sd-d f3 'x) 'y))))
    (fmakunbound 'neovm--sd-d)
    (fmakunbound 'neovm--sd-s)
    (fmakunbound 'neovm--sd-sf)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ x x) (+ y y) (* y z) (* x z) (* x y) (+ (* 3 (+ x x)) (* 2 y)) (* 2 x) (* (cos (+ (* x x) (* y y))) (+ x x)) 2 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gradient computation (vector of partial derivatives)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbolic_diff_gradient() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the gradient (vector of all partial derivatives) of a
    // scalar function, and compute directional derivatives.
    let form = r#"(progn
  (fset 'neovm--sd-d
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sd-d (nth 1 expr) var)
              (funcall 'neovm--sd-d (nth 2 expr) var)))
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (list '- (funcall 'neovm--sd-d (nth 1 expr) var))
          (list '- (funcall 'neovm--sd-d (nth 1 expr) var)
                (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '*)
        (list '+ (list '* (funcall 'neovm--sd-d (nth 1 expr) var) (nth 2 expr))
              (list '* (nth 1 expr) (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '^)
        (when (numberp (nth 2 expr))
          (list '* (list '* (nth 2 expr) (list '^ (nth 1 expr) (- (nth 2 expr) 1)))
                (funcall 'neovm--sd-d (nth 1 expr) var))))
       ((eq (car expr) 'sin)
        (list '* (list 'cos (nth 1 expr))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'cos)
        (list '* (list '- (list 'sin (nth 1 expr)))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       (t 0))))

  (fset 'neovm--sd-s
    (lambda (expr)
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sd-s e)) (cdr expr)))
               (a (nth 0 args)) (b (nth 1 args)))
          (cond
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           ((and (eq op '-) (= (length args) 1) (numberp a)) (- a))
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ((and (eq op '+) (equal a 0)) b) ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 2) (equal b 0)) a)
           ((and (eq op '*) (equal a 0)) 0) ((and (eq op '*) (equal b 0)) 0)
           ((and (eq op '*) (equal a 1)) b) ((and (eq op '*) (equal b 1)) a)
           ((and (eq op '^) (equal b 0)) 1) ((and (eq op '^) (equal b 1)) a)
           (t (cons op args)))))))

  (fset 'neovm--sd-sf
    (lambda (expr)
      (let ((prev nil) (cur expr) (n 0))
        (while (and (not (equal prev cur)) (< n 10))
          (setq prev cur) (setq cur (funcall 'neovm--sd-s cur)) (setq n (1+ n)))
        cur)))

  ;; Gradient: list of partial derivatives wrt each variable
  (fset 'neovm--sd-gradient
    (lambda (expr vars)
      (mapcar (lambda (var)
                (funcall 'neovm--sd-sf (funcall 'neovm--sd-d expr var)))
              vars)))

  ;; Laplacian: sum of second partial derivatives
  (fset 'neovm--sd-laplacian
    (lambda (expr vars)
      (let ((result 0))
        (dolist (var vars)
          (let ((second-deriv
                  (funcall 'neovm--sd-sf
                           (funcall 'neovm--sd-d
                                    (funcall 'neovm--sd-d expr var)
                                    var))))
            (setq result (if (equal result 0) second-deriv
                           (list '+ result second-deriv)))))
        (funcall 'neovm--sd-sf result))))

  (unwind-protect
      (let ((vars-2d '(x y))
            (vars-3d '(x y z)))
        (list
         ;; Gradient of x^2 + y^2 = (2x, 2y)
         (funcall 'neovm--sd-gradient '(+ (* x x) (* y y)) vars-2d)
         ;; Gradient of x*y = (y, x)
         (funcall 'neovm--sd-gradient '(* x y) vars-2d)
         ;; Gradient of 3x + 2y - z = (3, 2, -1)
         (funcall 'neovm--sd-gradient '(- (+ (* 3 x) (* 2 y)) z) vars-3d)
         ;; Gradient of x*y*z
         (funcall 'neovm--sd-gradient '(* x (* y z)) vars-3d)
         ;; Laplacian of x^2 + y^2 = 2 + 2 = 4
         (funcall 'neovm--sd-laplacian '(+ (* x x) (* y y)) vars-2d)
         ;; Laplacian of x^2 + y^2 + z^2 = 6
         (funcall 'neovm--sd-laplacian
                  '(+ (+ (* x x) (* y y)) (* z z)) vars-3d)
         ;; Laplacian of x*y = 0 + 0 = 0
         (funcall 'neovm--sd-laplacian '(* x y) vars-2d)))
    (fmakunbound 'neovm--sd-d)
    (fmakunbound 'neovm--sd-s)
    (fmakunbound 'neovm--sd-sf)
    (fmakunbound 'neovm--sd-gradient)
    (fmakunbound 'neovm--sd-laplacian)))"#;
    let expect = expect_test::expect![[
        r#""OK (((+ x x) (+ y y)) (y x) (3 2 -1) ((* y z) (* x z) (* x y)) 4 6 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Quotient rule and complex expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbolic_diff_quotient_and_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test quotient rule and more complex expression differentiation
    let form = r#"(progn
  (fset 'neovm--sd-d
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sd-d (nth 1 expr) var)
              (funcall 'neovm--sd-d (nth 2 expr) var)))
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (list '- (funcall 'neovm--sd-d (nth 1 expr) var))
          (list '- (funcall 'neovm--sd-d (nth 1 expr) var)
                (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '*)
        (list '+ (list '* (funcall 'neovm--sd-d (nth 1 expr) var) (nth 2 expr))
              (list '* (nth 1 expr) (funcall 'neovm--sd-d (nth 2 expr) var))))
       ((eq (car expr) '/)
        (let ((a (nth 1 expr)) (b (nth 2 expr)))
          (list '/ (list '- (list '* (funcall 'neovm--sd-d a var) b)
                         (list '* a (funcall 'neovm--sd-d b var)))
                (list '* b b))))
       ((eq (car expr) '^)
        (when (numberp (nth 2 expr))
          (list '* (list '* (nth 2 expr) (list '^ (nth 1 expr) (- (nth 2 expr) 1)))
                (funcall 'neovm--sd-d (nth 1 expr) var))))
       ((eq (car expr) 'sin)
        (list '* (list 'cos (nth 1 expr))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       ((eq (car expr) 'cos)
        (list '* (list '- (list 'sin (nth 1 expr)))
              (funcall 'neovm--sd-d (nth 1 expr) var)))
       (t 0))))

  (fset 'neovm--sd-s
    (lambda (expr)
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sd-s e)) (cdr expr)))
               (a (nth 0 args)) (b (nth 1 args)))
          (cond
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           ((and (eq op '-) (= (length args) 1) (numberp a)) (- a))
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ((and (eq op '+) (equal a 0)) b) ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 2) (equal b 0)) a)
           ((and (eq op '*) (equal a 0)) 0) ((and (eq op '*) (equal b 0)) 0)
           ((and (eq op '*) (equal a 1)) b) ((and (eq op '*) (equal b 1)) a)
           ((and (eq op '^) (equal b 0)) 1) ((and (eq op '^) (equal b 1)) a)
           ((and (eq op '/) (equal a 0)) 0) ((and (eq op '/) (equal b 1)) a)
           (t (cons op args)))))))

  (fset 'neovm--sd-sf
    (lambda (expr)
      (let ((prev nil) (cur expr) (n 0))
        (while (and (not (equal prev cur)) (< n 10))
          (setq prev cur) (setq cur (funcall 'neovm--sd-s cur)) (setq n (1+ n)))
        cur)))

  (unwind-protect
      (list
       ;; d/dx(x / y) = 1/y (y is constant wrt x)
       (funcall 'neovm--sd-sf (funcall 'neovm--sd-d '(/ x y) 'x))
       ;; d/dx(1 / x) = -1/x^2
       (funcall 'neovm--sd-sf (funcall 'neovm--sd-d '(/ 1 x) 'x))
       ;; d/dx(x / (x + 1)) quotient rule
       (funcall 'neovm--sd-sf (funcall 'neovm--sd-d '(/ x (+ x 1)) 'x))
       ;; d/dx((x^2 - 1) / (x + 1))
       (funcall 'neovm--sd-sf
                (funcall 'neovm--sd-d '(/ (- (* x x) 1) (+ x 1)) 'x))
       ;; d/dx(sin(x) * cos(x))
       (funcall 'neovm--sd-sf
                (funcall 'neovm--sd-d '(* (sin x) (cos x)) 'x))
       ;; d/dx(x^2 * sin(x)) -- product of polynomial and trig
       (funcall 'neovm--sd-sf
                (funcall 'neovm--sd-d '(* (* x x) (sin x)) 'x)))
    (fmakunbound 'neovm--sd-d)
    (fmakunbound 'neovm--sd-s)
    (fmakunbound 'neovm--sd-sf)))"#;
    let expect = expect_test::expect![[
        r#""OK ((/ y (* y y)) (/ -1 (* x x)) (/ (- (+ x 1) x) (* (+ x 1) (+ x 1))) (/ (- (* (+ x x) (+ x 1)) (- (* x x) 1)) (* (+ x 1) (+ x 1))) (+ (* (cos x) (cos x)) (* (sin x) (- (sin x)))) (+ (* (+ x x) (sin x)) (* (* x x) (cos x))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
