//! Oracle parity tests for numeric algorithm patterns.
//!
//! Covers: GCD/LCM via Euclidean algorithm with multiple inputs,
//! modular exponentiation (fast power mod), Sieve of Eratosthenes with
//! prime factorization, base conversion (decimal to any base and back),
//! matrix multiplication using nested lists, Newton's method for cube root,
//! and polynomial evaluation with derivative (Horner's method extended).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// GCD/LCM with fold over multiple numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_gcd_lcm_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GCD and LCM over lists of numbers, using fold (reduce) pattern
    let form = "(progn
  (fset 'neovm--test-gcd2
    (lambda (a b)
      (let ((x (abs a)) (y (abs b)))
        (while (/= y 0)
          (let ((tmp (% x y)))
            (setq x y y tmp)))
        x)))
  (fset 'neovm--test-lcm2
    (lambda (a b)
      (if (or (= a 0) (= b 0)) 0
        (/ (abs (* a b)) (funcall 'neovm--test-gcd2 a b)))))
  (fset 'neovm--test-gcd-list
    (lambda (lst)
      (let ((result (car lst)))
        (dolist (x (cdr lst))
          (setq result (funcall 'neovm--test-gcd2 result x)))
        result)))
  (fset 'neovm--test-lcm-list
    (lambda (lst)
      (let ((result (car lst)))
        (dolist (x (cdr lst))
          (setq result (funcall 'neovm--test-lcm2 result x)))
        result)))
  (unwind-protect
      (list
        ;; Single pair
        (funcall 'neovm--test-gcd2 48 18)
        (funcall 'neovm--test-lcm2 12 8)
        ;; GCD of multiple numbers
        (funcall 'neovm--test-gcd-list '(48 36 24 60))
        ;; LCM of multiple numbers
        (funcall 'neovm--test-lcm-list '(4 6 10))
        ;; GCD of coprime numbers = 1
        (funcall 'neovm--test-gcd-list '(7 11 13 17))
        ;; LCM of numbers with common factors
        (funcall 'neovm--test-lcm-list '(12 18 24))
        ;; Verify identity: gcd(a,b) * lcm(a,b) = a * b
        (let ((a 360) (b 252))
          (= (* (funcall 'neovm--test-gcd2 a b)
                (funcall 'neovm--test-lcm2 a b))
             (* a b))))
    (fmakunbound 'neovm--test-gcd2)
    (fmakunbound 'neovm--test-lcm2)
    (fmakunbound 'neovm--test-gcd-list)
    (fmakunbound 'neovm--test-lcm-list)))";
    let expect = expect_test::expect![[r#""OK (6 24 12 60 1 72 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modular exponentiation (fast power mod)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_modular_exponentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute (base^exp) mod m using binary exponentiation
    // Avoids overflow by taking mod at each step
    let form = "(progn
  (fset 'neovm--test-powmod
    (lambda (base exp mod)
      (let ((result 1)
            (b (% base mod)))
        (while (> exp 0)
          (when (= (% exp 2) 1)
            (setq result (% (* result b) mod)))
          (setq exp (/ exp 2)
                b (% (* b b) mod)))
        result)))
  (unwind-protect
      (list
        ;; 2^10 mod 1000 = 1024 mod 1000 = 24
        (funcall 'neovm--test-powmod 2 10 1000)
        ;; 3^13 mod 100 = 1594323 mod 100 = 23
        (funcall 'neovm--test-powmod 3 13 100)
        ;; Fermat's little theorem: a^(p-1) mod p = 1 for prime p
        (funcall 'neovm--test-powmod 2 12 13)
        (funcall 'neovm--test-powmod 5 6 7)
        ;; Large-ish exponent
        (funcall 'neovm--test-powmod 7 256 1000000007)
        ;; base=0 -> result=0
        (funcall 'neovm--test-powmod 0 100 17)
        ;; exp=0 -> result=1
        (funcall 'neovm--test-powmod 12345 0 97)
        ;; Verify: (a*b) mod m = ((a mod m) * (b mod m)) mod m
        (let ((a 123) (b 456) (m 1000))
          (= (% (* a b) m)
             (% (* (% a m) (% b m)) m))))
    (fmakunbound 'neovm--test-powmod)))";
    let expect = expect_test::expect![[r#""OK (24 23 1 1 699853951 0 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sieve of Eratosthenes with prime factorization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_sieve_and_factorize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build sieve, collect primes, then use them for trial-division factorization
    let form = "(progn
  (fset 'neovm--test-sieve
    (lambda (limit)
      (let ((is-prime (make-vector (1+ limit) t)))
        (aset is-prime 0 nil)
        (aset is-prime 1 nil)
        (let ((i 2))
          (while (<= (* i i) limit)
            (when (aref is-prime i)
              (let ((j (* i i)))
                (while (<= j limit)
                  (aset is-prime j nil)
                  (setq j (+ j i)))))
            (setq i (1+ i))))
        (let ((primes nil))
          (let ((i 2))
            (while (<= i limit)
              (when (aref is-prime i)
                (setq primes (cons i primes)))
              (setq i (1+ i))))
          (nreverse primes)))))
  (fset 'neovm--test-factorize
    (lambda (n primes)
      (let ((factors nil)
            (remaining n))
        (dolist (p primes)
          (when (> (* p p) remaining)
            (when (> remaining 1)
              (setq factors (cons remaining factors)))
            (setq remaining 1))
          (while (and (> remaining 1) (= (% remaining p) 0))
            (setq factors (cons p factors)
                  remaining (/ remaining p))))
        (when (> remaining 1)
          (setq factors (cons remaining factors)))
        (nreverse factors))))
  (unwind-protect
      (let ((primes (funcall 'neovm--test-sieve 100)))
        (list
          ;; Primes up to 30
          (let ((p30 nil))
            (dolist (p primes)
              (when (<= p 30) (setq p30 (cons p p30))))
            (nreverse p30))
          ;; Factorizations
          (funcall 'neovm--test-factorize 60 primes)
          (funcall 'neovm--test-factorize 97 primes)
          (funcall 'neovm--test-factorize 360 primes)
          (funcall 'neovm--test-factorize 1 primes)
          ;; Verify: product of factors = original
          (let ((factors (funcall 'neovm--test-factorize 2520 primes)))
            (let ((product 1))
              (dolist (f factors)
                (setq product (* product f)))
              (list factors (= product 2520))))))
    (fmakunbound 'neovm--test-sieve)
    (fmakunbound 'neovm--test-factorize)))";
    let expect = expect_test::expect![[
        r#""OK ((2 3 5 7 11 13 17 19 23 29) (2 2 3 5) (97) (2 2 2 3 3 5) nil ((2 2 2 3 3 5 7) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Base conversion (decimal to any base, and back)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_base_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert decimal to base-N string and back, supporting bases 2-36
    let form = r#"(progn
  (fset 'neovm--test-digits "0123456789abcdefghijklmnopqrstuvwxyz")
  (fset 'neovm--test-to-base
    (lambda (n base)
      (if (= n 0) "0"
        (let ((result "")
              (num n))
          (while (> num 0)
            (let ((digit (% num base)))
              (setq result (concat (char-to-string
                                     (aref (symbol-value 'neovm--test-digits) digit))
                                   result)
                    num (/ num base))))
          result))))
  (fset 'neovm--test-from-base
    (lambda (s base)
      (let ((result 0)
            (i 0)
            (len (length s)))
        (while (< i len)
          (let* ((ch (aref s i))
                 (digit (cond
                          ((and (>= ch ?0) (<= ch ?9)) (- ch ?0))
                          ((and (>= ch ?a) (<= ch ?z)) (+ 10 (- ch ?a)))
                          (t 0))))
            (setq result (+ (* result base) digit)
                  i (1+ i))))
        result)))
  (unwind-protect
      (list
        ;; Binary
        (funcall 'neovm--test-to-base 42 2)
        (funcall 'neovm--test-from-base "101010" 2)
        ;; Octal
        (funcall 'neovm--test-to-base 255 8)
        (funcall 'neovm--test-from-base "377" 8)
        ;; Hex
        (funcall 'neovm--test-to-base 65535 16)
        (funcall 'neovm--test-from-base "ffff" 16)
        ;; Base 36
        (funcall 'neovm--test-to-base 1000000 36)
        ;; Roundtrip verification for multiple bases
        (let ((results nil))
          (dolist (base '(2 3 7 10 16 36))
            (dolist (n '(0 1 42 255 1000 99999))
              (let* ((s (funcall 'neovm--test-to-base n base))
                     (back (funcall 'neovm--test-from-base s base)))
                (unless (= n back)
                  (setq results (cons (list 'fail n base s back) results))))))
          (if results results 'all-roundtrips-passed)))
    (fmakunbound 'neovm--test-to-base)
    (fmakunbound 'neovm--test-from-base)
    (makunbound 'neovm--test-digits)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--test-digits)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix multiplication using nested lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_matrix_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiply two matrices represented as lists of row-lists
    let form = "(progn
  (fset 'neovm--test-mat-rows (lambda (m) (length m)))
  (fset 'neovm--test-mat-cols (lambda (m) (length (car m))))
  (fset 'neovm--test-mat-ref
    (lambda (m r c) (nth c (nth r m))))
  (fset 'neovm--test-mat-transpose
    (lambda (m)
      (let ((rows (funcall 'neovm--test-mat-rows m))
            (cols (funcall 'neovm--test-mat-cols m))
            (result nil))
        (let ((c 0))
          (while (< c cols)
            (let ((row nil) (r 0))
              (while (< r rows)
                (setq row (cons (funcall 'neovm--test-mat-ref m r c) row))
                (setq r (1+ r)))
              (setq result (cons (nreverse row) result)))
            (setq c (1+ c))))
        (nreverse result))))
  (fset 'neovm--test-dot-product
    (lambda (a b)
      (let ((sum 0))
        (while (and a b)
          (setq sum (+ sum (* (car a) (car b)))
                a (cdr a) b (cdr b)))
        sum)))
  (fset 'neovm--test-mat-mul
    (lambda (a b)
      (let ((bt (funcall 'neovm--test-mat-transpose b))
            (result nil))
        (dolist (row-a a)
          (let ((new-row nil))
            (dolist (col-b bt)
              (setq new-row
                    (cons (funcall 'neovm--test-dot-product row-a col-b)
                          new-row)))
            (setq result (cons (nreverse new-row) result))))
        (nreverse result))))
  (unwind-protect
      (list
        ;; 2x3 * 3x2 = 2x2
        (funcall 'neovm--test-mat-mul
                 '((1 2 3) (4 5 6))
                 '((7 8) (9 10) (11 12)))
        ;; Identity multiplication: A * I = A
        (let ((a '((1 2) (3 4)))
              (i '((1 0) (0 1))))
          (funcall 'neovm--test-mat-mul a i))
        ;; Square: A * A
        (funcall 'neovm--test-mat-mul
                 '((1 1) (1 0))
                 '((1 1) (1 0)))
        ;; 1x3 * 3x1 = 1x1 (dot product as matrix multiply)
        (funcall 'neovm--test-mat-mul
                 '((2 3 4))
                 '((5) (6) (7)))
        ;; Associativity: (A*B)*C = A*(B*C)
        (let ((a '((1 2) (3 4)))
              (b '((5 6) (7 8)))
              (c '((9 10) (11 12))))
          (equal (funcall 'neovm--test-mat-mul
                          (funcall 'neovm--test-mat-mul a b) c)
                 (funcall 'neovm--test-mat-mul
                          a (funcall 'neovm--test-mat-mul b c)))))
    (fmakunbound 'neovm--test-mat-rows)
    (fmakunbound 'neovm--test-mat-cols)
    (fmakunbound 'neovm--test-mat-ref)
    (fmakunbound 'neovm--test-mat-transpose)
    (fmakunbound 'neovm--test-dot-product)
    (fmakunbound 'neovm--test-mat-mul)))";
    let expect = expect_test::expect![[
        r#""OK (((58 64) (139 154)) ((1 2) (3 4)) ((2 1) (1 1)) ((56)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Newton's method for cube root approximation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_newton_cube_root() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Newton's method for cube root: x_{n+1} = (2*x_n + S/x_n^2) / 3
    let form = "(progn
  (fset 'neovm--test-cbrt-newton
    (lambda (s tolerance)
      (let ((guess (/ (float s) 3.0))
            (iters 0))
        (when (= guess 0.0) (setq guess 1.0))
        (while (and (> (abs (- (* guess guess guess) s)) tolerance)
                    (< iters 200))
          (setq guess (/ (+ (* 2.0 guess) (/ (float s) (* guess guess))) 3.0)
                iters (1+ iters)))
        (cons guess iters))))
  (unwind-protect
      (let ((eps 1e-10)
            (results nil))
        (dolist (s '(8.0 27.0 64.0 125.0 2.0 1000.0 0.001))
          (let* ((result (funcall 'neovm--test-cbrt-newton s eps))
                 (approx (car result))
                 (iters (cdr result))
                 ;; Verify: approx^3 should be close to s
                 (check (abs (- (* approx approx approx) s))))
            (setq results
                  (cons (list s (< check 1e-6) (< iters 100))
                        results))))
        (nreverse results))
    (fmakunbound 'neovm--test-cbrt-newton)))";
    let expect = expect_test::expect![[
        r#""OK ((8.0 t t) (27.0 t t) (64.0 t t) (125.0 t t) (2.0 t t) (1000.0 t t) (0.001 t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial evaluation and derivative (extended Horner's method)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_horner_with_derivative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extended Horner's method that simultaneously computes p(x) and p'(x)
    // using the relation: p'(x) = sum of (coeff * degree * x^(degree-1))
    // Efficient single-pass: track both value and derivative
    let form = "(progn
  (fset 'neovm--test-horner-deriv
    (lambda (coeffs x)
      ;; coeffs: highest-degree first (a_n a_{n-1} ... a_0)
      ;; Returns (value . derivative)
      (let ((val 0.0)
            (deriv 0.0))
        (dolist (c coeffs)
          ;; deriv = deriv * x + val (from chain rule of Horner)
          ;; val = val * x + c
          (setq deriv (+ (* deriv (float x)) val)
                val (+ (* val (float x)) (float c))))
        (cons val deriv))))
  (fset 'neovm--test-newton-root
    (lambda (coeffs x0 tolerance max-iters)
      ;; Newton's method for polynomial root: x_{n+1} = x_n - p(x_n)/p'(x_n)
      (let ((x (float x0))
            (iters 0))
        (while (< iters max-iters)
          (let* ((vd (funcall 'neovm--test-horner-deriv coeffs x))
                 (val (car vd))
                 (deriv (cdr vd)))
            (when (< (abs val) tolerance)
              (setq iters max-iters))  ;; converged, exit
            (when (and (>= (abs val) tolerance) (/= deriv 0.0))
              (setq x (- x (/ val deriv))
                    iters (1+ iters)))))
        x)))
  (unwind-protect
      (let ((eps 1e-8))
        (list
          ;; p(x) = x^2 - 4, evaluate at x=3: 9-4=5, p'(3)=6
          (funcall 'neovm--test-horner-deriv '(1 0 -4) 3.0)
          ;; p(x) = 2x^3 - 3x^2 + x - 5, evaluate at x=2: 16-12+2-5=1
          (funcall 'neovm--test-horner-deriv '(2 -3 1 -5) 2.0)
          ;; p(x) = x^2 - 2, find root (should be sqrt(2))
          (let* ((root (funcall 'neovm--test-newton-root '(1 0 -2) 1.5 eps 100))
                 (diff (abs (- root (sqrt 2.0)))))
            (list 'sqrt2 (< diff 1e-6)))
          ;; p(x) = x^3 - 6x^2 + 11x - 6 = (x-1)(x-2)(x-3)
          ;; Find roots starting from different points
          (let ((coeffs '(1 -6 11 -6)))
            (list
              (let ((r (funcall 'neovm--test-newton-root coeffs 0.5 eps 100)))
                (< (abs (- r 1.0)) 1e-4))
              (let ((r (funcall 'neovm--test-newton-root coeffs 1.5 eps 100)))
                (< (abs (- r 2.0)) 1e-4))
              (let ((r (funcall 'neovm--test-newton-root coeffs 3.5 eps 100)))
                (< (abs (- r 3.0)) 1e-4))))))
    (fmakunbound 'neovm--test-horner-deriv)
    (fmakunbound 'neovm--test-newton-root)))";
    let expect = expect_test::expect![[r#""OK ((5.0 . 6.0) (1.0 . 13.0) (sqrt2 t) (t nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Integer partition counting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numpat_integer_partitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count integer partitions using dynamic programming
    // p(n) = number of ways to write n as sum of positive integers
    let form = "(progn
  (fset 'neovm--test-partition-count
    (lambda (n)
      ;; DP approach: dp[i] = number of partitions of i
      ;; For each possible part k from 1 to n, update dp
      (let ((dp (make-vector (1+ n) 0)))
        (aset dp 0 1)
        (let ((k 1))
          (while (<= k n)
            (let ((i k))
              (while (<= i n)
                (aset dp i (+ (aref dp i) (aref dp (- i k))))
                (setq i (1+ i))))
            (setq k (1+ k))))
        (aref dp n))))
  (fset 'neovm--test-list-partitions
    (lambda (n max-part)
      ;; Generate all partitions of n where parts <= max-part, in decreasing order
      (cond
        ((= n 0) '(()))
        ((or (< n 0) (= max-part 0)) nil)
        (t (let ((with-max (mapcar
                             (lambda (p) (cons max-part p))
                             (funcall 'neovm--test-list-partitions
                                      (- n max-part) max-part)))
                 (without-max (funcall 'neovm--test-list-partitions
                                       n (1- max-part))))
             (append with-max without-max))))))
  (unwind-protect
      (list
        ;; Known partition numbers: p(1)=1, p(2)=2, p(3)=3, p(4)=5, p(5)=7
        (mapcar (lambda (n) (funcall 'neovm--test-partition-count n))
                '(0 1 2 3 4 5 6 7 8 10))
        ;; Explicit partitions of 5
        (funcall 'neovm--test-list-partitions 5 5)
        ;; Verify count matches enumeration
        (= (funcall 'neovm--test-partition-count 6)
           (length (funcall 'neovm--test-list-partitions 6 6))))
    (fmakunbound 'neovm--test-partition-count)
    (fmakunbound 'neovm--test-list-partitions)))";
    let expect = expect_test::expect![[
        r#""OK ((1 1 2 3 5 7 11 15 22 42) ((5) (4 1) (3 2) (3 1 1) (2 2 1) (2 1 1 1) (1 1 1 1 1)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
