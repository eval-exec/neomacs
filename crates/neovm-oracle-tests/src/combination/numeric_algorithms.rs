//! Complex oracle parity tests for numeric algorithm implementations.
//!
//! Tests GCD/LCM computation, Newton's method for square root,
//! polynomial evaluation (Horner's method), statistical functions
//! (mean, variance, stddev, median), matrix determinant via recursive
//! cofactor expansion, and bisection method for root finding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// GCD/LCM with extended Euclidean algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_extended_gcd_lcm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extended GCD: returns (gcd, x, y) where a*x + b*y = gcd(a,b)
    // Then use it for LCM and verify Bezout's identity
    let form = "(progn
  (fset 'neovm--test-egcd
    (lambda (a b)
      (if (= b 0)
          (list a 1 0)
        (let* ((sub (funcall 'neovm--test-egcd b (% a b)))
               (g (car sub))
               (x (cadr sub))
               (y (caddr sub)))
          (list g y (- x (* (/ a b) y)))))))
  (unwind-protect
      (let ((results nil))
        ;; Test several pairs and verify Bezout's identity: a*x + b*y = gcd
        (dolist (pair '((48 18) (100 75) (35 15) (252 105) (17 13) (1071 462)))
          (let* ((a (car pair))
                 (b (cadr pair))
                 (egcd (funcall 'neovm--test-egcd a b))
                 (g (car egcd))
                 (x (cadr egcd))
                 (y (caddr egcd))
                 ;; Bezout check: a*x + b*y = g
                 (bezout-ok (= (+ (* a x) (* b y)) g))
                 ;; LCM = |a*b| / gcd(a,b)
                 (lcm-val (/ (* a b) g)))
            (setq results
                  (cons (list (cons a b) g bezout-ok lcm-val)
                        results))))
        (nreverse results))
    (fmakunbound 'neovm--test-egcd)))";
    let expect = expect_test::expect![[
        r#""OK (((48 . 18) 6 t 144) ((100 . 75) 25 t 300) ((35 . 15) 5 t 105) ((252 . 105) 21 t 1260) ((17 . 13) 1 t 221) ((1071 . 462) 21 t 23562))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Newton's method for square root
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_newton_sqrt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Newton's method: x_{n+1} = (x_n + S/x_n) / 2
    // Converges to sqrt(S). Compare with built-in sqrt.
    let form = "(progn
  (fset 'neovm--test-newton-sqrt
    (lambda (s tolerance)
      (let ((guess (/ (float s) 2.0))
            (iters 0))
        (while (and (> (abs (- (* guess guess) s)) tolerance)
                    (< iters 100))
          (setq guess (/ (+ guess (/ (float s) guess)) 2.0)
                iters (1+ iters)))
        (cons guess iters))))
  (unwind-protect
      (let ((eps 1e-12)
            (results nil))
        (dolist (s '(2.0 3.0 4.0 9.0 25.0 144.0 2.0 0.01 10000.0))
          (let* ((newton-result (funcall 'neovm--test-newton-sqrt s eps))
                 (newton-val (car newton-result))
                 (newton-iters (cdr newton-result))
                 (builtin-val (sqrt s))
                 (diff (abs (- newton-val builtin-val))))
            (setq results
                  (cons (list s
                              (< diff 1e-10)
                              (< newton-iters 50))
                        results))))
        (nreverse results))
    (fmakunbound 'neovm--test-newton-sqrt)))";
    let expect = expect_test::expect![[
        r#""OK ((2.0 t t) (3.0 t t) (4.0 t t) (9.0 t t) (25.0 t t) (144.0 t t) (2.0 t t) (0.01 t t) (10000.0 t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial evaluation (Horner's method)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_horner_polynomial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Horner's method: evaluate p(x) = a_n*x^n + ... + a_1*x + a_0
    // Coefficients stored highest-degree first: (a_n a_{n-1} ... a_1 a_0)
    // p(x) = (...((a_n * x + a_{n-1}) * x + a_{n-2}) * x + ...) + a_0
    let form = "(progn
  (fset 'neovm--test-horner
    (lambda (coeffs x)
      (let ((result 0.0))
        (dolist (c coeffs)
          (setq result (+ (* result x) (float c))))
        result)))
  (fset 'neovm--test-poly-naive
    (lambda (coeffs x)
      (let ((result 0.0)
            (degree (1- (length coeffs)))
            (i 0))
        (dolist (c coeffs)
          (setq result (+ result (* (float c) (expt (float x) (- degree i))))
                i (1+ i)))
        result)))
  (unwind-protect
      (let ((eps 1e-8)
            (results nil))
        ;; p(x) = 2x^3 - 6x^2 + 2x - 1
        (let ((coeffs '(2 -6 2 -1)))
          (dolist (x '(-3.0 -1.0 0.0 1.0 2.0 3.0 5.0 10.0))
            (let ((h (funcall 'neovm--test-horner coeffs x))
                  (n (funcall 'neovm--test-poly-naive coeffs x)))
              (setq results
                    (cons (list x (< (abs (- h n)) eps))
                          results)))))
        ;; Also test: p(x) = x^4 + 1 at various points
        (let ((coeffs2 '(1 0 0 0 1)))
          (dolist (x '(-2.0 -1.0 0.0 1.0 2.0))
            (let ((h (funcall 'neovm--test-horner coeffs2 x))
                  (expected (+ (expt x 4.0) 1.0)))
              (setq results
                    (cons (list 'x4+1 x (< (abs (- h expected)) eps))
                          results)))))
        (nreverse results))
    (fmakunbound 'neovm--test-horner)
    (fmakunbound 'neovm--test-poly-naive)))";
    let expect = expect_test::expect![[
        r#""OK ((-3.0 t) (-1.0 t) (0.0 t) (1.0 t) (2.0 t) (3.0 t) (5.0 t) (10.0 t) (x4+1 -2.0 t) (x4+1 -1.0 t) (x4+1 0.0 t) (x4+1 1.0 t) (x4+1 2.0 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Statistical functions: mean, variance, stddev, median
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  ;; Mean
  (fset 'neovm--test-mean
    (lambda (lst)
      (let ((sum 0.0) (n 0))
        (dolist (x lst)
          (setq sum (+ sum (float x))
                n (1+ n)))
        (/ sum n))))
  ;; Variance (population)
  (fset 'neovm--test-variance
    (lambda (lst)
      (let ((m (funcall 'neovm--test-mean lst))
            (sum-sq 0.0)
            (n 0))
        (dolist (x lst)
          (let ((diff (- (float x) m)))
            (setq sum-sq (+ sum-sq (* diff diff))
                  n (1+ n))))
        (/ sum-sq n))))
  ;; Standard deviation
  (fset 'neovm--test-stddev
    (lambda (lst)
      (sqrt (funcall 'neovm--test-variance lst))))
  ;; Median (sorts the list first)
  (fset 'neovm--test-median
    (lambda (lst)
      (let* ((sorted (sort (copy-sequence lst) '<))
             (n (length sorted))
             (mid (/ n 2)))
        (if (= (% n 2) 0)
            ;; even: average of two middle elements
            (/ (+ (float (nth (1- mid) sorted))
                  (float (nth mid sorted)))
               2.0)
          ;; odd: middle element
          (float (nth mid sorted))))))
  (unwind-protect
      (let ((data1 '(4 8 15 16 23 42))
            (data2 '(2 4 4 4 5 5 7 9))
            (data3 '(10 20 30 40 50))
            (eps 1e-10))
        (list
          ;; data1 stats
          (let ((m (funcall 'neovm--test-mean data1)))
            (list (< (abs (- m 18.0)) eps)
                  (funcall 'neovm--test-median data1)))
          ;; data2 stats: known mean=5, variance=4, stddev=2
          (let ((m (funcall 'neovm--test-mean data2))
                (v (funcall 'neovm--test-variance data2))
                (s (funcall 'neovm--test-stddev data2)))
            (list (< (abs (- m 5.0)) eps)
                  (< (abs (- v 4.0)) eps)
                  (< (abs (- s 2.0)) eps)
                  (funcall 'neovm--test-median data2)))
          ;; data3 stats
          (let ((m (funcall 'neovm--test-mean data3))
                (med (funcall 'neovm--test-median data3)))
            (list (< (abs (- m 30.0)) eps)
                  (< (abs (- med 30.0)) eps)))))
    (fmakunbound 'neovm--test-mean)
    (fmakunbound 'neovm--test-variance)
    (fmakunbound 'neovm--test-stddev)
    (fmakunbound 'neovm--test-median)))";
    let expect = expect_test::expect![[r#""OK ((t 15.5) (t t t 4.5) (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix determinant (recursive cofactor expansion)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_matrix_determinant() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Matrix represented as list of lists (rows)
    // det computed by cofactor expansion along first row
    let form = "(progn
  ;; Get element at (row, col) from matrix
  (fset 'neovm--test-mat-ref
    (lambda (mat r c)
      (nth c (nth r mat))))
  ;; Build submatrix by removing row r and col c
  (fset 'neovm--test-minor
    (lambda (mat r c)
      (let ((result nil)
            (ri 0))
        (dolist (row mat)
          (unless (= ri r)
            (let ((new-row nil)
                  (ci 0))
              (dolist (val row)
                (unless (= ci c)
                  (setq new-row (cons val new-row)))
                (setq ci (1+ ci)))
              (setq result (cons (nreverse new-row) result))))
          (setq ri (1+ ri)))
        (nreverse result))))
  ;; Recursive determinant
  (fset 'neovm--test-det
    (lambda (mat)
      (let ((n (length mat)))
        (cond
          ((= n 1) (funcall 'neovm--test-mat-ref mat 0 0))
          ((= n 2)
           (- (* (funcall 'neovm--test-mat-ref mat 0 0)
                 (funcall 'neovm--test-mat-ref mat 1 1))
              (* (funcall 'neovm--test-mat-ref mat 0 1)
                 (funcall 'neovm--test-mat-ref mat 1 0))))
          (t
           (let ((det 0)
                 (sign 1)
                 (j 0))
             (dolist (val (car mat))
               (setq det (+ det (* sign val
                                    (funcall 'neovm--test-det
                                             (funcall 'neovm--test-minor mat 0 j)))))
               (setq sign (- sign)
                     j (1+ j)))
             det))))))
  (unwind-protect
      (list
        ;; 2x2: det([[1,2],[3,4]]) = 1*4 - 2*3 = -2
        (funcall 'neovm--test-det '((1 2) (3 4)))
        ;; 3x3: det([[6,1,1],[4,-2,5],[2,8,7]]) = -306
        (funcall 'neovm--test-det '((6 1 1) (4 -2 5) (2 8 7)))
        ;; Identity 3x3: det = 1
        (funcall 'neovm--test-det '((1 0 0) (0 1 0) (0 0 1)))
        ;; Singular matrix: det = 0
        (funcall 'neovm--test-det '((1 2 3) (4 5 6) (7 8 9)))
        ;; 4x4 matrix
        (funcall 'neovm--test-det
                 '((1 0 2 -1) (3 0 0 5) (2 1 4 -3) (1 0 5 0)))
        ;; Diagonal matrix: det = product of diagonal
        (funcall 'neovm--test-det '((2 0 0) (0 3 0) (0 0 5))))
    (fmakunbound 'neovm--test-mat-ref)
    (fmakunbound 'neovm--test-minor)
    (fmakunbound 'neovm--test-det)))";
    let expect = expect_test::expect![[r#""OK (-2 -306 1 0 30 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bisection method for root finding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_bisection_root_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bisection method: find root of f(x) = 0 in interval [a, b]
    // where f(a) and f(b) have opposite signs
    let form = "(progn
  (fset 'neovm--test-bisect
    (lambda (f a b tolerance max-iters)
      (let ((fa (funcall f a))
            (iters 0)
            (mid 0.0))
        (while (and (> (- b a) tolerance)
                    (< iters max-iters))
          (setq mid (/ (+ a b) 2.0))
          (let ((fm (funcall f mid)))
            (if (< (* fa fm) 0)
                (setq b mid)
              (setq a mid
                    fa fm)))
          (setq iters (1+ iters)))
        (cons (/ (+ a b) 2.0) iters))))
  (unwind-protect
      (let ((eps 1e-10))
        (list
          ;; Find sqrt(2) as root of f(x) = x^2 - 2
          (let* ((f (lambda (x) (- (* x x) 2.0)))
                 (result (funcall 'neovm--test-bisect f 1.0 2.0 eps 100))
                 (root (car result)))
            (list (< (abs (- root (sqrt 2.0))) 1e-8)
                  (< (cdr result) 100)))
          ;; Find root of f(x) = cos(x) - x (near 0.739)
          (let* ((f (lambda (x) (- (cos x) x)))
                 (result (funcall 'neovm--test-bisect f 0.0 1.0 eps 100))
                 (root (car result)))
            ;; Verify f(root) ~ 0
            (list (< (abs (- (cos root) root)) 1e-8)
                  (< (cdr result) 100)))
          ;; Find root of f(x) = e^x - 3 (= ln(3) ~ 1.0986)
          (let* ((f (lambda (x) (- (exp x) 3.0)))
                 (result (funcall 'neovm--test-bisect f 0.0 2.0 eps 100))
                 (root (car result)))
            (list (< (abs (- root (log 3.0))) 1e-8)
                  (< (cdr result) 100)))
          ;; Find root of x^3 - x - 2 (has root near 1.5214)
          (let* ((f (lambda (x) (- (+ (expt x 3.0) (- x)) 2.0)))
                 (result (funcall 'neovm--test-bisect f 1.0 2.0 eps 100))
                 (root (car result))
                 (fval (- (+ (expt root 3.0) (- root)) 2.0)))
            (list (< (abs fval) 1e-8)
                  (< (cdr result) 100)))))
    (fmakunbound 'neovm--test-bisect)))";
    let expect = expect_test::expect![[r#""OK ((t t) (t t) (t t) (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fibonacci numbers via matrix exponentiation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_fibonacci_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // [[1,1],[1,0]]^n = [[F(n+1),F(n)],[F(n),F(n-1)]]
    // Use repeated squaring for O(log n) computation
    let form = "(progn
  ;; 2x2 matrix multiply
  (fset 'neovm--test-mat2-mul
    (lambda (a b)
      (list (list (+ (* (nth 0 (nth 0 a)) (nth 0 (nth 0 b)))
                     (* (nth 1 (nth 0 a)) (nth 0 (nth 1 b))))
                  (+ (* (nth 0 (nth 0 a)) (nth 1 (nth 0 b)))
                     (* (nth 1 (nth 0 a)) (nth 1 (nth 1 b)))))
            (list (+ (* (nth 0 (nth 1 a)) (nth 0 (nth 0 b)))
                     (* (nth 1 (nth 1 a)) (nth 0 (nth 1 b))))
                  (+ (* (nth 0 (nth 1 a)) (nth 1 (nth 0 b)))
                     (* (nth 1 (nth 1 a)) (nth 1 (nth 1 b))))))))
  ;; Matrix power by repeated squaring
  (fset 'neovm--test-mat2-pow
    (lambda (m n)
      (cond
        ((= n 0) '((1 0) (0 1)))
        ((= n 1) m)
        ((= (% n 2) 0)
         (let ((half (funcall 'neovm--test-mat2-pow m (/ n 2))))
           (funcall 'neovm--test-mat2-mul half half)))
        (t
         (funcall 'neovm--test-mat2-mul m
                  (funcall 'neovm--test-mat2-pow m (1- n)))))))
  ;; Fibonacci via matrix
  (fset 'neovm--test-fib-mat
    (lambda (n)
      (if (= n 0) 0
        (let ((result (funcall 'neovm--test-mat2-pow
                               '((1 1) (1 0)) n)))
          (nth 1 (nth 0 result))))))
  ;; Simple recursive fib for verification
  (fset 'neovm--test-fib-simple
    (lambda (n)
      (let ((a 0) (b 1))
        (dotimes (_ n)
          (let ((tmp b))
            (setq b (+ a b)
                  a tmp)))
        a)))
  (unwind-protect
      (let ((results nil))
        (dolist (n '(0 1 2 3 5 8 10 15 20 25))
          (let ((mat-fib (funcall 'neovm--test-fib-mat n))
                (simple-fib (funcall 'neovm--test-fib-simple n)))
            (setq results
                  (cons (list n mat-fib (= mat-fib simple-fib))
                        results))))
        (nreverse results))
    (fmakunbound 'neovm--test-mat2-mul)
    (fmakunbound 'neovm--test-mat2-pow)
    (fmakunbound 'neovm--test-fib-mat)
    (fmakunbound 'neovm--test-fib-simple)))";
    let expect = expect_test::expect![[
        r#""OK ((0 0 t) (1 1 t) (2 1 t) (3 2 t) (5 5 t) (8 21 t) (10 55 t) (15 610 t) (20 6765 t) (25 75025 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Numerical derivative and gradient descent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numalgo_gradient_descent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Numerically differentiate, then do gradient descent to find minimum
    // of f(x) = (x - 3)^2 + 1, minimum at x=3
    let form = "(progn
  ;; Numerical derivative: f'(x) ~ (f(x+h) - f(x-h)) / (2h)
  (fset 'neovm--test-deriv
    (lambda (f x)
      (let ((h 1e-7))
        (/ (- (funcall f (+ x h))
              (funcall f (- x h)))
           (* 2.0 h)))))
  ;; Gradient descent
  (fset 'neovm--test-gd
    (lambda (f x0 lr max-iters tolerance)
      (let ((x (float x0))
            (iters 0))
        (while (and (< iters max-iters)
                    (> (abs (funcall 'neovm--test-deriv f x)) tolerance))
          (setq x (- x (* lr (funcall 'neovm--test-deriv f x)))
                iters (1+ iters)))
        (cons x iters))))
  (unwind-protect
      (let ((eps 1e-4))
        (list
          ;; f(x) = (x-3)^2 + 1, minimum at x=3
          (let* ((f (lambda (x) (+ (expt (- x 3.0) 2) 1.0)))
                 (result (funcall 'neovm--test-gd f 0.0 0.1 1000 1e-8))
                 (x-min (car result)))
            (list (< (abs (- x-min 3.0)) eps)
                  (< (cdr result) 1000)))
          ;; f(x) = x^4 - 4x^2, has local min near x = sqrt(2) ~ 1.414
          ;; Start from x=2 (should converge to right local min)
          (let* ((f (lambda (x) (- (expt x 4.0) (* 4.0 (expt x 2.0)))))
                 (result (funcall 'neovm--test-gd f 2.0 0.01 5000 1e-8))
                 (x-min (car result)))
            (list (< (abs (- (abs x-min) (sqrt 2.0))) eps)
                  (< (cdr result) 5000)))))
    (fmakunbound 'neovm--test-deriv)
    (fmakunbound 'neovm--test-gd)))";
    let expect = expect_test::expect![[r#""OK ((t t) (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
