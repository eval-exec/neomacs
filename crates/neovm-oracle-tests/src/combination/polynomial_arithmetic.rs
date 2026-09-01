//! Oracle parity tests for polynomial arithmetic in Elisp:
//! polynomial representation as sorted list of (coefficient . exponent),
//! addition, multiplication, evaluation at a point, differentiation,
//! and polynomial GCD via the Euclidean algorithm.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Polynomial representation and normalization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polynomial_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Polynomials are represented as sorted (descending exponent) lists
    // of (coefficient . exponent) pairs. Zero terms are removed.
    let form = r#"(progn
  ;; Normalize: sort by descending exponent, merge same-exponent terms,
  ;; remove zero coefficients
  (fset 'neovm--poly-normalize
    (lambda (poly)
      ;; First sort by descending exponent
      (let ((sorted (sort (copy-sequence poly)
                          (lambda (a b) (> (cdr a) (cdr b))))))
        ;; Merge same-exponent terms
        (let ((result nil)
              (remaining sorted))
          (while remaining
            (let ((term (car remaining))
                  (coeff (caar remaining))
                  (exp (cdar remaining)))
              (setq remaining (cdr remaining))
              ;; Accumulate coefficients for same exponent
              (while (and remaining (= (cdar remaining) exp))
                (setq coeff (+ coeff (caar remaining)))
                (setq remaining (cdr remaining)))
              ;; Only keep non-zero coefficients
              (unless (= coeff 0)
                (push (cons coeff exp) result))))
          (nreverse result)))))

  ;; Pretty-print a polynomial as a string
  (fset 'neovm--poly-to-string
    (lambda (poly)
      (if (null poly) "0"
        (mapconcat
         (lambda (term)
           (let ((c (car term)) (e (cdr term)))
             (cond
              ((= e 0) (number-to-string c))
              ((= e 1) (if (= c 1) "x"
                         (if (= c -1) "-x"
                           (concat (number-to-string c) "x"))))
              (t (if (= c 1) (concat "x^" (number-to-string e))
                   (if (= c -1) (concat "-x^" (number-to-string e))
                     (concat (number-to-string c) "x^" (number-to-string e))))))))
         poly " + "))))

  (unwind-protect
      (list
       ;; Normalize: combine like terms
       (funcall 'neovm--poly-normalize '((3 . 2) (2 . 1) (5 . 2) (1 . 0)))
       ;; Normalize: remove zero coefficients
       (funcall 'neovm--poly-normalize '((3 . 2) (-3 . 2) (1 . 0)))
       ;; Normalize: already normalized
       (funcall 'neovm--poly-normalize '((5 . 3) (3 . 2) (1 . 0)))
       ;; Normalize: empty polynomial
       (funcall 'neovm--poly-normalize nil)
       ;; Normalize: single term
       (funcall 'neovm--poly-normalize '((7 . 4)))
       ;; Normalize: all zeros
       (funcall 'neovm--poly-normalize '((3 . 1) (-3 . 1)))
       ;; Pretty print
       (funcall 'neovm--poly-to-string '((3 . 2) (2 . 1) (1 . 0)))
       (funcall 'neovm--poly-to-string '((1 . 3) (-1 . 1)))
       (funcall 'neovm--poly-to-string nil))
    (fmakunbound 'neovm--poly-normalize)
    (fmakunbound 'neovm--poly-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK (((8 . 2) (2 . 1) (1 . 0)) ((1 . 0)) ((5 . 3) (3 . 2) (1 . 0)) nil ((7 . 4)) nil \"3x^2 + 2x + 1\" \"x^3 + -x\" \"0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial addition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polynomial_addition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Add two polynomials by concatenating and normalizing.
    let form = r#"(progn
  (fset 'neovm--poly-normalize
    (lambda (poly)
      (let ((sorted (sort (copy-sequence poly)
                          (lambda (a b) (> (cdr a) (cdr b))))))
        (let ((result nil) (remaining sorted))
          (while remaining
            (let ((coeff (caar remaining))
                  (exp (cdar remaining)))
              (setq remaining (cdr remaining))
              (while (and remaining (= (cdar remaining) exp))
                (setq coeff (+ coeff (caar remaining)))
                (setq remaining (cdr remaining)))
              (unless (= coeff 0)
                (push (cons coeff exp) result))))
          (nreverse result)))))

  ;; Add two polynomials
  (fset 'neovm--poly-add
    (lambda (p1 p2)
      (funcall 'neovm--poly-normalize (append p1 p2))))

  ;; Negate a polynomial
  (fset 'neovm--poly-negate
    (lambda (p)
      (mapcar (lambda (term) (cons (- (car term)) (cdr term))) p)))

  ;; Subtract
  (fset 'neovm--poly-sub
    (lambda (p1 p2)
      (funcall 'neovm--poly-add p1 (funcall 'neovm--poly-negate p2))))

  (unwind-protect
      (list
       ;; (3x^2 + 2x + 1) + (x^2 + 3x + 4) = 4x^2 + 5x + 5
       (funcall 'neovm--poly-add
                '((3 . 2) (2 . 1) (1 . 0))
                '((1 . 2) (3 . 1) (4 . 0)))
       ;; p + 0 = p
       (funcall 'neovm--poly-add '((5 . 3) (1 . 0)) nil)
       ;; 0 + p = p
       (funcall 'neovm--poly-add nil '((2 . 1) (7 . 0)))
       ;; p + (-p) = 0 (empty list)
       (funcall 'neovm--poly-add '((3 . 2) (1 . 0)) '((-3 . 2) (-1 . 0)))
       ;; Different degree polynomials
       (funcall 'neovm--poly-add '((1 . 5)) '((1 . 2) (1 . 0)))
       ;; Subtraction: (5x^2 + 3x) - (2x^2 + x) = 3x^2 + 2x
       (funcall 'neovm--poly-sub
                '((5 . 2) (3 . 1))
                '((2 . 2) (1 . 1)))
       ;; Negate
       (funcall 'neovm--poly-negate '((3 . 2) (-1 . 1) (5 . 0)))
       ;; Add many terms at same degree
       (funcall 'neovm--poly-add
                '((1 . 3) (2 . 3) (3 . 3))
                '((-5 . 3) (1 . 0))))
    (fmakunbound 'neovm--poly-normalize)
    (fmakunbound 'neovm--poly-add)
    (fmakunbound 'neovm--poly-negate)
    (fmakunbound 'neovm--poly-sub)))"#;
    let expect = expect_test::expect![[
        r#""OK (((4 . 2) (5 . 1) (5 . 0)) ((5 . 3) (1 . 0)) ((2 . 1) (7 . 0)) nil ((1 . 5) (1 . 2) (1 . 0)) ((3 . 2) (2 . 1)) ((-3 . 2) (1 . 1) (-5 . 0)) ((1 . 3) (1 . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polynomial_multiplication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiply polynomials by distributing every term of one over the other.
    let form = r#"(progn
  (fset 'neovm--poly-normalize
    (lambda (poly)
      (let ((sorted (sort (copy-sequence poly)
                          (lambda (a b) (> (cdr a) (cdr b))))))
        (let ((result nil) (remaining sorted))
          (while remaining
            (let ((coeff (caar remaining))
                  (exp (cdar remaining)))
              (setq remaining (cdr remaining))
              (while (and remaining (= (cdar remaining) exp))
                (setq coeff (+ coeff (caar remaining)))
                (setq remaining (cdr remaining)))
              (unless (= coeff 0)
                (push (cons coeff exp) result))))
          (nreverse result)))))

  ;; Multiply two polynomials
  (fset 'neovm--poly-mul
    (lambda (p1 p2)
      (let ((terms nil))
        (dolist (t1 p1)
          (dolist (t2 p2)
            (push (cons (* (car t1) (car t2))
                        (+ (cdr t1) (cdr t2)))
                  terms)))
        (funcall 'neovm--poly-normalize terms))))

  ;; Scalar multiply
  (fset 'neovm--poly-scale
    (lambda (p scalar)
      (if (= scalar 0) nil
        (mapcar (lambda (term) (cons (* (car term) scalar) (cdr term))) p))))

  (unwind-protect
      (list
       ;; (x + 1)(x - 1) = x^2 - 1
       (funcall 'neovm--poly-mul
                '((1 . 1) (1 . 0))
                '((1 . 1) (-1 . 0)))
       ;; (x + 1)^2 = x^2 + 2x + 1
       (funcall 'neovm--poly-mul
                '((1 . 1) (1 . 0))
                '((1 . 1) (1 . 0)))
       ;; (2x)(3x^2) = 6x^3
       (funcall 'neovm--poly-mul '((2 . 1)) '((3 . 2)))
       ;; p * 0 = 0
       (funcall 'neovm--poly-mul '((5 . 3) (1 . 0)) nil)
       ;; p * 1 = p
       (funcall 'neovm--poly-mul '((3 . 2) (1 . 0)) '((1 . 0)))
       ;; (x^2 + x + 1)(x - 1) = x^3 - 1
       (funcall 'neovm--poly-mul
                '((1 . 2) (1 . 1) (1 . 0))
                '((1 . 1) (-1 . 0)))
       ;; Scalar multiply
       (funcall 'neovm--poly-scale '((3 . 2) (1 . 1) (5 . 0)) 4)
       ;; (2x + 3)(4x^2 + x + 2)
       (funcall 'neovm--poly-mul
                '((2 . 1) (3 . 0))
                '((4 . 2) (1 . 1) (2 . 0))))
    (fmakunbound 'neovm--poly-normalize)
    (fmakunbound 'neovm--poly-mul)
    (fmakunbound 'neovm--poly-scale)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 2) (-1 . 0)) ((1 . 2) (2 . 1) (1 . 0)) ((6 . 3)) nil ((3 . 2) (1 . 0)) ((1 . 3) (-1 . 0)) ((12 . 2) (4 . 1) (20 . 0)) ((8 . 3) (14 . 2) (7 . 1) (6 . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial evaluation at a point
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polynomial_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate a polynomial at a given x using Horner's method or direct sum.
    let form = r#"(progn
  ;; Direct evaluation: sum of c * x^e for each term
  (fset 'neovm--poly-eval-direct
    (lambda (poly x)
      (let ((sum 0))
        (dolist (term poly)
          (let ((c (car term)) (e (cdr term)))
            ;; Compute x^e by repeated multiplication
            (let ((power 1) (i 0))
              (while (< i e)
                (setq power (* power x))
                (setq i (1+ i)))
              (setq sum (+ sum (* c power))))))
        sum)))

  ;; Horner's method: convert to dense form first, then evaluate
  (fset 'neovm--poly-degree
    (lambda (poly)
      (if (null poly) 0
        (apply #'max (mapcar #'cdr poly)))))

  (fset 'neovm--poly-coeff-at
    (lambda (poly exp)
      (let ((found 0))
        (dolist (term poly)
          (when (= (cdr term) exp)
            (setq found (+ found (car term)))))
        found)))

  (fset 'neovm--poly-eval-horner
    (lambda (poly x)
      (if (null poly) 0
        (let* ((deg (funcall 'neovm--poly-degree poly))
               (result (funcall 'neovm--poly-coeff-at poly deg))
               (i (1- deg)))
          (while (>= i 0)
            (setq result (+ (* result x)
                            (funcall 'neovm--poly-coeff-at poly i)))
            (setq i (1- i)))
          result))))

  (unwind-protect
      (let ((p1 '((3 . 2) (2 . 1) (1 . 0)))   ;; 3x^2 + 2x + 1
            (p2 '((1 . 3) (-2 . 1) (5 . 0)))    ;; x^3 - 2x + 5
            (p3 '((1 . 0)))                       ;; constant 1
            (p4 nil))                              ;; zero polynomial
        (list
         ;; Evaluate p1 at x=0,1,2,3,-1
         (funcall 'neovm--poly-eval-direct p1 0)
         (funcall 'neovm--poly-eval-direct p1 1)
         (funcall 'neovm--poly-eval-direct p1 2)
         (funcall 'neovm--poly-eval-direct p1 3)
         (funcall 'neovm--poly-eval-direct p1 -1)
         ;; Horner's should give same results
         (funcall 'neovm--poly-eval-horner p1 0)
         (funcall 'neovm--poly-eval-horner p1 1)
         (funcall 'neovm--poly-eval-horner p1 2)
         ;; Evaluate p2
         (funcall 'neovm--poly-eval-direct p2 0)
         (funcall 'neovm--poly-eval-direct p2 1)
         (funcall 'neovm--poly-eval-direct p2 -1)
         (funcall 'neovm--poly-eval-horner p2 2)
         ;; Constant and zero
         (funcall 'neovm--poly-eval-direct p3 42)
         (funcall 'neovm--poly-eval-direct p4 42)
         (funcall 'neovm--poly-eval-horner p3 100)
         (funcall 'neovm--poly-eval-horner p4 100)
         ;; Degree
         (funcall 'neovm--poly-degree p1)
         (funcall 'neovm--poly-degree p2)
         (funcall 'neovm--poly-degree p3)
         (funcall 'neovm--poly-degree p4)))
    (fmakunbound 'neovm--poly-eval-direct)
    (fmakunbound 'neovm--poly-degree)
    (fmakunbound 'neovm--poly-coeff-at)
    (fmakunbound 'neovm--poly-eval-horner)))"#;
    let expect = expect_test::expect![[r#""OK (1 6 17 34 2 1 6 17 5 4 6 9 1 0 1 0 2 3 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: polynomial differentiation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polynomial_differentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Differentiate polynomials: d/dx(c*x^e) = c*e*x^(e-1).
    // Test single and higher-order derivatives.
    let form = r#"(progn
  (fset 'neovm--poly-normalize
    (lambda (poly)
      (let ((sorted (sort (copy-sequence poly)
                          (lambda (a b) (> (cdr a) (cdr b))))))
        (let ((result nil) (remaining sorted))
          (while remaining
            (let ((coeff (caar remaining))
                  (exp (cdar remaining)))
              (setq remaining (cdr remaining))
              (while (and remaining (= (cdar remaining) exp))
                (setq coeff (+ coeff (caar remaining)))
                (setq remaining (cdr remaining)))
              (unless (= coeff 0)
                (push (cons coeff exp) result))))
          (nreverse result)))))

  ;; Differentiate once
  (fset 'neovm--poly-deriv
    (lambda (poly)
      (let ((result nil))
        (dolist (term poly)
          (let ((c (car term)) (e (cdr term)))
            (when (> e 0)
              (push (cons (* c e) (1- e)) result))))
        (funcall 'neovm--poly-normalize (nreverse result)))))

  ;; Nth derivative
  (fset 'neovm--poly-nth-deriv
    (lambda (poly n)
      (let ((p poly) (i 0))
        (while (< i n)
          (setq p (funcall 'neovm--poly-deriv p))
          (setq i (1+ i)))
        p)))

  ;; Antiderivative (indefinite integral, C=0)
  (fset 'neovm--poly-integrate
    (lambda (poly)
      (let ((result nil))
        (dolist (term poly)
          (let ((c (car term)) (e (cdr term)))
            ;; c*x^e -> (c/(e+1)) * x^(e+1)
            ;; Use integer division only if exact, else keep as-is
            ;; For testing, keep coefficients as integers when possible
            (push (cons c (1+ e)) result)))
        (funcall 'neovm--poly-normalize (nreverse result)))))

  (unwind-protect
      (list
       ;; d/dx(3x^2 + 2x + 1) = 6x + 2
       (funcall 'neovm--poly-deriv '((3 . 2) (2 . 1) (1 . 0)))
       ;; d/dx(x^5) = 5x^4
       (funcall 'neovm--poly-deriv '((1 . 5)))
       ;; d/dx(7) = 0 (empty)
       (funcall 'neovm--poly-deriv '((7 . 0)))
       ;; d/dx(0) = 0
       (funcall 'neovm--poly-deriv nil)
       ;; d/dx(x) = 1
       (funcall 'neovm--poly-deriv '((1 . 1)))
       ;; Second derivative of x^3 + x^2 + x + 1 = 6x + 2
       (funcall 'neovm--poly-nth-deriv '((1 . 3) (1 . 2) (1 . 1) (1 . 0)) 2)
       ;; Third derivative of x^4 = 24x
       (funcall 'neovm--poly-nth-deriv '((1 . 4)) 3)
       ;; Fourth derivative of x^3 = 0
       (funcall 'neovm--poly-nth-deriv '((1 . 3)) 4)
       ;; d/dx(5x^4 - 3x^3 + 2x^2 - x + 7) = 20x^3 - 9x^2 + 4x - 1
       (funcall 'neovm--poly-deriv '((5 . 4) (-3 . 3) (2 . 2) (-1 . 1) (7 . 0)))
       ;; Integration: integral of 6x + 2 = 6x^2 + 2x (coeff not divided)
       (funcall 'neovm--poly-integrate '((6 . 1) (2 . 0)))
       ;; Integration of constant 5 = 5x
       (funcall 'neovm--poly-integrate '((5 . 0)))
       ;; Integration of zero
       (funcall 'neovm--poly-integrate nil))
    (fmakunbound 'neovm--poly-normalize)
    (fmakunbound 'neovm--poly-deriv)
    (fmakunbound 'neovm--poly-nth-deriv)
    (fmakunbound 'neovm--poly-integrate)))"#;
    let expect = expect_test::expect![[
        r#""OK (((6 . 1) (2 . 0)) ((5 . 4)) nil nil ((1 . 0)) ((6 . 1) (2 . 0)) ((24 . 1)) nil ((20 . 3) (-9 . 2) (4 . 1) (-1 . 0)) ((6 . 2) (2 . 1)) ((5 . 1)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: polynomial GCD via Euclidean algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polynomial_gcd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Polynomial GCD using the Euclidean algorithm with pseudo-remainder.
    // Also test: divisibility check, factoring out common factors.
    let form = r#"(progn
  (fset 'neovm--poly-normalize
    (lambda (poly)
      (let ((sorted (sort (copy-sequence poly)
                          (lambda (a b) (> (cdr a) (cdr b))))))
        (let ((result nil) (remaining sorted))
          (while remaining
            (let ((coeff (caar remaining))
                  (exp (cdar remaining)))
              (setq remaining (cdr remaining))
              (while (and remaining (= (cdar remaining) exp))
                (setq coeff (+ coeff (caar remaining)))
                (setq remaining (cdr remaining)))
              (unless (= coeff 0)
                (push (cons coeff exp) result))))
          (nreverse result)))))

  (fset 'neovm--poly-add
    (lambda (p1 p2)
      (funcall 'neovm--poly-normalize (append p1 p2))))

  (fset 'neovm--poly-negate
    (lambda (p)
      (mapcar (lambda (term) (cons (- (car term)) (cdr term))) p)))

  (fset 'neovm--poly-sub
    (lambda (p1 p2)
      (funcall 'neovm--poly-add p1 (funcall 'neovm--poly-negate p2))))

  (fset 'neovm--poly-mul
    (lambda (p1 p2)
      (let ((terms nil))
        (dolist (t1 p1)
          (dolist (t2 p2)
            (push (cons (* (car t1) (car t2))
                        (+ (cdr t1) (cdr t2)))
                  terms)))
        (funcall 'neovm--poly-normalize terms))))

  (fset 'neovm--poly-degree
    (lambda (poly)
      (if (null poly) -1
        (apply #'max (mapcar #'cdr poly)))))

  (fset 'neovm--poly-leading-coeff
    (lambda (poly)
      (if (null poly) 0 (caar poly))))

  ;; Scale all coefficients
  (fset 'neovm--poly-scale
    (lambda (p scalar)
      (if (= scalar 0) nil
        (funcall 'neovm--poly-normalize
                 (mapcar (lambda (term) (cons (* (car term) scalar) (cdr term))) p)))))

  ;; Polynomial pseudo-remainder: use pseudo-division to avoid fractions
  ;; pseudo_rem(a, b) where we multiply a by lc(b)^(deg(a)-deg(b)+1) then
  ;; perform standard long division to get remainder.
  ;; Simplified: iterative subtraction of shifted b.
  (fset 'neovm--poly-prem
    (lambda (a b)
      (if (null b) a
        (let ((r a)
              (db (funcall 'neovm--poly-degree b))
              (lcb (funcall 'neovm--poly-leading-coeff b)))
          (while (and r (>= (funcall 'neovm--poly-degree r) db))
            (let* ((dr (funcall 'neovm--poly-degree r))
                   (lcr (funcall 'neovm--poly-leading-coeff r))
                   (shift (- dr db))
                   ;; multiply b by lcr * x^shift, subtract from r*lcb
                   (b-shifted (mapcar (lambda (term)
                                        (cons (* (car term) lcr) (+ (cdr term) shift)))
                                      b))
                   (r-scaled (funcall 'neovm--poly-scale r lcb)))
              (setq r (funcall 'neovm--poly-sub r-scaled
                               (funcall 'neovm--poly-normalize b-shifted)))))
          r))))

  ;; GCD via Euclidean algorithm with pseudo-remainder
  (fset 'neovm--poly-gcd
    (lambda (a b)
      (let ((p a) (q b) (steps 0))
        (while (and q (< steps 20))
          (let ((rem (funcall 'neovm--poly-prem p q)))
            (setq p q)
            (setq q rem)
            (setq steps (1+ steps))))
        ;; Make monic-ish: divide by GCD of all coefficients
        (if (null p) nil
          (let ((g (abs (funcall 'neovm--poly-leading-coeff p))))
            (dolist (term p)
              (setq g (funcall 'neovm--pgcd-int g (abs (car term)))))
            (if (= g 0) p
              (funcall 'neovm--poly-normalize
                       (mapcar (lambda (term) (cons (/ (car term) g) (cdr term))) p))))))))

  ;; Integer GCD helper
  (fset 'neovm--pgcd-int
    (lambda (a b)
      (let ((x (abs a)) (y (abs b)))
        (while (/= y 0)
          (let ((tmp (% x y)))
            (setq x y)
            (setq y tmp)))
        x)))

  (unwind-protect
      (list
       ;; GCD of (x^2 - 1) and (x - 1) = (x - 1) up to constant multiple
       ;; x^2 - 1 = (x+1)(x-1), so gcd with (x-1) is (x-1)
       (funcall 'neovm--poly-gcd
                '((1 . 2) (-1 . 0))     ;; x^2 - 1
                '((1 . 1) (-1 . 0)))    ;; x - 1

       ;; GCD of (x^2 + 2x + 1) and (x + 1) = (x + 1)
       ;; x^2 + 2x + 1 = (x+1)^2
       (funcall 'neovm--poly-gcd
                '((1 . 2) (2 . 1) (1 . 0))
                '((1 . 1) (1 . 0)))

       ;; GCD of coprime polynomials: (x^2 + 1) and (x + 1)
       ;; These share no common factor (over integers)
       (funcall 'neovm--poly-gcd
                '((1 . 2) (1 . 0))
                '((1 . 1) (1 . 0)))

       ;; GCD of p and 0 = p
       (funcall 'neovm--poly-gcd '((3 . 2) (1 . 0)) nil)

       ;; GCD of 0 and p = p
       (funcall 'neovm--poly-gcd nil '((2 . 1) (4 . 0)))

       ;; Verify multiplication: (x+1)*(x-1) = x^2-1
       (funcall 'neovm--poly-mul
                '((1 . 1) (1 . 0))
                '((1 . 1) (-1 . 0)))

       ;; Pseudo-remainder: (x^2 + 2x + 1) prem (x + 1) = 0
       (funcall 'neovm--poly-prem
                '((1 . 2) (2 . 1) (1 . 0))
                '((1 . 1) (1 . 0)))

       ;; Integer GCD helper
       (funcall 'neovm--pgcd-int 12 8)
       (funcall 'neovm--pgcd-int 17 13)
       (funcall 'neovm--pgcd-int 0 5))
    (fmakunbound 'neovm--poly-normalize)
    (fmakunbound 'neovm--poly-add)
    (fmakunbound 'neovm--poly-negate)
    (fmakunbound 'neovm--poly-sub)
    (fmakunbound 'neovm--poly-mul)
    (fmakunbound 'neovm--poly-degree)
    (fmakunbound 'neovm--poly-leading-coeff)
    (fmakunbound 'neovm--poly-scale)
    (fmakunbound 'neovm--poly-prem)
    (fmakunbound 'neovm--poly-gcd)
    (fmakunbound 'neovm--pgcd-int)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 1) (-1 . 0)) ((1 . 1) (1 . 0)) ((1 . 0)) ((3 . 2) (1 . 0)) ((1 . 1) (2 . 0)) ((1 . 2) (-1 . 0)) nil 4 1 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
