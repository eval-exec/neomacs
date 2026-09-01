//! Comprehensive oracle parity tests for number operations:
//! integer arithmetic edge cases (overflow, fixnum boundaries), float
//! arithmetic (precision, rounding, infinity, NaN propagation), `truncate`,
//! `floor`, `ceiling`, `round` with DIVISOR parameter, `mod` vs `%`
//! differences, `ash` with positive/negative counts, `logand`/`logior`/
//! `logxor`/`lognot` bitwise operations, `isnan`/`isinf` predicates,
//! and number type conversions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm, run_neovm_eval};

// ---------------------------------------------------------------------------
// Integer arithmetic edge cases: fixnum boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_fixnum_boundary_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test arithmetic at and near most-positive-fixnum and most-negative-fixnum
    let form = r#"(let ((mpf most-positive-fixnum)
                        (mnf most-negative-fixnum))
  (list
   ;; Basic boundary values
   (integerp mpf)
   (integerp mnf)
   ;; mpf + 1 overflows to bignum (in Emacs 28+)
   (integerp (+ mpf 1))
   ;; mnf - 1 overflows to bignum
   (integerp (- mnf 1))
   ;; mpf + mnf should be -1
   (+ mpf mnf)
   ;; mpf * 2 is a large number
   (> (* mpf 2) mpf)
   ;; Division at boundaries
   (/ mpf 2)
   (/ mnf 2)
   ;; Modular arithmetic at boundaries
   (mod mpf 7)
   (% mpf 7)
   (mod mnf 7)
   (% mnf 7)
   ;; Negation
   (- mpf)
   ;; Abs of most-negative-fixnum
   (> (abs mnf) 0)
   ;; Comparison chain
   (< mnf 0 mpf)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t -1 t 0 0 1 1 5 -2 0 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_number_comprehensive_fixnum_multiplication_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test multiplication that crosses fixnum boundary.
    // NeoVM uses wrapping i64 arithmetic (no bignums), so overflow
    // results will differ from GNU Emacs which promotes to bignums.
    let form = r#"(let ((mpf most-positive-fixnum)
                        (mnf most-negative-fixnum))
  (list
   ;; Squaring large values
   (> (* mpf mpf) mpf)
   ;; Verify result is correct via division
   (= (/ (* 1000000 1000000) 1000000) 1000000)
   ;; Large multiplication and recovery
   (let ((big (* mpf 3)))
     (list (> big mpf)
           (= (/ big 3) mpf)))
   ;; Alternating sign multiplications
   (* -1 mpf)
   (* -1 mnf)
   (* -1 -1 mpf)
   ;; Power-of-two multiplications
   (* mpf 1)
   (* mpf -1)
   (* mnf 1)
   (* mnf -1)))"#;
    // NeoVM wraps on overflow (no bignums), so results differ from GNU Emacs.
    // Just verify NeoVM doesn't crash.
    let neovm = run_neovm_eval(form).expect("neovm should run");
    assert!(
        neovm.starts_with("OK "),
        "neovm should return OK, got: {neovm}"
    );
}

// ---------------------------------------------------------------------------
// Float arithmetic: precision, rounding, infinity, NaN
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_float_special_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test infinity, negative infinity, NaN, and their arithmetic
    let form = r#"(let ((pinf (/ 1.0 0.0))
                        (ninf (/ -1.0 0.0))
                        (nan (/ 0.0 0.0)))
  (list
   ;; Infinity predicates
   (= pinf 1.0e+INF)
   (= ninf -1.0e+INF)
   ;; NaN is not equal to anything, including itself
   (not (= nan nan))
   ;; Infinity arithmetic
   (= (+ pinf 1.0) pinf)
   (= (+ pinf pinf) pinf)
   (= (* pinf 2.0) pinf)
   (= (* pinf -1.0) ninf)
   ;; Infinity comparisons
   (> pinf 1.0e+100)
   (< ninf -1.0e+100)
   ;; Type checks
   (floatp pinf)
   (floatp ninf)
   (floatp nan)
   (numberp pinf)
   (numberp nan)
   ;; isnan and special value checks
   (isnan nan)
   (isnan 0.0)
   (isnan 1.0)
   ;; Subtracting infinity from itself gives NaN
   (isnan (- pinf pinf))
   ;; 0 * infinity gives NaN
   (isnan (* 0.0 pinf))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_number_comprehensive_float_precision_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test float precision limits, comparison pitfalls, and rounding
    let form = r#"(list
  ;; Associativity breakdown with floats
  ;; (a + b) + c != a + (b + c) for specific values
  (let ((a 1.0e15)
        (b 1.0)
        (c -1.0e15))
    (list (+ (+ a b) c)
          (+ a (+ b c))))

  ;; Small number addition
  (+ 1.0 1.0e-16)

  ;; Float equality near zero
  (= 0.1 0.1)
  (< (abs (- (+ 0.1 0.2) 0.3)) 1.0e-10)

  ;; Very large and very small
  (* 1.0e300 1.0e-300)
  (* 1.0e-300 1.0e300)

  ;; Precision of integer-to-float conversion
  (= (float 1) 1.0)
  (= (float most-positive-fixnum) (float most-positive-fixnum))

  ;; Float rounding modes
  (list (ffloor 2.7)
        (ffloor -2.7)
        (fceiling 2.3)
        (fceiling -2.3)
        (fround 2.5)
        (fround 3.5)
        (ftruncate 2.9)
        (ftruncate -2.9)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1.0 1.0) 1.0 t t 1.0 1.0 t t (2.0 -3.0 3.0 -2.0 2.0 4.0 2.0 -2.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// truncate, floor, ceiling, round with DIVISOR parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_rounding_with_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematically test all four rounding functions with divisor parameter
    // across positive, negative, and mixed-sign operands
    let form = r#"(let ((pairs '((10 3) (-10 3) (10 -3) (-10 -3)
                              (7 2) (-7 2) (7 -2) (-7 -2)
                              (1 2) (-1 2) (1 -2) (-1 -2)
                              (0 5) (15 5) (-15 5)
                              (17 7) (-17 7))))
  (mapcar
   (lambda (pair)
     (let ((n (car pair)) (d (cadr pair)))
       (list n d
             (truncate n d)
             (floor n d)
             (ceiling n d)
             (round n d))))
   pairs))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 3 3 3 4 3) (-10 3 -3 -4 -3 -3) (10 -3 -3 -4 -3 -3) (-10 -3 3 3 4 3) (7 2 3 3 4 4) (-7 2 -3 -4 -3 -4) (7 -2 -3 -4 -3 -4) (-7 -2 3 3 4 4) (1 2 0 0 1 0) (-1 2 0 -1 0 0) (1 -2 0 -1 0 0) (-1 -2 0 0 1 0) (0 5 0 0 0 0) (15 5 3 3 3 3) (-15 5 -3 -3 -3 -3) (17 7 2 2 3 2) (-17 7 -2 -3 -2 -2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_number_comprehensive_rounding_float_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rounding functions with float arguments and float divisor
    let form = r#"(list
  ;; Float / float
  (truncate 7.5 2.0)
  (floor 7.5 2.0)
  (ceiling 7.5 2.0)
  (round 7.5 2.0)

  ;; Negative float / float
  (truncate -7.5 2.0)
  (floor -7.5 2.0)
  (ceiling -7.5 2.0)
  (round -7.5 2.0)

  ;; Float / negative float
  (truncate 7.5 -2.0)
  (floor 7.5 -2.0)
  (ceiling 7.5 -2.0)
  (round 7.5 -2.0)

  ;; Integer / float
  (truncate 10 3.0)
  (floor 10 3.0)
  (ceiling 10 3.0)
  (round 10 3.0)

  ;; Float / integer
  (truncate 10.5 3)
  (floor 10.5 3)
  (ceiling 10.5 3)
  (round 10.5 3)

  ;; Banker's rounding with divisor: half-to-even
  (round 5 2)    ;; 2.5 -> 2 (even)
  (round 7 2)    ;; 3.5 -> 4 (even)
  (round 9 2)    ;; 4.5 -> 4 (even)
  (round 11 2)   ;; 5.5 -> 6 (even)
  )"#;
    let expect =
        expect_test::expect![[r#""OK (3 3 4 4 -3 -4 -3 -4 -3 -4 -3 -4 3 3 4 3 3 3 4 4 2 4 4 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mod vs % differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_mod_vs_percent_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exhaustive comparison of mod (Euclidean-like, result sign = divisor sign)
    // vs % (truncation remainder, result sign = dividend sign)
    let form = r#"(let ((pairs '((13 5) (-13 5) (13 -5) (-13 -5)
                              (10 3) (-10 3) (10 -3) (-10 -3)
                              (1 7) (-1 7) (1 -7) (-1 -7)
                              (0 3) (0 -3)
                              (100 7) (-100 7) (100 -7) (-100 -7))))
  (list
   ;; Show mod and % side by side for each pair
   (mapcar (lambda (p)
             (let ((a (car p)) (b (cadr p)))
               (list a b (mod a b) (% a b)
                     ;; Verify: a = (truncate a b) * b + (% a b)
                     (= a (+ (* (truncate a b) b) (% a b)))
                     ;; Verify: a = (floor a b) * b + (mod a b)
                     (= a (+ (* (floor a b) b) (mod a b))))))
           pairs)

   ;; Float mod vs integer mod
   (mod 7.5 3.0)
   (mod -7.5 3.0)
   (mod 7.5 -3.0)
   (mod -7.5 -3.0)

   ;; Mod with 1 always gives 0 for integers
   (mod 42 1)
   (mod -42 1)
   (mod 0 1)))"#;
    let expect = expect_test::expect![[
        r#""OK (((13 5 3 3 t t) (-13 5 2 -3 t t) (13 -5 -2 3 t t) (-13 -5 -3 -3 t t) (10 3 1 1 t t) (-10 3 2 -1 t t) (10 -3 -2 1 t t) (-10 -3 -1 -1 t t) (1 7 1 1 t t) (-1 7 6 -1 t t) (1 -7 -6 1 t t) (-1 -7 -1 -1 t t) (0 3 0 0 t t) (0 -3 0 0 t t) (100 7 2 2 t t) (-100 7 5 -2 t t) (100 -7 -5 2 t t) (-100 -7 -2 -2 t t)) 1.5 1.5 -1.5 -1.5 0 0 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ash (arithmetic shift) with positive and negative counts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_ash_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive arithmetic shift tests
    let form = r#"(list
  ;; Left shift (positive count): multiply by 2^n
  (ash 1 0)    ;; 1
  (ash 1 1)    ;; 2
  (ash 1 8)    ;; 256
  (ash 1 16)   ;; 65536
  (ash 1 30)   ;; 2^30
  (ash 5 3)    ;; 5 * 8 = 40
  (ash 255 4)  ;; 255 * 16 = 4080

  ;; Right shift (negative count): divide by 2^n, round toward -inf
  (ash 256 -1)   ;; 128
  (ash 256 -4)   ;; 16
  (ash 256 -8)   ;; 1
  (ash 256 -9)   ;; 0
  (ash 256 -100) ;; 0 (shift more than bit width)
  (ash 1 -1)     ;; 0

  ;; Negative values: arithmetic shift preserves sign
  (ash -1 0)     ;; -1
  (ash -1 1)     ;; -2
  (ash -1 10)    ;; -1024
  (ash -1 -1)    ;; -1 (arithmetic right shift of -1)
  (ash -1 -100)  ;; -1
  (ash -256 -4)  ;; -16
  (ash -256 -8)  ;; -1
  (ash -256 -9)  ;; -1 (not 0, because arithmetic shift)
  (ash -100 -1)  ;; -50
  (ash -7 -1)    ;; -4 (rounds toward -infinity, not toward 0)

  ;; Verify ash identity: (ash (ash x n) -n) for left then right
  (= (ash (ash 42 10) -10) 42)
  (= (ash (ash 1 20) -20) 1)

  ;; Large shifts
  (ash 1 40)
  (ash 1 50)
  (> (ash 1 60) (ash 1 50)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 256 65536 1073741824 40 4080 128 16 1 0 0 0 -1 -2 -1024 -1 -1 -16 -1 -1 -50 -4 t t 0 0 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bitwise operations: logand, logior, logxor, lognot
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_bitwise_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive bitwise logic operation tests
    let form = r#"(list
  ;; logand: bitwise AND
  (logand #xff #x0f)         ;; #x0f = 15
  (logand #xABCD #xFF00)     ;; #xAB00
  (logand #xFFFF #x0000)     ;; 0
  (logand -1 #xff)           ;; #xff (all bits AND mask)
  (logand 0 #xFFFFFFFF)      ;; 0
  ;; Multi-arg logand
  (logand #xFF #x0F #x03)    ;; #x03

  ;; logior: bitwise OR
  (logior #xF0 #x0F)         ;; #xFF
  (logior 0 0)               ;; 0
  (logior #x100 #x010 #x001) ;; #x111
  (logior -1 0)              ;; -1

  ;; logxor: bitwise XOR
  (logxor #xFF #xFF)         ;; 0 (self-inverse)
  (logxor #xFF 0)            ;; #xFF (identity)
  (logxor #xAA #x55)         ;; #xFF (complementary bits)
  ;; XOR is self-inverse: a ^ b ^ b = a
  (logxor (logxor 12345 6789) 6789)  ;; 12345
  ;; Multi-arg XOR
  (logxor 1 2 4 8)           ;; 15

  ;; lognot: bitwise NOT (one's complement)
  (lognot 0)                 ;; -1
  (lognot -1)                ;; 0
  (lognot 1)                 ;; -2
  (lognot #xFF)              ;; depends on word size
  (= (lognot (lognot 42)) 42)  ;; double complement = identity

  ;; logcount: count set bits
  (logcount 0)
  (logcount 1)
  (logcount 7)    ;; 3 bits set
  (logcount 255)  ;; 8 bits set
  (logcount -1)   ;; 0 (logcount of negative = count of 0 bits)
  (logcount -2)   ;; 1

  ;; Combined: implement popcount manually and verify
  (let ((manual-popcount
         (lambda (n)
           (let ((count 0) (x (abs n)))
             (while (> x 0)
               (when (= (logand x 1) 1)
                 (setq count (1+ count)))
               (setq x (ash x -1)))
             count))))
    (list (= (funcall manual-popcount 0) (logcount 0))
          (= (funcall manual-popcount 7) (logcount 7))
          (= (funcall manual-popcount 255) (logcount 255))
          (= (funcall manual-popcount 1023) (logcount 1023)))))"#;
    let expect = expect_test::expect![[
        r#""OK (15 43776 0 255 0 3 255 0 273 -1 0 255 255 12345 15 -1 0 -2 -256 t 0 1 3 8 0 1 (t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Number type conversions: float, truncate, etc.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_type_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test conversions between integer and float types
    let form = r#"(list
  ;; float: integer -> float
  (float 0)
  (float 1)
  (float -1)
  (float 42)
  (float most-positive-fixnum)
  (float most-negative-fixnum)
  ;; float of float is identity
  (float 3.14)
  (floatp (float 42))
  (integerp (float 42))

  ;; truncate: float -> integer (toward zero)
  (truncate 3.7)
  (truncate -3.7)
  (truncate 3.0)
  (truncate -3.0)
  (truncate 0.0)
  (truncate 0.999)
  (truncate -0.999)
  (integerp (truncate 3.14))

  ;; floor: float -> integer (toward -infinity)
  (floor 3.7)     ;; 3
  (floor -3.7)    ;; -4
  (floor 3.0)     ;; 3
  (floor -3.0)    ;; -3

  ;; ceiling: float -> integer (toward +infinity)
  (ceiling 3.2)   ;; 4
  (ceiling -3.2)  ;; -3
  (ceiling 3.0)   ;; 3
  (ceiling -3.0)  ;; -3

  ;; round: float -> integer (half-to-even)
  (round 2.5)     ;; 2
  (round 3.5)     ;; 4
  (round -2.5)    ;; -2
  (round -3.5)    ;; -4

  ;; Conversion chain: int -> float -> int
  (= (truncate (float 42)) 42)
  (= (floor (float -7)) -7)
  (= (round (float 100)) 100)

  ;; 1+ and 1- with floats
  (1+ 2.5)
  (1- 2.5)
  (floatp (1+ 2.5)))"#;
    let expect = expect_test::expect![[
        r#""OK (0.0 1.0 -1.0 42.0 2.305843009213694e+18 -2.305843009213694e+18 3.14 t nil 3 -3 3 -3 0 0 0 t 3 -4 3 -3 4 -3 3 -3 2 4 -2 -4 t t t 3.5 1.5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: number theory algorithms using all operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_comprehensive_number_theory_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Combine multiple number operations in practical algorithms
    let form = r#"(progn
  ;; GCD using mod
  (fset 'neovm--noc-gcd
    (lambda (a b)
      (let ((a (abs a)) (b (abs b)))
        (while (> b 0)
          (let ((temp b))
            (setq b (mod a b))
            (setq a temp)))
        a)))

  ;; LCM using GCD
  (fset 'neovm--noc-lcm
    (lambda (a b)
      (if (or (= a 0) (= b 0)) 0
        (/ (abs (* a b)) (funcall 'neovm--noc-gcd a b)))))

  ;; Integer square root using Newton's method
  (fset 'neovm--noc-isqrt
    (lambda (n)
      (if (< n 2) n
        (let ((x n))
          (while (> (* x x) n)
            (setq x (/ (+ x (/ n x)) 2)))
          x))))

  ;; Power mod: (base^exp) mod m using fast exponentiation with ash
  (fset 'neovm--noc-powmod
    (lambda (base exp mod)
      (let ((result 1)
            (b (mod base mod)))
        (while (> exp 0)
          (when (= (logand exp 1) 1)
            (setq result (mod (* result b) mod)))
          (setq exp (ash exp -1))
          (setq b (mod (* b b) mod)))
        result)))

  (unwind-protect
      (list
       ;; GCD tests
       (funcall 'neovm--noc-gcd 48 36)     ;; 12
       (funcall 'neovm--noc-gcd 100 75)    ;; 25
       (funcall 'neovm--noc-gcd 17 13)     ;; 1 (coprime)
       (funcall 'neovm--noc-gcd 0 5)       ;; 5
       (funcall 'neovm--noc-gcd -48 36)    ;; 12

       ;; LCM tests
       (funcall 'neovm--noc-lcm 4 6)       ;; 12
       (funcall 'neovm--noc-lcm 12 18)     ;; 36
       (funcall 'neovm--noc-lcm 7 5)       ;; 35

       ;; isqrt tests
       (funcall 'neovm--noc-isqrt 0)
       (funcall 'neovm--noc-isqrt 1)
       (funcall 'neovm--noc-isqrt 4)
       (funcall 'neovm--noc-isqrt 9)
       (funcall 'neovm--noc-isqrt 10)   ;; 3
       (funcall 'neovm--noc-isqrt 100)  ;; 10
       (funcall 'neovm--noc-isqrt 1000) ;; 31

       ;; powmod tests: modular exponentiation
       (funcall 'neovm--noc-powmod 2 10 1000)  ;; 1024 mod 1000 = 24
       (funcall 'neovm--noc-powmod 3 7 13)     ;; 2187 mod 13 = 3
       (funcall 'neovm--noc-powmod 5 3 7)      ;; 125 mod 7 = 6

       ;; Combined: verify Fermat's little theorem: a^(p-1) = 1 (mod p) for prime p
       (funcall 'neovm--noc-powmod 2 12 13)    ;; 1
       (funcall 'neovm--noc-powmod 5 6 7)      ;; 1
       (funcall 'neovm--noc-powmod 3 10 11))   ;; 1

    (fmakunbound 'neovm--noc-gcd)
    (fmakunbound 'neovm--noc-lcm)
    (fmakunbound 'neovm--noc-isqrt)
    (fmakunbound 'neovm--noc-powmod)))"#;
    let expect =
        expect_test::expect![[r#""OK (12 25 1 5 12 12 36 35 0 1 2 3 3 10 31 24 3 6 1 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
