//! Oracle parity tests for a computer algebra system in Elisp:
//! polynomial representation and arithmetic (add, multiply, divide),
//! polynomial GCD, symbolic differentiation with simplification,
//! symbolic integration for polynomials, rational expression simplification,
//! matrix determinant (Bareiss algorithm), characteristic polynomial.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Polynomial representation and arithmetic (add, multiply, divide)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cas_polynomial_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Polynomials as vectors of coefficients, index = degree
  ;; e.g., [1 2 3] = 1 + 2x + 3x^2

  ;; Trim trailing zeros
  (fset 'neovm--cas-trim
    (lambda (p)
      (let ((v (copy-sequence p)))
        (while (and (> (length v) 1) (= (aref v (1- (length v))) 0))
          (setq v (seq-take v (1- (length v)))))
        v)))

  ;; Degree
  (fset 'neovm--cas-degree
    (lambda (p)
      (let ((trimmed (funcall 'neovm--cas-trim p)))
        (if (and (= (length trimmed) 1) (= (aref trimmed 0) 0))
            -1
          (1- (length trimmed))))))

  ;; Add two polynomials
  (fset 'neovm--cas-add
    (lambda (a b)
      (let* ((la (length a)) (lb (length b))
             (len (max la lb))
             (result (make-vector len 0)))
        (dotimes (i la)
          (aset result i (+ (aref result i) (aref a i))))
        (dotimes (i lb)
          (aset result i (+ (aref result i) (aref b i))))
        (funcall 'neovm--cas-trim result))))

  ;; Subtract
  (fset 'neovm--cas-sub
    (lambda (a b)
      (let* ((la (length a)) (lb (length b))
             (len (max la lb))
             (result (make-vector len 0)))
        (dotimes (i la)
          (aset result i (aref a i)))
        (dotimes (i lb)
          (aset result i (- (aref result i) (aref b i))))
        (funcall 'neovm--cas-trim result))))

  ;; Multiply
  (fset 'neovm--cas-mul
    (lambda (a b)
      (let* ((la (length a)) (lb (length b))
             (len (+ la lb -1))
             (result (make-vector len 0)))
        (dotimes (i la)
          (dotimes (j lb)
            (aset result (+ i j)
                  (+ (aref result (+ i j))
                     (* (aref a i) (aref b j))))))
        (funcall 'neovm--cas-trim result))))

  ;; Polynomial long division: returns (quotient . remainder)
  (fset 'neovm--cas-divmod
    (lambda (a b)
      (let* ((da (funcall 'neovm--cas-degree a))
             (db (funcall 'neovm--cas-degree b)))
        (if (< da db)
            (cons [0] (funcall 'neovm--cas-trim a))
          (let ((rem (copy-sequence a))
                (quot (make-vector (+ (- da db) 1) 0))
                (lc-b (aref b db)))
            (let ((i da))
              (while (>= i db)
                (let ((coeff (/ (aref rem i) lc-b)))
                  (aset quot (- i db) coeff)
                  (dotimes (j (1+ db))
                    (aset rem (+ (- i db) j)
                          (- (aref rem (+ (- i db) j))
                             (* coeff (aref b j))))))
                (setq i (1- i))))
            (cons (funcall 'neovm--cas-trim quot)
                  (funcall 'neovm--cas-trim rem)))))))

  (unwind-protect
      (list
       ;; (1 + 2x + 3x^2) + (4 + 5x) = 5 + 7x + 3x^2
       (funcall 'neovm--cas-add [1 2 3] [4 5])
       ;; (1 + x) * (1 - x) = 1 - x^2
       (funcall 'neovm--cas-mul [1 1] [1 -1])
       ;; (1 + x)^2 = 1 + 2x + x^2
       (funcall 'neovm--cas-mul [1 1] [1 1])
       ;; (x^2 + 2x + 1) / (x + 1) = (x + 1) remainder 0
       (funcall 'neovm--cas-divmod [1 2 1] [1 1])
       ;; (x^3 - 1) / (x - 1) = x^2 + x + 1 remainder 0
       (funcall 'neovm--cas-divmod [-1 0 0 1] [-1 1])
       ;; (2x^3 + 3x^2 + x + 5) / (x + 1)
       (funcall 'neovm--cas-divmod [5 1 3 2] [1 1])
       ;; Degree tests
       (funcall 'neovm--cas-degree [1 2 3])
       (funcall 'neovm--cas-degree [5])
       (funcall 'neovm--cas-degree [0])
       ;; Subtraction
       (funcall 'neovm--cas-sub [1 2 3] [1 2 3]))
    (fmakunbound 'neovm--cas-trim)
    (fmakunbound 'neovm--cas-degree)
    (fmakunbound 'neovm--cas-add)
    (fmakunbound 'neovm--cas-sub)
    (fmakunbound 'neovm--cas-mul)
    (fmakunbound 'neovm--cas-divmod)))"#;
    let expect = expect_test::expect![[
        r#""OK ([5 7 3] [1 0 -1] [1 2 1] ([1 1] . [0]) ([1 1 1] . [0]) ([0 1 2] . [5]) 2 0 -1 [0])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial GCD via Euclidean algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cas_polynomial_gcd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--cas-trim
    (lambda (p)
      (let ((v (copy-sequence p)))
        (while (and (> (length v) 1) (= (aref v (1- (length v))) 0))
          (setq v (seq-take v (1- (length v)))))
        v)))

  (fset 'neovm--cas-degree
    (lambda (p)
      (let ((trimmed (funcall 'neovm--cas-trim p)))
        (if (and (= (length trimmed) 1) (= (aref trimmed 0) 0))
            -1
          (1- (length trimmed))))))

  (fset 'neovm--cas-divmod
    (lambda (a b)
      (let* ((da (funcall 'neovm--cas-degree a))
             (db (funcall 'neovm--cas-degree b)))
        (if (< da db)
            (cons [0] (funcall 'neovm--cas-trim a))
          (let ((rem (copy-sequence a))
                (quot (make-vector (+ (- da db) 1) 0))
                (lc-b (aref b db)))
            (let ((i da))
              (while (>= i db)
                (let ((coeff (/ (aref rem i) lc-b)))
                  (aset quot (- i db) coeff)
                  (dotimes (j (1+ db))
                    (aset rem (+ (- i db) j)
                          (- (aref rem (+ (- i db) j))
                             (* coeff (aref b j))))))
                (setq i (1- i))))
            (cons (funcall 'neovm--cas-trim quot)
                  (funcall 'neovm--cas-trim rem)))))))

  ;; Is polynomial zero?
  (fset 'neovm--cas-zero-p
    (lambda (p)
      (let ((trimmed (funcall 'neovm--cas-trim p)))
        (and (= (length trimmed) 1) (= (aref trimmed 0) 0)))))

  ;; GCD of two integers
  (fset 'neovm--cas-igcd
    (lambda (a b)
      (let ((x (abs a)) (y (abs b)))
        (while (/= y 0)
          (let ((tmp (% x y))) (setq x y) (setq y tmp)))
        x)))

  ;; Make polynomial primitive (divide by GCD of coefficients)
  (fset 'neovm--cas-primitive
    (lambda (p)
      (let ((trimmed (funcall 'neovm--cas-trim p)))
        (if (funcall 'neovm--cas-zero-p trimmed) [0]
          (let ((g (abs (aref trimmed 0))))
            (dotimes (i (length trimmed))
              (setq g (funcall 'neovm--cas-igcd g (abs (aref trimmed i)))))
            (if (= g 0) trimmed
              (let ((result (make-vector (length trimmed) 0)))
                (dotimes (i (length trimmed))
                  (aset result i (/ (aref trimmed i) g)))
                ;; Make leading coefficient positive
                (if (< (aref result (1- (length result))) 0)
                    (progn (dotimes (i (length result))
                             (aset result i (- (aref result i))))
                           result)
                  result))))))))

  ;; Polynomial GCD
  (fset 'neovm--cas-gcd
    (lambda (a b)
      (let ((p (funcall 'neovm--cas-trim a))
            (q (funcall 'neovm--cas-trim b))
            (steps 0))
        (while (and (not (funcall 'neovm--cas-zero-p q)) (< steps 30))
          (let* ((divmod (funcall 'neovm--cas-divmod p q))
                 (rem (cdr divmod)))
            (setq p q)
            (setq q rem)
            (setq steps (1+ steps))))
        (funcall 'neovm--cas-primitive p))))

  (unwind-protect
      (list
       ;; GCD of (x^2-1) and (x-1) = (x-1) = [-1 1]
       (funcall 'neovm--cas-gcd [-1 0 1] [-1 1])
       ;; GCD of (x^2+2x+1) and (x+1) = (x+1) = [1 1]
       (funcall 'neovm--cas-gcd [1 2 1] [1 1])
       ;; GCD of coprime: (x^2+1) and (x+1) = 1
       (funcall 'neovm--cas-gcd [1 0 1] [1 1])
       ;; GCD with zero polynomial
       (funcall 'neovm--cas-gcd [3 6 9] [0])
       ;; Primitive part
       (funcall 'neovm--cas-primitive [6 12 18])
       ;; Integer GCD
       (funcall 'neovm--cas-igcd 48 18)
       (funcall 'neovm--cas-igcd 0 5)
       ;; Zero check
       (funcall 'neovm--cas-zero-p [0])
       (funcall 'neovm--cas-zero-p [0 0 0])
       (funcall 'neovm--cas-zero-p [1]))
    (fmakunbound 'neovm--cas-trim)
    (fmakunbound 'neovm--cas-degree)
    (fmakunbound 'neovm--cas-divmod)
    (fmakunbound 'neovm--cas-zero-p)
    (fmakunbound 'neovm--cas-igcd)
    (fmakunbound 'neovm--cas-primitive)
    (fmakunbound 'neovm--cas-gcd)))"#;
    let expect = expect_test::expect![[r#""OK ([-1 1] [1 1] [1] [1 2 3] [1 2 3] 6 5 t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbolic differentiation with simplification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cas_symbolic_differentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Differentiate symbolic expressions and simplify
  (fset 'neovm--cas-diff
    (lambda (expr var)
      (cond
       ((numberp expr) 0)
       ((symbolp expr) (if (eq expr var) 1 0))
       ((eq (car expr) '+)
        (funcall 'neovm--cas-simp
                 (list '+ (funcall 'neovm--cas-diff (cadr expr) var)
                       (funcall 'neovm--cas-diff (caddr expr) var))))
       ((eq (car expr) '*)
        (funcall 'neovm--cas-simp
                 (list '+ (list '* (funcall 'neovm--cas-diff (cadr expr) var) (caddr expr))
                       (list '* (cadr expr) (funcall 'neovm--cas-diff (caddr expr) var)))))
       ((eq (car expr) '-)
        (if (= (length expr) 2)
            (funcall 'neovm--cas-simp
                     (list '- (funcall 'neovm--cas-diff (cadr expr) var)))
          (funcall 'neovm--cas-simp
                   (list '- (funcall 'neovm--cas-diff (cadr expr) var)
                         (funcall 'neovm--cas-diff (caddr expr) var)))))
       ((eq (car expr) 'expt)
        (let ((base (cadr expr)) (power (caddr expr)))
          (when (numberp power)
            (funcall 'neovm--cas-simp
                     (list '* (list '* power (list 'expt base (1- power)))
                           (funcall 'neovm--cas-diff base var))))))
       (t (list 'diff expr var)))))

  ;; Simplifier
  (fset 'neovm--cas-simp
    (lambda (expr)
      (if (atom expr) expr
        (let* ((op (car expr))
               (args (mapcar 'neovm--cas-simp (cdr expr)))
               (a (nth 0 args)) (b (nth 1 args)))
          (cond
           ((and (eq op '+) (numberp a) (numberp b)) (+ a b))
           ((and (eq op '+) (equal a 0)) b)
           ((and (eq op '+) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 1) (numberp a)) (- a))
           ((and (eq op '-) (= (length args) 2) (numberp a) (numberp b)) (- a b))
           ((and (eq op '-) (= (length args) 2) (equal b 0)) a)
           ((and (eq op '-) (= (length args) 2) (equal a b)) 0)
           ((and (eq op '*) (numberp a) (numberp b)) (* a b))
           ((and (eq op '*) (equal a 0)) 0)
           ((and (eq op '*) (equal b 0)) 0)
           ((and (eq op '*) (equal a 1)) b)
           ((and (eq op '*) (equal b 1)) a)
           ((and (eq op 'expt) (equal b 0)) 1)
           ((and (eq op 'expt) (equal b 1)) a)
           (t (cons op args)))))))

  ;; Simplify to fixed point
  (fset 'neovm--cas-simp-fix
    (lambda (expr)
      (let ((prev nil) (cur expr) (n 0))
        (while (and (not (equal prev cur)) (< n 20))
          (setq prev cur)
          (setq cur (funcall 'neovm--cas-simp cur))
          (setq n (1+ n)))
        cur)))

  (unwind-protect
      (list
       ;; d/dx(x^3) = 3x^2
       (funcall 'neovm--cas-simp-fix (funcall 'neovm--cas-diff '(expt x 3) 'x))
       ;; d/dx(2*x) = 2
       (funcall 'neovm--cas-simp-fix (funcall 'neovm--cas-diff '(* 2 x) 'x))
       ;; d/dx(x^2 + x + 1)
       (funcall 'neovm--cas-simp-fix
                (funcall 'neovm--cas-diff '(+ (+ (expt x 2) x) 1) 'x))
       ;; d/dx(x*x) using product rule = 2x
       (funcall 'neovm--cas-simp-fix (funcall 'neovm--cas-diff '(* x x) 'x))
       ;; d/dx(5) = 0
       (funcall 'neovm--cas-diff 5 'x)
       ;; d/dx(y) = 0 (y is not the differentiation variable)
       (funcall 'neovm--cas-diff 'y 'x)
       ;; Second derivative of x^4 = 12x^2
       (funcall 'neovm--cas-simp-fix
                (funcall 'neovm--cas-diff
                         (funcall 'neovm--cas-diff '(expt x 4) 'x) 'x))
       ;; d/dx(3*x^2 - 2*x + 5)
       (funcall 'neovm--cas-simp-fix
                (funcall 'neovm--cas-diff
                         '(+ (- (* 3 (expt x 2)) (* 2 x)) 5) 'x)))
    (fmakunbound 'neovm--cas-diff)
    (fmakunbound 'neovm--cas-simp)
    (fmakunbound 'neovm--cas-simp-fix)))"#;
    let expect = expect_test::expect![[
        r#""OK ((* 3 (expt x 2)) 2 (+ (* 2 x) 1) (+ x x) 0 0 (* 4 (* 3 (expt x 2))) (- (* 3 (* 2 x)) 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbolic integration for polynomials
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cas_symbolic_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Integrate polynomial terms symbolically
  ;; integral(c*x^n) = c/(n+1) * x^(n+1) + C
  ;; We keep results as rational (numerator . denominator) pairs for exactness

  ;; Integrate a polynomial given as coefficient vector
  ;; Returns list of (coeff_num coeff_den degree) triples
  (fset 'neovm--cas-integrate-poly
    (lambda (coeffs)
      (let ((result nil))
        (dotimes (i (length coeffs))
          (let ((c (aref coeffs i)))
            (unless (= c 0)
              (push (list c (1+ i) (1+ i)) result))))
        (nreverse result))))

  ;; Definite integral: evaluate antiderivative at bounds
  ;; poly is coefficient vector, returns exact integer result
  ;; when all divisions are exact
  (fset 'neovm--cas-definite-integral
    (lambda (coeffs a b)
      (let ((sum 0))
        (dotimes (i (length coeffs))
          (let ((c (aref coeffs i))
                (n (1+ i)))
            ;; c * (b^n - a^n) / n
            (let ((b-pow 1) (a-pow 1))
              (dotimes (_ n)
                (setq b-pow (* b-pow b))
                (setq a-pow (* a-pow a)))
              ;; Accumulate c*(b^n - a^n) and track denominator separately
              (setq sum (+ sum (/ (* c (- b-pow a-pow)) n))))))
        sum)))

  ;; Evaluate polynomial at a point (Horner's)
  (fset 'neovm--cas-eval-poly
    (lambda (coeffs x)
      (let ((result 0)
            (i (1- (length coeffs))))
        (while (>= i 0)
          (setq result (+ (* result x) (aref coeffs i)))
          (setq i (1- i)))
        result)))

  (unwind-protect
      (list
       ;; Integrate [2 3 4] = 2 + 3x + 4x^2
       ;; -> 2x + 3/2 x^2 + 4/3 x^3
       (funcall 'neovm--cas-integrate-poly [2 3 4])
       ;; Integrate constant [5]
       (funcall 'neovm--cas-integrate-poly [5])
       ;; Integrate [0 0 1] = x^2 -> x^3/3
       (funcall 'neovm--cas-integrate-poly [0 0 1])
       ;; Definite integral of x^2 from 0 to 3 = 27/3 = 9
       (funcall 'neovm--cas-definite-integral [0 0 1] 0 3)
       ;; Definite integral of 2x from 0 to 5 = 25
       (funcall 'neovm--cas-definite-integral [0 2] 0 5)
       ;; Definite integral of 1 from 2 to 7 = 5
       (funcall 'neovm--cas-definite-integral [1] 2 7)
       ;; Definite integral of x^2 + x from 0 to 4
       ;; = 64/3 + 8 = 29 (integer division)
       (funcall 'neovm--cas-definite-integral [0 1 1] 0 4)
       ;; Polynomial evaluation tests
       (funcall 'neovm--cas-eval-poly [1 2 3] 2)
       (funcall 'neovm--cas-eval-poly [1 2 3] 0)
       (funcall 'neovm--cas-eval-poly [1 2 3] -1))
    (fmakunbound 'neovm--cas-integrate-poly)
    (fmakunbound 'neovm--cas-definite-integral)
    (fmakunbound 'neovm--cas-eval-poly)))"#;
    let expect = expect_test::expect![[
        r#""OK (((2 1 1) (3 2 2) (4 3 3)) ((5 1 1)) ((1 3 3)) 9 25 5 29 17 1 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rational expression simplification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cas_rational_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Rational numbers as (numerator . denominator) pairs
  ;; Always keep in lowest terms with positive denominator

  (fset 'neovm--cas-igcd
    (lambda (a b)
      (let ((x (abs a)) (y (abs b)))
        (while (/= y 0)
          (let ((tmp (% x y))) (setq x y) (setq y tmp)))
        x)))

  (fset 'neovm--cas-rat-normalize
    (lambda (n d)
      (if (= d 0) (error "Division by zero")
        (let* ((g (funcall 'neovm--cas-igcd (abs n) (abs d)))
               (nn (/ n g))
               (dd (/ d g)))
          ;; Ensure positive denominator
          (if (< dd 0)
              (cons (- nn) (- dd))
            (cons nn dd))))))

  ;; Arithmetic on rationals
  (fset 'neovm--cas-rat-add
    (lambda (r1 r2)
      (funcall 'neovm--cas-rat-normalize
               (+ (* (car r1) (cdr r2)) (* (car r2) (cdr r1)))
               (* (cdr r1) (cdr r2)))))

  (fset 'neovm--cas-rat-sub
    (lambda (r1 r2)
      (funcall 'neovm--cas-rat-normalize
               (- (* (car r1) (cdr r2)) (* (car r2) (cdr r1)))
               (* (cdr r1) (cdr r2)))))

  (fset 'neovm--cas-rat-mul
    (lambda (r1 r2)
      (funcall 'neovm--cas-rat-normalize
               (* (car r1) (car r2))
               (* (cdr r1) (cdr r2)))))

  (fset 'neovm--cas-rat-div
    (lambda (r1 r2)
      (funcall 'neovm--cas-rat-normalize
               (* (car r1) (cdr r2))
               (* (cdr r1) (car r2)))))

  ;; Continued fraction representation
  (fset 'neovm--cas-to-cf
    (lambda (n d)
      (let ((result nil) (a n) (b d))
        (while (/= b 0)
          (push (/ a b) result)
          (let ((tmp (% a b)))
            (setq a b)
            (setq b tmp)))
        (nreverse result))))

  ;; Reconstruct rational from continued fraction
  (fset 'neovm--cas-from-cf
    (lambda (cf)
      (let ((n 1) (d 0))
        (dolist (a (reverse cf))
          (let ((tmp n))
            (setq n (+ (* a n) d))
            (setq d tmp)))
        (cons n d))))

  (unwind-protect
      (list
       ;; Normalize: 6/4 = 3/2
       (funcall 'neovm--cas-rat-normalize 6 4)
       ;; Normalize: -6/-4 = 3/2
       (funcall 'neovm--cas-rat-normalize -6 -4)
       ;; Normalize: -6/4 = -3/2
       (funcall 'neovm--cas-rat-normalize -6 4)
       ;; Add: 1/2 + 1/3 = 5/6
       (funcall 'neovm--cas-rat-add '(1 . 2) '(1 . 3))
       ;; Sub: 3/4 - 1/4 = 1/2
       (funcall 'neovm--cas-rat-sub '(3 . 4) '(1 . 4))
       ;; Mul: 2/3 * 3/4 = 1/2
       (funcall 'neovm--cas-rat-mul '(2 . 3) '(3 . 4))
       ;; Div: (2/3) / (4/5) = 10/12 = 5/6
       (funcall 'neovm--cas-rat-div '(2 . 3) '(4 . 5))
       ;; Chain: 1/2 + 1/3 + 1/6 = 1
       (funcall 'neovm--cas-rat-add
                (funcall 'neovm--cas-rat-add '(1 . 2) '(1 . 3))
                '(1 . 6))
       ;; Continued fraction of 355/113 (pi approximation)
       (funcall 'neovm--cas-to-cf 355 113)
       ;; Round-trip: cf -> rational -> cf
       (funcall 'neovm--cas-from-cf '(3 7 15 1))
       ;; Zero numerator
       (funcall 'neovm--cas-rat-normalize 0 5))
    (fmakunbound 'neovm--cas-igcd)
    (fmakunbound 'neovm--cas-rat-normalize)
    (fmakunbound 'neovm--cas-rat-add)
    (fmakunbound 'neovm--cas-rat-sub)
    (fmakunbound 'neovm--cas-rat-mul)
    (fmakunbound 'neovm--cas-rat-div)
    (fmakunbound 'neovm--cas-to-cf)
    (fmakunbound 'neovm--cas-from-cf)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 . 2) (3 . 2) (-3 . 2) (5 . 6) (1 . 2) (1 . 2) (5 . 6) (1 . 1) (3 7 16) (355 . 113) (0 . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix determinant via Bareiss algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cas_matrix_determinant_bareiss() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Matrix as vector of row vectors
  ;; mat[i][j] = (aref (aref mat i) j)

  (fset 'neovm--cas-mat-get
    (lambda (mat i j) (aref (aref mat i) j)))

  (fset 'neovm--cas-mat-set
    (lambda (mat i j val) (aset (aref mat i) j val)))

  ;; Deep copy a matrix
  (fset 'neovm--cas-mat-copy
    (lambda (mat)
      (let* ((n (length mat))
             (result (make-vector n nil)))
        (dotimes (i n)
          (aset result i (copy-sequence (aref mat i))))
        result)))

  ;; Bareiss algorithm: fraction-free Gaussian elimination for determinant
  ;; Returns the determinant as an integer
  (fset 'neovm--cas-det-bareiss
    (lambda (mat)
      (let* ((m (funcall 'neovm--cas-mat-copy mat))
             (n (length m))
             (sign 1))
        (dotimes (k (1- n))
          ;; Pivot: find non-zero element in column k from row k onward
          (let ((pivot-row nil) (i k))
            (while (and (< i n) (not pivot-row))
              (unless (= (funcall 'neovm--cas-mat-get m i k) 0)
                (setq pivot-row i))
              (setq i (1+ i)))
            (if (not pivot-row)
                (progn (setq sign 0) (setq k (1- n)))  ;; singular
              ;; Swap rows if needed
              (when (/= pivot-row k)
                (let ((tmp (aref m k)))
                  (aset m k (aref m pivot-row))
                  (aset m pivot-row tmp))
                (setq sign (- sign)))
              ;; Eliminate
              (let ((pivot (funcall 'neovm--cas-mat-get m k k))
                    (prev (if (> k 0) (funcall 'neovm--cas-mat-get m (1- k) (1- k)) 1)))
                (let ((i (1+ k)))
                  (while (< i n)
                    (let ((j (1+ k)))
                      (while (< j n)
                        (funcall 'neovm--cas-mat-set m i j
                                 (/ (- (* pivot (funcall 'neovm--cas-mat-get m i j))
                                       (* (funcall 'neovm--cas-mat-get m i k)
                                          (funcall 'neovm--cas-mat-get m k j)))
                                    prev))
                        (setq j (1+ j))))
                    (setq i (1+ i))))))))
        (if (= sign 0) 0
          (* sign (funcall 'neovm--cas-mat-get m (1- n) (1- n)))))))

  (unwind-protect
      (list
       ;; 1x1 matrix
       (funcall 'neovm--cas-det-bareiss [[5]])
       ;; 2x2 matrix: |1 2; 3 4| = -2
       (funcall 'neovm--cas-det-bareiss [[1 2] [3 4]])
       ;; 3x3 identity: det = 1
       (funcall 'neovm--cas-det-bareiss [[1 0 0] [0 1 0] [0 0 1]])
       ;; 3x3: |1 2 3; 4 5 6; 7 8 9| = 0 (singular)
       (funcall 'neovm--cas-det-bareiss [[1 2 3] [4 5 6] [7 8 9]])
       ;; 3x3: |2 1 1; 1 3 2; 1 0 0| = -1
       (funcall 'neovm--cas-det-bareiss [[2 1 1] [1 3 2] [1 0 0]])
       ;; 4x4 matrix
       (funcall 'neovm--cas-det-bareiss
                [[1 2 3 4] [5 6 7 8] [2 6 4 8] [3 1 1 2]])
       ;; Diagonal matrix: det = product of diagonal
       (funcall 'neovm--cas-det-bareiss [[2 0 0] [0 3 0] [0 0 5]])
       ;; Upper triangular
       (funcall 'neovm--cas-det-bareiss [[2 1 3] [0 4 5] [0 0 6]]))
    (fmakunbound 'neovm--cas-mat-get)
    (fmakunbound 'neovm--cas-mat-set)
    (fmakunbound 'neovm--cas-mat-copy)
    (fmakunbound 'neovm--cas-det-bareiss)))"#;
    let expect = expect_test::expect![[r#""OK (5 -2 1 0 -1 72 30 48)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Characteristic polynomial of a matrix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cas_characteristic_polynomial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the characteristic polynomial via the Faddeev-LeVerrier algorithm:
    // c_n = 1, c_{n-k} = -1/k * sum_{i=1}^{k} c_{n-k+i} * tr(A^i)
    let form = r#"(progn
  ;; Matrix multiply
  (fset 'neovm--cas-mat-mul
    (lambda (a b)
      (let* ((n (length a))
             (result (make-vector n nil)))
        (dotimes (i n)
          (aset result i (make-vector n 0)))
        (dotimes (i n)
          (dotimes (j n)
            (let ((sum 0))
              (dotimes (k n)
                (setq sum (+ sum (* (aref (aref a i) k)
                                    (aref (aref b k) j)))))
              (aset (aref result i) j sum))))
        result)))

  ;; Trace of a matrix
  (fset 'neovm--cas-mat-trace
    (lambda (m)
      (let ((sum 0) (n (length m)))
        (dotimes (i n)
          (setq sum (+ sum (aref (aref m i) i))))
        sum)))

  ;; Matrix power
  (fset 'neovm--cas-mat-pow
    (lambda (m k)
      (if (= k 0)
          ;; Identity matrix
          (let* ((n (length m))
                 (result (make-vector n nil)))
            (dotimes (i n)
              (aset result i (make-vector n 0))
              (aset (aref result i) i 1))
            result)
        (if (= k 1) m
          (funcall 'neovm--cas-mat-mul m
                   (funcall 'neovm--cas-mat-pow m (1- k)))))))

  ;; Newton's identities for characteristic polynomial coefficients
  ;; p_k = tr(A^k), then:
  ;; c_n = 1
  ;; c_{n-1} = -p_1
  ;; c_{n-2} = -(p_2 + c_{n-1}*p_1)/2
  ;; c_{n-k} = -(1/k) * sum_{i=1}^{k} p_i * c_{n-k+i}
  (fset 'neovm--cas-char-poly
    (lambda (mat)
      (let* ((n (length mat))
             (coeffs (make-vector (1+ n) 0))
             (traces (make-vector (1+ n) 0)))
        ;; coeffs[n] = 1 (leading coefficient)
        (aset coeffs n 1)
        ;; Compute traces
        (dotimes (k n)
          (aset traces (1+ k)
                (funcall 'neovm--cas-mat-trace
                         (funcall 'neovm--cas-mat-pow mat (1+ k)))))
        ;; Compute coefficients
        (dotimes (k n)
          (let ((sum 0))
            (dotimes (i (1+ k))
              (setq sum (+ sum (* (aref traces (1+ i))
                                  (aref coeffs (- n k (- -1 i) 1))))))
            (aset coeffs (- n k 1) (/ (- sum) (1+ k)))))
        coeffs)))

  (unwind-protect
      (list
       ;; Trace of 2x2
       (funcall 'neovm--cas-mat-trace [[3 1] [0 2]])
       ;; Matrix multiply 2x2
       (funcall 'neovm--cas-mat-mul [[1 2] [3 4]] [[5 6] [7 8]])
       ;; Char poly of [[2 1] [1 2]] = x^2 - 4x + 3 = [3 -4 1]
       (funcall 'neovm--cas-char-poly [[2 1] [1 2]])
       ;; Char poly of identity 2x2 = x^2 - 2x + 1 = [1 -2 1]
       (funcall 'neovm--cas-char-poly [[1 0] [0 1]])
       ;; Char poly of [[0 1] [-1 0]] (rotation) = x^2 + 1 = [1 0 1]
       (funcall 'neovm--cas-char-poly [[0 1] [-1 0]])
       ;; Char poly of 1x1 [[5]] = x - 5 = [-5 1]
       (funcall 'neovm--cas-char-poly [[5]])
       ;; Char poly of 3x3 diagonal
       ;; [[1 0 0] [0 2 0] [0 0 3]] = (x-1)(x-2)(x-3) = x^3 - 6x^2 + 11x - 6
       (funcall 'neovm--cas-char-poly [[1 0 0] [0 2 0] [0 0 3]])
       ;; Matrix power: A^0 = I, A^2
       (funcall 'neovm--cas-mat-pow [[1 1] [0 1]] 0)
       (funcall 'neovm--cas-mat-pow [[1 1] [0 1]] 3))
    (fmakunbound 'neovm--cas-mat-mul)
    (fmakunbound 'neovm--cas-mat-trace)
    (fmakunbound 'neovm--cas-mat-pow)
    (fmakunbound 'neovm--cas-char-poly)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 [[19 22] [43 50]] [3 -4 1] [1 -2 1] [1 0 1] [-5 1] [-6 11 -6 1] [[1 0] [0 1]] [[1 3] [0 1]])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
