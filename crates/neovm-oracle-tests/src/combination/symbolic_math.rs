//! Oracle parity tests for symbolic mathematics in Elisp:
//! expressions represented as S-expressions, symbolic differentiation (d/dx),
//! simplification rules, expression evaluation with variable bindings,
//! polynomial operations (add, multiply), and symbolic integration of
//! simple forms.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Symbolic differentiation: d/dx of algebraic expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_differentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement d/dx for: constants, variables, +, -, *, /, expt
    // Expressions: number | symbol | (+ e1 e2) | (* e1 e2) | (expt e n)
    let form = r#"(progn
  (fset 'neovm--sm-deriv
    (lambda (expr var)
      "Compute symbolic derivative of EXPR with respect to VAR."
      (cond
       ;; Constant: d/dx(c) = 0
       ((numberp expr) 0)
       ;; Variable: d/dx(x) = 1, d/dx(y) = 0
       ((symbolp expr)
        (if (eq expr var) 1 0))
       ;; Sum: d/dx(a+b) = d/dx(a) + d/dx(b)
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sm-deriv (nth 1 expr) var)
              (funcall 'neovm--sm-deriv (nth 2 expr) var)))
       ;; Difference: d/dx(a-b) = d/dx(a) - d/dx(b)
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            ;; Unary minus: d/dx(-a) = -(d/dx(a))
            (list '- (funcall 'neovm--sm-deriv (nth 1 expr) var))
          (list '- (funcall 'neovm--sm-deriv (nth 1 expr) var)
                (funcall 'neovm--sm-deriv (nth 2 expr) var))))
       ;; Product rule: d/dx(a*b) = a'*b + a*b'
       ((eq (car expr) '*)
        (let ((a (nth 1 expr))
              (b (nth 2 expr)))
          (list '+ (list '* (funcall 'neovm--sm-deriv a var) b)
                (list '* a (funcall 'neovm--sm-deriv b var)))))
       ;; Power rule: d/dx(x^n) = n*x^(n-1)*d/dx(x)
       ((eq (car expr) 'expt)
        (let ((base (nth 1 expr))
              (power (nth 2 expr)))
          (list '* (list '* power (list 'expt base (list '- power 1)))
                (funcall 'neovm--sm-deriv base var))))
       (t (list 'unknown-deriv expr)))))

  (unwind-protect
      (list
       ;; d/dx(5) = 0
       (funcall 'neovm--sm-deriv 5 'x)
       ;; d/dx(x) = 1
       (funcall 'neovm--sm-deriv 'x 'x)
       ;; d/dx(y) = 0 (different variable)
       (funcall 'neovm--sm-deriv 'y 'x)
       ;; d/dx(x + 3) = 1 + 0
       (funcall 'neovm--sm-deriv '(+ x 3) 'x)
       ;; d/dx(x * x) = 1*x + x*1 (unsimplified)
       (funcall 'neovm--sm-deriv '(* x x) 'x)
       ;; d/dx(3*x) = 0*x + 3*1
       (funcall 'neovm--sm-deriv '(* 3 x) 'x)
       ;; d/dx(x^3) = 3*x^(3-1)*1
       (funcall 'neovm--sm-deriv '(expt x 3) 'x)
       ;; d/dx(x^2 + 3*x + 5)
       (funcall 'neovm--sm-deriv '(+ (+ (expt x 2) (* 3 x)) 5) 'x))
    (fmakunbound 'neovm--sm-deriv)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 1 0 (+ 1 0) (+ (* 1 x) (* x 1)) (+ (* 0 x) (* 3 1)) (* (* 3 (expt x (- 3 1))) 1) (+ (+ (* (* 2 (expt x (- 2 1))) 1) (+ (* 0 x) (* 3 1))) 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Algebraic simplification rules
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_simplification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplify algebraic expressions by applying rewrite rules repeatedly
    let form = r#"(progn
  (fset 'neovm--sm-simplify
    (lambda (expr)
      "Simplify algebraic expression by applying rewrite rules."
      (if (or (numberp expr) (symbolp expr))
          expr
        ;; First simplify sub-expressions
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sm-simplify e))
                             (cdr expr)))
               (a (nth 0 args))
               (b (nth 1 args)))
          (cond
           ;; Arithmetic on constants
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ;; x + 0 = x, 0 + x = x
           ((and (eq op '+) (equal a 0)) b)
           ((and (eq op '+) (equal b 0)) a)
           ;; x - 0 = x
           ((and (eq op '-) (= (length args) 2) (equal b 0)) a)
           ;; x - x = 0
           ((and (eq op '-) (= (length args) 2) (equal a b)) 0)
           ;; x * 0 = 0, 0 * x = 0
           ((and (eq op '*) (or (equal a 0) (equal b 0))) 0)
           ;; x * 1 = x, 1 * x = x
           ((and (eq op '*) (equal a 1)) b)
           ((and (eq op '*) (equal b 1)) a)
           ;; x^0 = 1, x^1 = x
           ((and (eq op 'expt) (equal b 0)) 1)
           ((and (eq op 'expt) (equal b 1)) a)
           ;; - 0 = 0 (unary)
           ((and (eq op '-) (= (length args) 1) (equal a 0)) 0)
           ;; Default: reconstruct
           (t (cons op args)))))))

  (unwind-protect
      (list
       ;; (+ 3 4) -> 7
       (funcall 'neovm--sm-simplify '(+ 3 4))
       ;; (+ x 0) -> x
       (funcall 'neovm--sm-simplify '(+ x 0))
       ;; (* x 1) -> x
       (funcall 'neovm--sm-simplify '(* x 1))
       ;; (* x 0) -> 0
       (funcall 'neovm--sm-simplify '(* x 0))
       ;; (expt x 1) -> x
       (funcall 'neovm--sm-simplify '(expt x 1))
       ;; (expt x 0) -> 1
       (funcall 'neovm--sm-simplify '(expt x 0))
       ;; (+ (* 0 x) (* 3 1)) -> 3
       (funcall 'neovm--sm-simplify '(+ (* 0 x) (* 3 1)))
       ;; (- x x) -> 0
       (funcall 'neovm--sm-simplify '(- x x))
       ;; Nested: (+ (+ 0 (* 1 x)) (* x 0)) -> x
       (funcall 'neovm--sm-simplify '(+ (+ 0 (* 1 x)) (* x 0)))
       ;; (+ (* 2 3) (* 1 x)) -> (+ 6 x)
       (funcall 'neovm--sm-simplify '(+ (* 2 3) (* 1 x))))
    (fmakunbound 'neovm--sm-simplify)))"#;
    let expect = expect_test::expect![[r#""OK (7 x x 0 x 1 3 0 x (+ 6 x))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression evaluation with variable bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_eval_with_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate symbolic expressions given an environment (alist of bindings)
    let form = r#"(progn
  (fset 'neovm--sm-eval
    (lambda (expr env)
      "Evaluate symbolic expression with variable bindings from ENV alist."
      (cond
       ((numberp expr) expr)
       ((symbolp expr)
        (let ((binding (assq expr env)))
          (if binding (cdr binding)
            (signal 'error (list "unbound variable" expr)))))
       (t
        (let ((op (car expr))
              (args (mapcar (lambda (e) (funcall 'neovm--sm-eval e env))
                            (cdr expr))))
          (cond
           ((eq op '+) (apply '+ args))
           ((eq op '-) (apply '- args))
           ((eq op '*) (apply '* args))
           ((eq op '/) (/ (car args) (cadr args)))
           ((eq op 'expt) (expt (car args) (cadr args)))
           ((eq op 'sqrt) (sqrt (car args)))
           ((eq op 'abs) (abs (car args)))
           (t (signal 'error (list "unknown op" op)))))))))

  (unwind-protect
      (let ((env '((x . 3) (y . 4) (z . 5))))
        (list
         ;; Simple: x -> 3
         (funcall 'neovm--sm-eval 'x env)
         ;; (+ x y) -> 7
         (funcall 'neovm--sm-eval '(+ x y) env)
         ;; (* x (* y z)) -> 60
         (funcall 'neovm--sm-eval '(* x (* y z)) env)
         ;; (expt x 2) -> 9
         (funcall 'neovm--sm-eval '(expt x 2) env)
         ;; Pythagorean: sqrt(x^2 + y^2) = 5.0
         (funcall 'neovm--sm-eval '(sqrt (+ (expt x 2) (expt y 2))) env)
         ;; Polynomial: 2x^2 + 3x + 1 at x=3 -> 28
         (funcall 'neovm--sm-eval '(+ (+ (* 2 (expt x 2)) (* 3 x)) 1) env)
         ;; Nested: (x + y) * (x - y) at x=3, y=4 -> -7
         (funcall 'neovm--sm-eval '(* (+ x y) (- x y)) env)
         ;; Error case: unbound variable
         (condition-case err
             (funcall 'neovm--sm-eval 'w env)
           (error (list 'error (cadr err))))))
    (fmakunbound 'neovm--sm-eval)))"#;
    let expect =
        expect_test::expect![[r#""OK (3 7 60 9 5.0 28 -7 (error \"unbound variable\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial operations: representation, addition, multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_polynomial_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Polynomials represented as sorted alist: ((degree . coeff) ...)
    // e.g., 3x^2 + 2x + 1 = ((2 . 3) (1 . 2) (0 . 1))
    let form = r#"(progn
  ;; Get coefficient for a given degree
  (fset 'neovm--sm-poly-coeff
    (lambda (poly deg)
      (let ((entry (assq deg poly)))
        (if entry (cdr entry) 0))))

  ;; Normalize: remove zero coefficients, sort by degree descending
  (fset 'neovm--sm-poly-normalize
    (lambda (poly)
      (let ((filtered nil))
        (dolist (term poly)
          (unless (= (cdr term) 0)
            (setq filtered (cons term filtered))))
        (sort filtered (lambda (a b) (> (car a) (car b)))))))

  ;; Add two polynomials
  (fset 'neovm--sm-poly-add
    (lambda (p1 p2)
      (let ((result nil)
            (all-degrees nil))
        ;; Collect all degrees
        (dolist (term p1) (unless (memq (car term) all-degrees)
                            (setq all-degrees (cons (car term) all-degrees))))
        (dolist (term p2) (unless (memq (car term) all-degrees)
                            (setq all-degrees (cons (car term) all-degrees))))
        ;; Sum coefficients
        (dolist (deg all-degrees)
          (let ((sum (+ (funcall 'neovm--sm-poly-coeff p1 deg)
                        (funcall 'neovm--sm-poly-coeff p2 deg))))
            (setq result (cons (cons deg sum) result))))
        (funcall 'neovm--sm-poly-normalize result))))

  ;; Multiply two polynomials
  (fset 'neovm--sm-poly-mul
    (lambda (p1 p2)
      (let ((result nil))
        (dolist (t1 p1)
          (dolist (t2 p2)
            (let* ((deg (+ (car t1) (car t2)))
                   (coeff (* (cdr t1) (cdr t2)))
                   (existing (assq deg result)))
              (if existing
                  (setcdr existing (+ (cdr existing) coeff))
                (setq result (cons (cons deg coeff) result))))))
        (funcall 'neovm--sm-poly-normalize result))))

  ;; Evaluate polynomial at a point
  (fset 'neovm--sm-poly-eval
    (lambda (poly x)
      (let ((result 0))
        (dolist (term poly)
          (setq result (+ result (* (cdr term) (expt x (car term))))))
        result)))

  ;; Format polynomial as string
  (fset 'neovm--sm-poly-to-string
    (lambda (poly)
      (if (null poly) "0"
        (let ((parts nil))
          (dolist (term poly)
            (let ((deg (car term)) (coeff (cdr term)))
              (cond
               ((= deg 0) (setq parts (cons (number-to-string coeff) parts)))
               ((= deg 1)
                (if (= coeff 1) (setq parts (cons "x" parts))
                  (setq parts (cons (concat (number-to-string coeff) "x") parts))))
               (t
                (if (= coeff 1)
                    (setq parts (cons (concat "x^" (number-to-string deg)) parts))
                  (setq parts (cons (concat (number-to-string coeff) "x^"
                                            (number-to-string deg)) parts)))))))
          (mapconcat #'identity (nreverse parts) " + ")))))

  (unwind-protect
      (let ((p1 '((2 . 3) (1 . 2) (0 . 1)))   ;; 3x^2 + 2x + 1
            (p2 '((2 . 1) (1 . -1) (0 . 4)))   ;; x^2 - x + 4
            (p3 '((1 . 1) (0 . 1)))             ;; x + 1
            (p4 '((1 . 1) (0 . -1))))           ;; x - 1
        (list
         ;; p1 + p2 = 4x^2 + x + 5
         (funcall 'neovm--sm-poly-add p1 p2)
         ;; p3 * p4 = x^2 - 1 (difference of squares)
         (funcall 'neovm--sm-poly-mul p3 p4)
         ;; p1 * p3 = 3x^3 + 5x^2 + 3x + 1
         (funcall 'neovm--sm-poly-mul p1 p3)
         ;; Evaluate p1 at x=2: 3*4 + 2*2 + 1 = 17
         (funcall 'neovm--sm-poly-eval p1 2)
         ;; Evaluate p1 at x=0: 1
         (funcall 'neovm--sm-poly-eval p1 0)
         ;; Evaluate (x+1)*(x-1) at x=5: 24
         (funcall 'neovm--sm-poly-eval
                  (funcall 'neovm--sm-poly-mul p3 p4) 5)
         ;; String representation
         (funcall 'neovm--sm-poly-to-string p1)
         (funcall 'neovm--sm-poly-to-string
                  (funcall 'neovm--sm-poly-mul p3 p4))))
    (fmakunbound 'neovm--sm-poly-coeff)
    (fmakunbound 'neovm--sm-poly-normalize)
    (fmakunbound 'neovm--sm-poly-add)
    (fmakunbound 'neovm--sm-poly-mul)
    (fmakunbound 'neovm--sm-poly-eval)
    (fmakunbound 'neovm--sm-poly-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK (((2 . 4) (1 . 1) (0 . 5)) ((2 . 1) (0 . -1)) ((3 . 3) (2 . 5) (1 . 3) (0 . 1)) 17 1 24 \"3x^2 + 2x + 1\" \"x^2 + -1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Differentiate-then-simplify pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_deriv_then_simplify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full pipeline: differentiate, then simplify the result
    let form = r#"(progn
  (fset 'neovm--sm-d
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sm-d (nth 1 expr) var)
              (funcall 'neovm--sm-d (nth 2 expr) var)))
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (list '- (funcall 'neovm--sm-d (nth 1 expr) var))
          (list '- (funcall 'neovm--sm-d (nth 1 expr) var)
                (funcall 'neovm--sm-d (nth 2 expr) var))))
       ((eq (car expr) '*)
        (list '+ (list '* (funcall 'neovm--sm-d (nth 1 expr) var) (nth 2 expr))
              (list '* (nth 1 expr) (funcall 'neovm--sm-d (nth 2 expr) var))))
       ((eq (car expr) 'expt)
        (list '* (list '* (nth 2 expr) (list 'expt (nth 1 expr) (- (nth 2 expr) 1)))
              (funcall 'neovm--sm-d (nth 1 expr) var)))
       (t expr))))

  (fset 'neovm--sm-s
    (lambda (expr)
      (if (or (numberp expr) (symbolp expr)) expr
        (let* ((op (car expr))
               (args (mapcar (lambda (e) (funcall 'neovm--sm-s e)) (cdr expr)))
               (a (nth 0 args))
               (b (nth 1 args)))
          (cond
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ((and (eq op '+) (equal a 0)) b)
           ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 2) (equal b 0)) a)
           ((and (eq op '*) (or (equal a 0) (equal b 0))) 0)
           ((and (eq op '*) (equal a 1)) b)
           ((and (eq op '*) (equal b 1)) a)
           ((and (eq op 'expt) (equal b 0)) 1)
           ((and (eq op 'expt) (equal b 1)) a)
           (t (cons op args)))))))

  ;; Apply simplification repeatedly until stable
  (fset 'neovm--sm-simplify-fix
    (lambda (expr)
      (let ((prev nil) (current expr) (n 0))
        (while (and (not (equal prev current)) (< n 10))
          (setq prev current)
          (setq current (funcall 'neovm--sm-s current))
          (setq n (1+ n)))
        current)))

  (unwind-protect
      (list
       ;; d/dx(x + 3) simplified: should be 1
       (funcall 'neovm--sm-simplify-fix
                (funcall 'neovm--sm-d '(+ x 3) 'x))
       ;; d/dx(3*x) simplified: should be 3
       (funcall 'neovm--sm-simplify-fix
                (funcall 'neovm--sm-d '(* 3 x) 'x))
       ;; d/dx(x^2) simplified
       (funcall 'neovm--sm-simplify-fix
                (funcall 'neovm--sm-d '(expt x 2) 'x))
       ;; d/dx(x^2 + x) simplified
       (funcall 'neovm--sm-simplify-fix
                (funcall 'neovm--sm-d '(+ (expt x 2) x) 'x))
       ;; d/dx(5) = 0
       (funcall 'neovm--sm-simplify-fix
                (funcall 'neovm--sm-d 5 'x))
       ;; d/dx(2*x + 3*x) simplified
       (funcall 'neovm--sm-simplify-fix
                (funcall 'neovm--sm-d '(+ (* 2 x) (* 3 x)) 'x))
       ;; d/dy(x^2 + y) wrt y = 1
       (funcall 'neovm--sm-simplify-fix
                (funcall 'neovm--sm-d '(+ (expt x 2) y) 'y)))
    (fmakunbound 'neovm--sm-d)
    (fmakunbound 'neovm--sm-s)
    (fmakunbound 'neovm--sm-simplify-fix)))"#;
    let expect = expect_test::expect![[r#""OK (1 3 (* 2 x) (+ (* 2 x) 1) 0 5 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbolic integration of simple polynomial forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_simple_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Symbolic integration for: constants, x, x^n, sums
    // integral(c, x) = c*x
    // integral(x, x) = x^2/2
    // integral(x^n, x) = x^(n+1)/(n+1) for n != -1
    // integral(a+b, x) = integral(a,x) + integral(b,x)
    // integral(c*f, x) = c * integral(f, x)
    let form = r#"(progn
  (fset 'neovm--sm-integrate
    (lambda (expr var)
      "Symbolic integration of EXPR with respect to VAR."
      (cond
       ;; integral(c) = c*x
       ((numberp expr)
        (if (= expr 0) 0
          (list '* expr var)))
       ;; integral(x) = x^2 / 2
       ((and (symbolp expr) (eq expr var))
        (list '/ (list 'expt var 2) 2))
       ;; integral(y) where y != x = y*x
       ((symbolp expr)
        (list '* expr var))
       ;; integral(a + b) = integral(a) + integral(b)
       ((eq (car expr) '+)
        (list '+ (funcall 'neovm--sm-integrate (nth 1 expr) var)
              (funcall 'neovm--sm-integrate (nth 2 expr) var)))
       ;; integral(c * x^n) or integral(c * x)
       ((eq (car expr) '*)
        (let ((a (nth 1 expr))
              (b (nth 2 expr)))
          (cond
           ;; c * f(x): pull constant out
           ((numberp a)
            (list '* a (funcall 'neovm--sm-integrate b var)))
           ;; f(x) * c: pull constant out
           ((numberp b)
            (list '* b (funcall 'neovm--sm-integrate a var)))
           (t (list 'integral expr var)))))
       ;; integral(x^n) = x^(n+1) / (n+1)
       ((and (eq (car expr) 'expt)
             (eq (nth 1 expr) var)
             (numberp (nth 2 expr)))
        (let ((n (nth 2 expr)))
          (if (= n -1)
              (list 'ln (list 'abs var))
            (list '/ (list 'expt var (+ n 1)) (+ n 1)))))
       (t (list 'integral expr var)))))

  (unwind-protect
      (list
       ;; integral(3, x) = 3*x
       (funcall 'neovm--sm-integrate 3 'x)
       ;; integral(x, x) = x^2/2
       (funcall 'neovm--sm-integrate 'x 'x)
       ;; integral(x^2, x) = x^3/3
       (funcall 'neovm--sm-integrate '(expt x 2) 'x)
       ;; integral(x^-1, x) = ln(|x|)
       (funcall 'neovm--sm-integrate '(expt x -1) 'x)
       ;; integral(3*x^2, x) = 3 * x^3/3
       (funcall 'neovm--sm-integrate '(* 3 (expt x 2)) 'x)
       ;; integral(x^2 + x, x) = x^3/3 + x^2/2
       (funcall 'neovm--sm-integrate '(+ (expt x 2) x) 'x)
       ;; integral(2*x + 1, x) = 2*x^2/2 + 1*x
       (funcall 'neovm--sm-integrate '(+ (* 2 x) 1) 'x)
       ;; integral of constant 0
       (funcall 'neovm--sm-integrate 0 'x))
    (fmakunbound 'neovm--sm-integrate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((* 3 x) (/ (expt x 2) 2) (/ (expt x 3) 3) (ln (abs x)) (* 3 (/ (expt x 3) 3)) (+ (/ (expt x 3) 3) (/ (expt x 2) 2)) (+ (* 2 (/ (expt x 2) 2)) (* 1 x)) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression tree operations: depth, size, variable extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symmath_expression_tree_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute various properties of symbolic expression trees
    let form = r#"(progn
  ;; Compute depth of expression tree
  (fset 'neovm--sm-depth
    (lambda (expr)
      (cond
       ((or (numberp expr) (symbolp expr)) 0)
       (t (1+ (apply #'max
                     (mapcar (lambda (e) (funcall 'neovm--sm-depth e))
                             (cdr expr))))))))

  ;; Count total nodes in expression tree
  (fset 'neovm--sm-size
    (lambda (expr)
      (cond
       ((or (numberp expr) (symbolp expr)) 1)
       (t (1+ (apply #'+ (mapcar (lambda (e) (funcall 'neovm--sm-size e))
                                  (cdr expr))))))))

  ;; Extract all variables (unique, sorted)
  (fset 'neovm--sm-vars
    (lambda (expr)
      (cond
       ((numberp expr) nil)
       ((symbolp expr) (list expr))
       (t (let ((all-vars nil))
            (dolist (sub (cdr expr))
              (dolist (v (funcall 'neovm--sm-vars sub))
                (unless (memq v all-vars)
                  (setq all-vars (cons v all-vars)))))
            (sort all-vars (lambda (a b)
                             (string< (symbol-name a) (symbol-name b)))))))))

  ;; Substitute variable with expression
  (fset 'neovm--sm-subst
    (lambda (expr var replacement)
      (cond
       ((numberp expr) expr)
       ((symbolp expr) (if (eq expr var) replacement expr))
       (t (cons (car expr)
                (mapcar (lambda (e) (funcall 'neovm--sm-subst e var replacement))
                        (cdr expr)))))))

  (unwind-protect
      (let ((e1 '(+ (* 3 x) (* 2 y)))
            (e2 '(+ (expt x 2) (* (- y 1) (+ x z))))
            (e3 42)
            (e4 'x))
        (list
         ;; Depths
         (mapcar (lambda (e) (funcall 'neovm--sm-depth e))
                 (list e1 e2 e3 e4))
         ;; Sizes
         (mapcar (lambda (e) (funcall 'neovm--sm-size e))
                 (list e1 e2 e3 e4))
         ;; Variables
         (funcall 'neovm--sm-vars e1)
         (funcall 'neovm--sm-vars e2)
         (funcall 'neovm--sm-vars e3)
         ;; Substitution: replace x with (+ a 1) in 3*x + 2*y
         (funcall 'neovm--sm-subst e1 'x '(+ a 1))
         ;; Substitution: replace y with 0 in e2
         (funcall 'neovm--sm-subst e2 'y 0)))
    (fmakunbound 'neovm--sm-depth)
    (fmakunbound 'neovm--sm-size)
    (fmakunbound 'neovm--sm-vars)
    (fmakunbound 'neovm--sm-subst)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 3 0 0) (7 11 1 1) (x y) (x y z) nil (+ (* 3 (+ a 1)) (* 2 y)) (+ (expt x 2) (* (- 0 1) (+ x z))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
