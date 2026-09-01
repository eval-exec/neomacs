//! Oracle parity tests for random number operations and number theory:
//! `random` with no arg and integer arg, `%` (modulo), `mod`, `ash`
//! (arithmetic shift), `lsh` (logical shift), `logand`, `logior`,
//! `logxor`, `lognot`, `logcount`, bit manipulation patterns, testing
//! with large integers, GCD via Euclidean algorithm.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// random with integer argument: result range, type, and distribution sanity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_random_integer_range_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // random with a positive integer N must return an integer in [0, N).
    // We cannot compare exact values (random!), so we verify invariants
    // deterministically: type checks, range checks, and that random(1)
    // always returns 0.
    let form = r#"(list
  ;; random(1) always returns 0
  (= (random 1) 0)
  ;; random(N) returns an integer
  (integerp (random 10))
  (integerp (random 1000000))
  ;; range check: random(N) is >= 0 and < N for several N
  (let ((ok t))
    (dotimes (_ 50)
      (let ((r (random 100)))
        (unless (and (>= r 0) (< r 100))
          (setq ok nil))))
    ok)
  ;; random(2) only returns 0 or 1
  (let ((ok t))
    (dotimes (_ 100)
      (let ((r (random 2)))
        (unless (memq r '(0 1))
          (setq ok nil))))
    ok)
  ;; random with large bound
  (let ((ok t))
    (dotimes (_ 30)
      (let ((r (random 1000000000)))
        (unless (and (>= r 0) (< r 1000000000))
          (setq ok nil))))
    ok))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// % (remainder) vs mod: sign behavior differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remainder_vs_mod_sign_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // % follows the sign of the dividend; mod follows the sign of the divisor
    let form = r#"(list
  ;; Both positive: identical
  (% 10 3)                ;; 1
  (mod 10 3)              ;; 1
  ;; Negative dividend
  (% -10 3)               ;; -1  (sign follows dividend)
  (mod -10 3)             ;; 2   (sign follows divisor)
  ;; Negative divisor
  (% 10 -3)               ;; 1   (sign follows dividend)
  (mod 10 -3)             ;; -2  (sign follows divisor)
  ;; Both negative
  (% -10 -3)              ;; -1
  (mod -10 -3)            ;; -1
  ;; Zero dividend
  (% 0 7)                 ;; 0
  (mod 0 7)               ;; 0
  ;; Exact division
  (% 12 4)                ;; 0
  (mod 12 4)              ;; 0
  ;; Relationship: a = (/ a b)*b + (% a b) for truncation
  (let ((a 17) (b 5))
    (= a (+ (* (/ a b) b) (% a b))))
  ;; Relationship: mod satisfies: 0 <= mod(a,b) < |b| when b > 0
  (let ((ok t))
    (dolist (a '(-20 -10 -5 -1 0 1 5 10 20))
      (dolist (b '(3 7 11))
        (let ((m (mod a b)))
          (unless (and (>= m 0) (< m b))
            (setq ok nil)))))
    ok)
  ;; Large values
  (% 1000000007 1000000)
  (mod -1000000007 1000000))"#;
    let expect = expect_test::expect![[r#""OK (1 1 -1 2 1 -2 -1 -1 0 0 0 0 t t 7 999993)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ash: arithmetic shift with large values and boundary cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ash_large_values_and_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Left shift builds large values
  (ash 1 0)
  (ash 1 1)
  (ash 1 10)
  (ash 1 20)
  (ash 1 30)
  ;; Right shift of negative preserves sign
  (ash -1 0)
  (ash -1 -1)
  (ash -256 -4)     ;; -16
  (ash -1024 -10)   ;; -1
  ;; Shift by 0 is identity
  (ash 42 0)
  (ash -42 0)
  ;; Left then right (lossy for positive)
  (ash (ash 255 8) -8)
  ;; Large shift right goes to 0 or -1
  (ash 999999 -50)
  (ash -999999 -50)
  ;; Equivalence: ash left by n == multiply by 2^n
  (= (ash 7 10) (* 7 1024))
  ;; Equivalence: ash right by n == floor division by 2^n for positives
  (= (ash 1023 -3) (/ 1023 8))
  ;; Chain of shifts
  (ash (ash (ash 1 5) 5) 5)   ;; 1 << 15 = 32768
  ;; Negative left shift (same as right shift)
  (ash 1024 -3)
  (= (ash 1024 -3) (ash 1024 -3)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 1024 1048576 1073741824 -1 -1 -16 -1 42 -42 255 0 -1 t t 32768 128 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// logand, logior, logxor with multiple arguments, lognot, logcount
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitwise_multi_arg_and_logcount() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Multi-arg logand
  (logand #xff #x0f)            ;; #x0f = 15
  (logand #xff #x0f #x07)       ;; #x07 = 7
  (logand #xffff #xff00 #xf000) ;; #xf000
  ;; No-arg and single-arg
  (logand)                       ;; -1 (identity)
  (logand 42)                    ;; 42
  ;; Multi-arg logior
  (logior #x00 #x0f)            ;; #x0f
  (logior #xf0 #x0f)            ;; #xff
  (logior 1 2 4 8 16)           ;; 31
  (logior)                       ;; 0 (identity)
  (logior 42)                    ;; 42
  ;; Multi-arg logxor
  (logxor #xff #x0f)            ;; #xf0
  (logxor 1 2 4)                ;; 7
  (logxor #xaa #x55)            ;; #xff
  ;; Self-XOR is zero
  (logxor 12345 12345)          ;; 0
  ;; Triple XOR recovers original
  (logxor (logxor 42 99) 99)    ;; 42
  ;; lognot
  (lognot 0)                    ;; -1
  (lognot -1)                   ;; 0
  (lognot 255)                  ;; -256
  (= (lognot (lognot 42)) 42)
  ;; logcount: count of set bits in non-negative, count of zero bits in negative
  (logcount 0)                  ;; 0
  (logcount 1)                  ;; 1
  (logcount 7)                  ;; 3
  (logcount 255)                ;; 8
  (logcount -1)                 ;; 0
  (logcount -2)                 ;; 1
  ;; logcount of powers of 2
  (logcount 1024)               ;; 1
  (logcount (1- 1024))          ;; 10 (all bits set below)
  )"#;
    let expect = expect_test::expect![[
        r#""OK (15 7 61440 -1 42 15 255 31 0 42 240 7 255 0 42 -1 0 -256 t 0 1 3 8 0 1 1 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// GCD via Euclidean algorithm and number theory patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gcd_euclidean_algorithm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-gcd
    (lambda (a b)
      "Compute GCD using Euclidean algorithm."
      (let ((x (abs a)) (y (abs b)))
        (while (> y 0)
          (let ((temp (% x y)))
            (setq x y)
            (setq y temp)))
        x)))

  (fset 'neovm--test-lcm
    (lambda (a b)
      "Compute LCM from GCD."
      (if (or (= a 0) (= b 0)) 0
        (/ (abs (* a b)) (funcall 'neovm--test-gcd a b)))))

  ;; Extended Euclidean: returns (gcd x y) such that a*x + b*y = gcd
  (fset 'neovm--test-extended-gcd
    (lambda (a b)
      (if (= b 0)
          (list a 1 0)
        (let* ((result (funcall 'neovm--test-extended-gcd b (% a b)))
               (g (nth 0 result))
               (x (nth 1 result))
               (y (nth 2 result)))
          (list g y (- x (* (/ a b) y)))))))

  (unwind-protect
      (list
        ;; Basic GCD
        (funcall 'neovm--test-gcd 12 8)        ;; 4
        (funcall 'neovm--test-gcd 100 75)       ;; 25
        (funcall 'neovm--test-gcd 17 13)        ;; 1 (coprime)
        (funcall 'neovm--test-gcd 0 5)          ;; 5
        (funcall 'neovm--test-gcd 0 0)          ;; 0
        (funcall 'neovm--test-gcd 1000000 999999)  ;; 1
        ;; GCD with negatives
        (funcall 'neovm--test-gcd -12 8)        ;; 4
        (funcall 'neovm--test-gcd -15 -25)      ;; 5
        ;; LCM
        (funcall 'neovm--test-lcm 4 6)          ;; 12
        (funcall 'neovm--test-lcm 12 18)        ;; 36
        (funcall 'neovm--test-lcm 7 13)         ;; 91
        (funcall 'neovm--test-lcm 0 5)          ;; 0
        ;; Extended GCD: verify a*x + b*y = gcd
        (let* ((result (funcall 'neovm--test-extended-gcd 35 15))
               (g (nth 0 result))
               (x (nth 1 result))
               (y (nth 2 result)))
          (list g (= g (+ (* 35 x) (* 15 y)))))
        (let* ((result (funcall 'neovm--test-extended-gcd 240 46))
               (g (nth 0 result))
               (x (nth 1 result))
               (y (nth 2 result)))
          (list g (= g (+ (* 240 x) (* 46 y)))))
        ;; GCD property: gcd(a,b) divides both a and b
        (let ((g (funcall 'neovm--test-gcd 360 150)))
          (list g (= 0 (% 360 g)) (= 0 (% 150 g)))))
    (fmakunbound 'neovm--test-gcd)
    (fmakunbound 'neovm--test-lcm)
    (fmakunbound 'neovm--test-extended-gcd)))"#;
    let expect =
        expect_test::expect![[r#""OK (4 25 1 5 0 1 4 5 12 36 91 0 (5 t) (2 t) (30 t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bit manipulation: popcount, is-power-of-2, next-power-of-2, bit-reverse
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bit_manipulation_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-popcount
    (lambda (n)
      "Count set bits in non-negative integer via iterated clearing."
      (let ((count 0) (val n))
        (while (> val 0)
          (setq val (logand val (1- val)))  ;; clear lowest set bit
          (setq count (1+ count)))
        count)))

  (fset 'neovm--test-power-of-2-p
    (lambda (n)
      "Check if n is a power of 2 (positive)."
      (and (> n 0)
           (= 0 (logand n (1- n))))))

  (fset 'neovm--test-next-power-of-2
    (lambda (n)
      "Find smallest power of 2 >= n."
      (if (<= n 1) 1
        (let ((p 1))
          (while (< p n)
            (setq p (ash p 1)))
          p))))

  (fset 'neovm--test-lowest-set-bit
    (lambda (n)
      "Isolate the lowest set bit."
      (logand n (- n))))

  (unwind-protect
      (list
        ;; popcount
        (funcall 'neovm--test-popcount 0)
        (funcall 'neovm--test-popcount 1)
        (funcall 'neovm--test-popcount 7)     ;; 3
        (funcall 'neovm--test-popcount 255)   ;; 8
        (funcall 'neovm--test-popcount 1023)  ;; 10
        ;; popcount matches logcount for positives
        (= (funcall 'neovm--test-popcount 12345) (logcount 12345))
        ;; power-of-2 check
        (funcall 'neovm--test-power-of-2-p 1)
        (funcall 'neovm--test-power-of-2-p 2)
        (funcall 'neovm--test-power-of-2-p 1024)
        (funcall 'neovm--test-power-of-2-p 3)     ;; nil
        (funcall 'neovm--test-power-of-2-p 0)     ;; nil
        (funcall 'neovm--test-power-of-2-p 100)   ;; nil
        ;; next power of 2
        (funcall 'neovm--test-next-power-of-2 1)   ;; 1
        (funcall 'neovm--test-next-power-of-2 3)   ;; 4
        (funcall 'neovm--test-next-power-of-2 5)   ;; 8
        (funcall 'neovm--test-next-power-of-2 16)  ;; 16
        (funcall 'neovm--test-next-power-of-2 17)  ;; 32
        (funcall 'neovm--test-next-power-of-2 1000) ;; 1024
        ;; lowest set bit
        (funcall 'neovm--test-lowest-set-bit 12)  ;; 4  (0b1100 -> 0b0100)
        (funcall 'neovm--test-lowest-set-bit 8)   ;; 8
        (funcall 'neovm--test-lowest-set-bit 7)   ;; 1
        (funcall 'neovm--test-lowest-set-bit 6)   ;; 2
        )
    (fmakunbound 'neovm--test-popcount)
    (fmakunbound 'neovm--test-power-of-2-p)
    (fmakunbound 'neovm--test-next-power-of-2)
    (fmakunbound 'neovm--test-lowest-set-bit)))"#;
    let expect =
        expect_test::expect![[r#""OK (0 1 3 8 10 t t t t nil nil nil 1 4 8 16 32 1024 4 8 1 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modular arithmetic: modular exponentiation and Fermat primality test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_modular_exponentiation_and_primality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mod-exp
    (lambda (base exp modulus)
      "Compute (base^exp) mod modulus via repeated squaring."
      (let ((result 1)
            (b (mod base modulus))
            (e exp))
        (while (> e 0)
          (when (= 1 (% e 2))
            (setq result (mod (* result b) modulus)))
          (setq e (/ e 2))
          (setq b (mod (* b b) modulus)))
        result)))

  ;; Simple trial-division primality
  (fset 'neovm--test-prime-p
    (lambda (n)
      (cond
        ((< n 2) nil)
        ((= n 2) t)
        ((= 0 (% n 2)) nil)
        (t (let ((d 3) (is-prime t))
             (while (and is-prime (<= (* d d) n))
               (when (= 0 (% n d))
                 (setq is-prime nil))
               (setq d (+ d 2)))
             is-prime)))))

  ;; Euler's totient function for small n
  (fset 'neovm--test-euler-totient
    (lambda (n)
      (let ((count 0) (i 1))
        (while (<= i n)
          ;; gcd by Euclidean
          (let ((a i) (b n))
            (while (> b 0)
              (let ((temp (% a b)))
                (setq a b)
                (setq b temp)))
            (when (= a 1)
              (setq count (1+ count))))
          (setq i (1+ i)))
        count)))

  (unwind-protect
      (list
        ;; Modular exponentiation
        (funcall 'neovm--test-mod-exp 2 10 1000)     ;; 1024 mod 1000 = 24
        (funcall 'neovm--test-mod-exp 3 13 100)      ;; 3^13 mod 100 = 1594323 mod 100 = 23
        (funcall 'neovm--test-mod-exp 7 0 13)        ;; 1
        (funcall 'neovm--test-mod-exp 5 1 100)       ;; 5
        (funcall 'neovm--test-mod-exp 2 20 1000000)  ;; 1048576
        ;; Fermat's little theorem: a^(p-1) ≡ 1 (mod p) for prime p, gcd(a,p)=1
        (funcall 'neovm--test-mod-exp 2 12 13)       ;; 1
        (funcall 'neovm--test-mod-exp 3 16 17)       ;; 1
        (funcall 'neovm--test-mod-exp 5 22 23)       ;; 1
        ;; Primality
        (mapcar (lambda (n) (funcall 'neovm--test-prime-p n))
                '(1 2 3 4 5 6 7 8 9 10 11 12 13 97 100 101))
        ;; Euler's totient
        (funcall 'neovm--test-euler-totient 1)    ;; 1
        (funcall 'neovm--test-euler-totient 6)    ;; 2
        (funcall 'neovm--test-euler-totient 10)   ;; 4
        (funcall 'neovm--test-euler-totient 12)   ;; 4
        ;; For prime p, totient(p) = p - 1
        (funcall 'neovm--test-euler-totient 7)    ;; 6
        (funcall 'neovm--test-euler-totient 13)   ;; 12
        )
    (fmakunbound 'neovm--test-mod-exp)
    (fmakunbound 'neovm--test-prime-p)
    (fmakunbound 'neovm--test-euler-totient)))"#;
    let expect = expect_test::expect![[
        r#""OK (24 23 1 5 48576 1 1 1 (nil t t nil t nil t nil nil nil t nil t t nil t) 1 2 4 4 6 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modulo chains and Chinese Remainder Theorem
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_chinese_remainder_theorem() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Extended GCD helper
  (fset 'neovm--test-crt-egcd
    (lambda (a b)
      (if (= b 0)
          (list a 1 0)
        (let* ((result (funcall 'neovm--test-crt-egcd b (mod a b)))
               (g (nth 0 result))
               (x (nth 1 result))
               (y (nth 2 result)))
          (list g y (- x (* (/ a b) y)))))))

  ;; CRT for two congruences: x ≡ a1 (mod m1), x ≡ a2 (mod m2)
  (fset 'neovm--test-crt2
    (lambda (a1 m1 a2 m2)
      (let* ((result (funcall 'neovm--test-crt-egcd m1 m2))
             (g (nth 0 result))
             (p (nth 1 result))
             (q (nth 2 result))
             (lcm (/ (* m1 m2) g)))
        (if (/= 0 (% (- a2 a1) g))
            nil  ;; no solution
          (let ((x (mod (+ a1 (* m1 p (/ (- a2 a1) g))) lcm)))
            (if (< x 0) (+ x lcm) x))))))

  (unwind-protect
      (list
        ;; x ≡ 2 (mod 3), x ≡ 3 (mod 5) => x = 8 (mod 15)
        (funcall 'neovm--test-crt2 2 3 3 5)
        ;; Verify: 8 mod 3 = 2, 8 mod 5 = 3
        (list (mod 8 3) (mod 8 5))
        ;; x ≡ 1 (mod 3), x ≡ 1 (mod 5) => x = 1 (mod 15)
        (funcall 'neovm--test-crt2 1 3 1 5)
        ;; x ≡ 0 (mod 2), x ≡ 0 (mod 3) => x = 0 (mod 6)
        (funcall 'neovm--test-crt2 0 2 0 3)
        ;; x ≡ 3 (mod 7), x ≡ 5 (mod 11) => some value mod 77
        (let ((x (funcall 'neovm--test-crt2 3 7 5 11)))
          (list x (mod x 7) (mod x 11)))
        ;; Larger moduli
        (let ((x (funcall 'neovm--test-crt2 13 97 25 101)))
          (list x (mod x 97) (mod x 101)))
        ;; Chain: use CRT iteratively for three congruences
        ;; x ≡ 1 (mod 2), x ≡ 2 (mod 3), x ≡ 3 (mod 5)
        (let* ((x1 (funcall 'neovm--test-crt2 1 2 2 3))  ;; x ≡ ? (mod 6)
               (x2 (funcall 'neovm--test-crt2 x1 6 3 5)))
          (list x2 (mod x2 2) (mod x2 3) (mod x2 5))))
    (fmakunbound 'neovm--test-crt-egcd)
    (fmakunbound 'neovm--test-crt2)))"#;
    let expect = expect_test::expect![[r#""OK (8 (2 3) 1 0 (38 3 5) (9519 13 25) (23 1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modulo arithmetic stress: Fibonacci mod, factorial mod
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_modular_fibonacci_and_factorial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Fibonacci modulo m
  (fset 'neovm--test-fib-mod
    (lambda (n m)
      (if (< n 2) (mod n m)
        (let ((a 0) (b 1) (i 2))
          (while (<= i n)
            (let ((temp (mod (+ a b) m)))
              (setq a b)
              (setq b temp))
            (setq i (1+ i)))
          b))))

  ;; Factorial modulo m
  (fset 'neovm--test-fact-mod
    (lambda (n m)
      (let ((result 1) (i 1))
        (while (<= i n)
          (setq result (mod (* result i) m))
          (setq i (1+ i)))
        result)))

  ;; Sum of digits via repeated mod and division
  (fset 'neovm--test-digit-sum
    (lambda (n)
      (let ((sum 0) (val (abs n)))
        (while (> val 0)
          (setq sum (+ sum (% val 10)))
          (setq val (/ val 10)))
        sum)))

  (unwind-protect
      (list
        ;; Fibonacci mod
        (funcall 'neovm--test-fib-mod 0 1000)    ;; 0
        (funcall 'neovm--test-fib-mod 1 1000)    ;; 1
        (funcall 'neovm--test-fib-mod 10 1000)   ;; 55
        (funcall 'neovm--test-fib-mod 20 1000)   ;; 6765 mod 1000 = 765
        (funcall 'neovm--test-fib-mod 50 997)    ;; fib(50) mod 997
        ;; Factorial mod
        (funcall 'neovm--test-fact-mod 0 1000)   ;; 1
        (funcall 'neovm--test-fact-mod 5 1000)   ;; 120
        (funcall 'neovm--test-fact-mod 10 997)   ;; 3628800 mod 997
        (funcall 'neovm--test-fact-mod 20 1000000007)
        ;; Wilson's theorem: (p-1)! ≡ -1 (mod p) for prime p
        ;; i.e., (p-1)! mod p = p-1
        (= (funcall 'neovm--test-fact-mod 6 7) 6)    ;; 6! mod 7 = 720 mod 7 = 6
        (= (funcall 'neovm--test-fact-mod 10 11) 10)
        ;; Digit sum
        (funcall 'neovm--test-digit-sum 0)
        (funcall 'neovm--test-digit-sum 12345)    ;; 15
        (funcall 'neovm--test-digit-sum 999999)   ;; 54
        (funcall 'neovm--test-digit-sum -42)       ;; 6
        ;; Divisibility by 9: digit sum mod 9 = number mod 9
        (let ((n 123456789))
          (= (mod (funcall 'neovm--test-digit-sum n) 9)
             (mod n 9))))
    (fmakunbound 'neovm--test-fib-mod)
    (fmakunbound 'neovm--test-fact-mod)
    (fmakunbound 'neovm--test-digit-sum)))"#;
    let expect =
        expect_test::expect![[r#""OK (0 1 55 765 448 1 120 717 146326063 t t 0 15 54 6 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sieve of Eratosthenes using bitwise operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sieve_of_eratosthenes_bitwise() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-sieve
    (lambda (limit)
      "Sieve of Eratosthenes up to LIMIT using a vector as bit array."
      (let ((sieve (make-vector (1+ limit) t))
            (primes nil))
        (aset sieve 0 nil)
        (when (> limit 0) (aset sieve 1 nil))
        (let ((i 2))
          (while (<= (* i i) limit)
            (when (aref sieve i)
              (let ((j (* i i)))
                (while (<= j limit)
                  (aset sieve j nil)
                  (setq j (+ j i)))))
            (setq i (1+ i))))
        (let ((i 2))
          (while (<= i limit)
            (when (aref sieve i)
              (setq primes (cons i primes)))
            (setq i (1+ i))))
        (nreverse primes))))

  (unwind-protect
      (list
        ;; Primes up to 30
        (funcall 'neovm--test-sieve 30)
        ;; Count of primes up to 100
        (length (funcall 'neovm--test-sieve 100))    ;; 25
        ;; Primes up to 2 (edge)
        (funcall 'neovm--test-sieve 2)
        ;; Primes up to 1
        (funcall 'neovm--test-sieve 1)
        ;; Goldbach check: every even number > 2 up to 50 is sum of two primes
        (let ((primes (funcall 'neovm--test-sieve 50))
              (ok t))
          (let ((n 4))
            (while (<= n 50)
              (let ((found nil))
                (dolist (p primes)
                  (when (and (<= p (/ n 2))
                             (memq (- n p) primes))
                    (setq found t)))
                (unless found (setq ok nil)))
              (setq n (+ n 2))))
          ok)
        ;; Twin primes up to 100: pairs (p, p+2) both prime
        (let ((primes (funcall 'neovm--test-sieve 100))
              (twins nil))
          (dolist (p primes)
            (when (memq (+ p 2) primes)
              (setq twins (cons (list p (+ p 2)) twins))))
          (nreverse twins)))
    (fmakunbound 'neovm--test-sieve)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 3 5 7 11 13 17 19 23 29) 25 (2) nil t ((3 5) (5 7) (11 13) (17 19) (29 31) (41 43) (59 61) (71 73)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bitwise encoding: run-length encoding of bit patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitwise_run_length_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Extract individual bits from an integer (LSB first, up to num-bits)
  (fset 'neovm--test-int-to-bits
    (lambda (n num-bits)
      (let ((bits nil) (i 0))
        (while (< i num-bits)
          (setq bits (cons (logand (ash n (- i)) 1) bits))
          (setq i (1+ i)))
        (nreverse bits))))

  ;; Run-length encode a list of bits
  (fset 'neovm--test-rle-encode
    (lambda (bits)
      (if (null bits) nil
        (let ((runs nil)
              (current (car bits))
              (count 1)
              (rest (cdr bits)))
          (while rest
            (if (= (car rest) current)
                (setq count (1+ count))
              (setq runs (cons (cons current count) runs))
              (setq current (car rest))
              (setq count 1))
            (setq rest (cdr rest)))
          (setq runs (cons (cons current count) runs))
          (nreverse runs)))))

  ;; Decode RLE back to bits
  (fset 'neovm--test-rle-decode
    (lambda (runs)
      (let ((bits nil))
        (dolist (run runs)
          (let ((bit (car run))
                (count (cdr run))
                (i 0))
            (while (< i count)
              (setq bits (cons bit bits))
              (setq i (1+ i)))))
        (nreverse bits))))

  (unwind-protect
      (list
        ;; Extract bits of 0b11001010 (8 bits)
        (funcall 'neovm--test-int-to-bits #b11001010 8)
        ;; RLE encode
        (funcall 'neovm--test-rle-encode '(1 1 0 0 1 0 1 0))
        ;; RLE of all-ones
        (funcall 'neovm--test-rle-encode '(1 1 1 1 1))
        ;; RLE of alternating
        (funcall 'neovm--test-rle-encode '(0 1 0 1 0 1))
        ;; Roundtrip: encode then decode
        (let* ((original '(1 1 1 0 0 1 0 0 0 1 1))
               (encoded (funcall 'neovm--test-rle-encode original))
               (decoded (funcall 'neovm--test-rle-decode encoded)))
          (equal original decoded))
        ;; RLE compression ratio: long runs compress well
        (let* ((bits (funcall 'neovm--test-int-to-bits #xff00ff00 32))
               (runs (funcall 'neovm--test-rle-encode bits)))
          (list (length bits) (length runs)))
        ;; Empty input
        (funcall 'neovm--test-rle-encode nil)
        (funcall 'neovm--test-rle-decode nil))
    (fmakunbound 'neovm--test-int-to-bits)
    (fmakunbound 'neovm--test-rle-encode)
    (fmakunbound 'neovm--test-rle-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 0 1 0 0 1 1) ((1 . 2) (0 . 2) (1 . 1) (0 . 1) (1 . 1) (0 . 1)) ((1 . 5)) ((0 . 1) (1 . 1) (0 . 1) (1 . 1) (0 . 1) (1 . 1)) t (32 4) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
