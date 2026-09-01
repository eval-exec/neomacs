//! Comprehensive oracle parity tests for number arithmetic:
//! all arithmetic operators (+, -, *, /, %, mod, 1+, 1-) with 0, 1, 2, many args,
//! integer overflow to float promotion, most-positive-fixnum / most-negative-fixnum
//! arithmetic, mixed int/float, ash all directions, lsh, logand, logior, logxor,
//! lognot with large/negative values, truncate/floor/ceiling/round with all arg
//! combinations, and expt with negative/zero/float bases and exponents.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// + with 0, 1, 2, many args and mixed types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_addition_all_arities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Zero args
  (+)
  ;; One arg
  (+ 42)
  (+ -17)
  (+ 3.14)
  ;; Two args: int+int, int+float, float+float
  (+ 3 4)
  (+ 3 4.0)
  (+ 3.0 4.0)
  (+ -5 -10)
  (+ -5 10)
  ;; Many args: all ints, all floats, mixed
  (+ 1 2 3 4 5 6 7 8 9 10)
  (+ 1.0 2.0 3.0 4.0 5.0)
  (+ 1 2.0 3 4.0 5)
  ;; Identity element: 0
  (+ 0 42)
  (+ 42 0)
  (+ 0 0 0 0 42 0 0 0)
  ;; Large accumulation
  (+ 1000000 2000000 3000000 4000000 5000000)
  ;; Cancellation
  (+ 100 -100)
  (+ 50 50 -100)
  ;; Overflow: fixnum boundary
  (+ most-positive-fixnum 1)
  (integerp (+ most-positive-fixnum 1))
  (+ most-negative-fixnum -1)
  (integerp (+ most-negative-fixnum -1))
  ;; Sum near boundary
  (+ most-positive-fixnum most-negative-fixnum))"#;
    let expect = expect_test::expect![[
        r#""OK (0 42 -17 3.14 7 7.0 7.0 -15 5 55 15.0 15.0 42 42 42 15000000 0 0 2305843009213693952 t -2305843009213693953 t -1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// - with 0, 1, 2, many args
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_subtraction_all_arities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; One arg: negation
  (- 42)
  (- -42)
  (- 0)
  (- 3.14)
  (- -3.14)
  ;; Two args
  (- 10 3)
  (- 3 10)
  (- 10 3.0)
  (- 10.0 3)
  (- -5 -10)
  ;; Many args: left-associative: (((a - b) - c) - d)
  (- 100 10 20 30)
  (- 100 10 20 30 40)
  (- 100.0 10 20.0 30)
  ;; Subtraction at fixnum boundary
  (- most-negative-fixnum 1)
  (- most-positive-fixnum -1)
  (- 0 most-positive-fixnum)
  (- 0 most-negative-fixnum)
  ;; Self-subtraction
  (- most-positive-fixnum most-positive-fixnum)
  (- most-negative-fixnum most-negative-fixnum))"#;
    let expect = expect_test::expect![[
        r#""OK (-42 42 0 -3.14 3.14 7 -7 7.0 7.0 5 40 0 40.0 -2305843009213693953 2305843009213693952 0 2305843009213693952 0 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// * with 0, 1, 2, many args
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_multiplication_all_arities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Zero args
  (*)
  ;; One arg
  (* 42)
  (* -7)
  (* 3.14)
  ;; Two args
  (* 6 7)
  (* -6 7)
  (* -6 -7)
  (* 6 7.0)
  (* 6.0 7.0)
  ;; Many args
  (* 1 2 3 4 5)
  (* 1 2 3 4 5 6 7 8 9 10)
  (* 2 2 2 2 2 2 2 2 2 2)
  ;; Identity: 1
  (* 1 42)
  (* 42 1)
  (* 1 1 1 1 42 1 1)
  ;; Zero annihilator
  (* 0 999999)
  (* 999999 0)
  (* 1 2 3 0 4 5)
  ;; Mixed int/float
  (* 2 3.5)
  (* -2 3.5)
  (* 2.0 -3.5)
  ;; Overflow to bignum
  (* most-positive-fixnum 2)
  (> (* most-positive-fixnum 2) most-positive-fixnum)
  (* most-negative-fixnum 2)
  (< (* most-negative-fixnum 2) most-negative-fixnum)
  ;; Sign rules
  (* -1 most-positive-fixnum)
  (* -1 most-negative-fixnum)
  (* -1 -1 -1 -1)
  (* -1 -1 -1))"#;
    let expect = expect_test::expect![[
        r#""OK (1 42 -7 3.14 42 -42 42 42.0 42.0 120 3628800 1024 42 42 42 0 0 0 7.0 -7.0 -7.0 4611686018427387902 t -4611686018427387904 t 0 2305843009213693952 1 -1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// / (division) with all combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_division_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; One arg: reciprocal (integer: 1/n truncated)
  (/ 1)
  (/ 2)
  (/ -1)
  (/ 3)
  ;; Two args: integer division truncates toward zero
  (/ 10 3)
  (/ -10 3)
  (/ 10 -3)
  (/ -10 -3)
  (/ 7 2)
  (/ -7 2)
  (/ 7 -2)
  (/ -7 -2)
  ;; Integer division: exact
  (/ 100 10)
  (/ -100 10)
  (/ 100 -10)
  (/ -100 -10)
  ;; Float division: exact
  (/ 7.0 2)
  (/ 7 2.0)
  (/ -7.0 2)
  (/ 7.0 -2)
  (/ 1.0 3)
  (/ 1.0 3.0)
  ;; Many args: left-associative ((a/b)/c)/d
  (/ 1000 10 10)
  (/ 1000 10 10 10)
  (/ 120 2 3 4 5)
  (/ 120.0 2 3 4 5)
  ;; Division of 0
  (/ 0 5)
  (/ 0 -5)
  (/ 0 5.0)
  ;; Division at fixnum boundary
  (/ most-positive-fixnum 2)
  (/ most-negative-fixnum 2)
  (/ most-positive-fixnum most-positive-fixnum)
  (/ most-negative-fixnum most-negative-fixnum)
  (/ most-positive-fixnum -1)
  ;; Float infinity from division
  (/ 1.0 0.0)
  (/ -1.0 0.0)
  (isnan (/ 0.0 0.0)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 0 -1 0 3 -3 -3 3 3 -3 -3 3 10 -10 -10 10 3.5 3.5 -3.5 -3.5 0.3333333333333333 0.3333333333333333 10 1 1 1.0 0 0 0.0 0 0 1 1 0 1.0e+INF -1.0e+INF t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Division by zero: integer error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_division_by_zero_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (condition-case err (/ 1 0) (arith-error 'div-by-zero))
  (condition-case err (/ 0 0) (arith-error 'div-zero-zero))
  (condition-case err (/ -1 0) (arith-error 'neg-div-zero))
  (condition-case err (% 10 0) (arith-error 'mod-by-zero))
  (condition-case err (mod 10 0) (arith-error 'emod-by-zero))
  ;; Float division by zero does NOT error
  (floatp (/ 1.0 0.0))
  (floatp (/ -1.0 0.0)))"#;
    let expect = expect_test::expect![[
        r#""OK (div-by-zero div-zero-zero neg-div-zero mod-by-zero emod-by-zero t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 1+ and 1-
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_one_plus_one_minus() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; 1+ (add1)
  (1+ 0)
  (1+ 1)
  (1+ -1)
  (1+ 42)
  (1+ -42)
  (1+ 3.14)
  (1+ -3.14)
  (1+ 0.0)
  (1+ most-positive-fixnum)
  (integerp (1+ most-positive-fixnum))
  ;; 1- (sub1)
  (1- 0)
  (1- 1)
  (1- -1)
  (1- 42)
  (1- -42)
  (1- 3.14)
  (1- -3.14)
  (1- 0.0)
  (1- most-negative-fixnum)
  (integerp (1- most-negative-fixnum))
  ;; Type preservation
  (integerp (1+ 5))
  (floatp (1+ 5.0))
  (integerp (1- 5))
  (floatp (1- 5.0))
  ;; Chain
  (1+ (1+ (1+ (1+ (1+ 0)))))
  (1- (1- (1- (1- (1- 10))))))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 0 43 -41 4.140000000000001 -2.14 1.0 2305843009213693952 t -1 0 -2 41 -43 2.14 -4.140000000000001 -1.0 -2305843009213693953 t t t t t 5 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// % and mod: comprehensive difference testing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_percent_mod_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((test-pairs '((10 3) (-10 3) (10 -3) (-10 -3)
                                (7 2) (-7 2) (7 -2) (-7 -2)
                                (1 1) (-1 1) (1 -1) (-1 -1)
                                (0 5) (0 -5)
                                (13 5) (-13 5) (13 -5) (-13 -5)
                                (100 7) (-100 7) (100 -7) (-100 -7)
                                (1 1000) (-1 1000)
                                (999 1000) (-999 1000))))
  (list
    ;; For each pair, show (n d %result mod-result %-identity mod-identity)
    (mapcar (lambda (pair)
              (let ((n (car pair)) (d (cadr pair)))
                (list n d
                      (% n d)
                      (mod n d)
                      ;; Identity: n = (truncate n d) * d + (% n d)
                      (= n (+ (* (truncate n d) d) (% n d)))
                      ;; Identity: n = (floor n d) * d + (mod n d)
                      (= n (+ (* (floor n d) d) (mod n d))))))
            test-pairs)
    ;; Float mod
    (mod 7.5 3.0)
    (mod -7.5 3.0)
    (mod 7.5 -3.0)
    (mod -7.5 -3.0)
    ;; Key difference
    (% -7 3)
    (mod -7 3)
    (% 7 -3)
    (mod 7 -3)))"#;
    let expect = expect_test::expect![[
        r#""OK (((10 3 1 1 t t) (-10 3 -1 2 t t) (10 -3 1 -2 t t) (-10 -3 -1 -1 t t) (7 2 1 1 t t) (-7 2 -1 1 t t) (7 -2 1 -1 t t) (-7 -2 -1 -1 t t) (1 1 0 0 t t) (-1 1 0 0 t t) (1 -1 0 0 t t) (-1 -1 0 0 t t) (0 5 0 0 t t) (0 -5 0 0 t t) (13 5 3 3 t t) (-13 5 -3 2 t t) (13 -5 3 -2 t t) (-13 -5 -3 -3 t t) (100 7 2 2 t t) (-100 7 -2 5 t t) (100 -7 2 -5 t t) (-100 -7 -2 -2 t t) (1 1000 1 1 t t) (-1 1000 -1 999 t t) (999 1000 999 999 t t) (-999 1000 -999 1 t t)) 1.5 1.5 -1.5 -1.5 -1 2 1 -2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ash: comprehensive arithmetic shift in all directions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_ash_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Left shift positive
  (ash 1 0) (ash 1 1) (ash 1 2) (ash 1 8) (ash 1 16) (ash 1 30)
  (ash 5 3)
  (ash 255 4)
  (ash 42 10)
  ;; Right shift positive
  (ash 256 -1) (ash 256 -4) (ash 256 -8) (ash 256 -9) (ash 256 -100)
  (ash 1 -1)
  (ash 1023 -3)
  ;; Left shift negative (arithmetic: preserves sign)
  (ash -1 0) (ash -1 1) (ash -1 10)
  (ash -5 3) (ash -42 4)
  ;; Right shift negative (arithmetic: sign extension)
  (ash -1 -1) (ash -1 -100)
  (ash -256 -4) (ash -256 -8) (ash -256 -9)
  (ash -100 -1)
  (ash -7 -1)    ;; -4 (rounds toward -infinity)
  (ash -3 -1)    ;; -2
  (ash -2 -1)    ;; -1
  ;; Shift of zero
  (ash 0 0) (ash 0 100) (ash 0 -100)
  ;; Large shifts producing bignums
  (ash 1 40) (ash 1 50) (ash 1 60)
  (> (ash 1 60) (ash 1 50))
  ;; Verify: ash left then right recovers value
  (= (ash (ash 42 10) -10) 42)
  (= (ash (ash 1 20) -20) 1)
  (= (ash (ash 12345 15) -15) 12345)
  ;; Verify: ash 1 n == 2^n
  (= (ash 1 0) 1)
  (= (ash 1 10) 1024)
  (= (ash 1 20) 1048576))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 4 256 65536 1073741824 40 4080 43008 128 16 1 0 0 0 127 -1 -2 -1024 -40 -672 -1 -1 -16 -1 -1 -50 -4 -2 -1 0 0 0 0 0 0 t t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// logand, logior, logxor, lognot with large/negative values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_bitwise_large_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; logand: multi-arg, negative, large
  (logand)
  (logand 255)
  (logand #xff #x0f)
  (logand #xABCD #xFF00)
  (logand -1 #xff)
  (logand 0 #xFFFFFFFF)
  (logand #xFF #x0F #x03)
  (logand -1 -1)
  (logand most-positive-fixnum most-positive-fixnum)
  (logand most-positive-fixnum most-negative-fixnum)
  ;; logior: multi-arg, negative
  (logior)
  (logior 255)
  (logior #xF0 #x0F)
  (logior 0 0)
  (logior #x100 #x010 #x001)
  (logior -1 0)
  (logior -1 42)
  (logior most-positive-fixnum 0)
  (logior 0 most-negative-fixnum)
  ;; logxor: multi-arg, self-inverse
  (logxor)
  (logxor 255)
  (logxor #xFF #xFF)
  (logxor #xFF 0)
  (logxor #xAA #x55)
  (logxor (logxor 12345 6789) 6789)
  (logxor 1 2 4 8)
  (logxor -1 0)
  (logxor -1 -1)
  (logxor most-positive-fixnum most-positive-fixnum)
  ;; lognot: complement
  (lognot 0) (lognot -1) (lognot 1) (lognot -2)
  (lognot #xFF)
  (lognot most-positive-fixnum)
  (lognot most-negative-fixnum)
  (= (lognot (lognot 42)) 42)
  (= (lognot (lognot most-positive-fixnum)) most-positive-fixnum)
  ;; De Morgan's
  (let ((a #xABCD) (b #x1234))
    (list
      (= (lognot (logand a b)) (logior (lognot a) (lognot b)))
      (= (lognot (logior a b)) (logand (lognot a) (lognot b)))))
  ;; logcount
  (logcount 0) (logcount 1) (logcount 7) (logcount 255)
  (logcount -1) (logcount -2) (logcount -256)
  (logcount 1023))"#;
    let expect = expect_test::expect![[
        r#""OK (-1 255 15 43776 255 0 3 -1 0 0 0 255 255 0 273 -1 -1 0 0 0 255 0 255 255 12345 15 -1 0 0 -1 0 -2 1 -256 0 0 t t (t t) 0 1 3 8 0 1 8 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// truncate/floor/ceiling/round: 1 arg and 2 arg, negative divisor, float
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_rounding_one_arg_all_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((vals '(0.0 0.1 0.5 0.9 1.0 1.1 1.5 1.9
                         2.0 2.3 2.5 2.7 3.0 3.5 4.5 5.5
                         -0.1 -0.5 -0.9 -1.0 -1.5 -2.0
                         -2.3 -2.5 -2.7 -3.5 -4.5 -5.5
                         100.5 -100.5 999.999 -999.999)))
  (mapcar (lambda (v)
            (list v
                  (truncate v)
                  (floor v)
                  (ceiling v)
                  (round v)))
          vals))"#;
    let expect = expect_test::expect![[
        r#""OK ((0.0 0 0 0 0) (0.1 0 0 1 0) (0.5 0 0 1 0) (0.9 0 0 1 1) (1.0 1 1 1 1) (1.1 1 1 2 1) (1.5 1 1 2 2) (1.9 1 1 2 2) (2.0 2 2 2 2) (2.3 2 2 3 2) (2.5 2 2 3 2) (2.7 2 2 3 3) (3.0 3 3 3 3) (3.5 3 3 4 4) (4.5 4 4 5 4) (5.5 5 5 6 6) (-0.1 0 -1 0 0) (-0.5 0 -1 0 0) (-0.9 0 -1 0 -1) (-1.0 -1 -1 -1 -1) (-1.5 -1 -2 -1 -2) (-2.0 -2 -2 -2 -2) (-2.3 -2 -3 -2 -2) (-2.5 -2 -3 -2 -2) (-2.7 -2 -3 -2 -3) (-3.5 -3 -4 -3 -4) (-4.5 -4 -5 -4 -4) (-5.5 -5 -6 -5 -6) (100.5 100 100 101 100) (-100.5 -100 -101 -100 -100) (999.999 999 999 1000 1000) (-999.999 -999 -1000 -999 -1000))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_number_arith_comp_rounding_two_arg_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((pairs '((10 3) (-10 3) (10 -3) (-10 -3)
                          (7 2) (-7 2) (7 -2) (-7 -2)
                          (1 2) (-1 2) (1 -2) (-1 -2)
                          (0 5) (15 5) (-15 5) (15 -5)
                          (17 7) (-17 7) (17 -7) (-17 -7)
                          (100 3) (-100 3) (100 -3)
                          (5 2) (7 2) (9 2) (11 2)
                          (-5 2) (-7 2) (-9 2) (-11 2))))
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
        r#""OK ((10 3 3 3 4 3) (-10 3 -3 -4 -3 -3) (10 -3 -3 -4 -3 -3) (-10 -3 3 3 4 3) (7 2 3 3 4 4) (-7 2 -3 -4 -3 -4) (7 -2 -3 -4 -3 -4) (-7 -2 3 3 4 4) (1 2 0 0 1 0) (-1 2 0 -1 0 0) (1 -2 0 -1 0 0) (-1 -2 0 0 1 0) (0 5 0 0 0 0) (15 5 3 3 3 3) (-15 5 -3 -3 -3 -3) (15 -5 -3 -3 -3 -3) (17 7 2 2 3 2) (-17 7 -2 -3 -2 -2) (17 -7 -2 -3 -2 -2) (-17 -7 2 2 3 2) (100 3 33 33 34 33) (-100 3 -33 -34 -33 -33) (100 -3 -33 -34 -33 -33) (5 2 2 2 3 2) (7 2 3 3 4 4) (9 2 4 4 5 4) (11 2 5 5 6 6) (-5 2 -2 -3 -2 -2) (-7 2 -3 -4 -3 -4) (-9 2 -4 -5 -4 -4) (-11 2 -5 -6 -5 -6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_number_arith_comp_rounding_float_divisor_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Float / float
  (truncate 7.5 2.0) (floor 7.5 2.0) (ceiling 7.5 2.0) (round 7.5 2.0)
  (truncate -7.5 2.0) (floor -7.5 2.0) (ceiling -7.5 2.0) (round -7.5 2.0)
  (truncate 7.5 -2.0) (floor 7.5 -2.0) (ceiling 7.5 -2.0) (round 7.5 -2.0)
  (truncate -7.5 -2.0) (floor -7.5 -2.0) (ceiling -7.5 -2.0) (round -7.5 -2.0)
  ;; Integer / float
  (truncate 10 3.0) (floor 10 3.0) (ceiling 10 3.0) (round 10 3.0)
  (truncate -10 3.0) (floor -10 3.0) (ceiling -10 3.0) (round -10 3.0)
  ;; Float / integer
  (truncate 10.5 3) (floor 10.5 3) (ceiling 10.5 3) (round 10.5 3)
  (truncate -10.5 3) (floor -10.5 3) (ceiling -10.5 3) (round -10.5 3)
  ;; Banker's rounding with divisor
  (round 5 2) (round 7 2) (round 9 2) (round 11 2) (round 13 2)
  (round -5 2) (round -7 2) (round -9 2) (round -11 2) (round -13 2)
  ;; Rounding with integer that divides evenly
  (truncate 12 4) (floor 12 4) (ceiling 12 4) (round 12 4)
  (truncate -12 4) (floor -12 4) (ceiling -12 4) (round -12 4))"#;
    let expect = expect_test::expect![[
        r#""OK (3 3 4 4 -3 -4 -3 -4 -3 -4 -3 -4 3 3 4 4 3 3 4 3 -3 -4 -3 -3 3 3 4 4 -3 -4 -3 -4 2 4 4 6 6 -2 -4 -4 -6 -6 3 3 3 3 -3 -3 -3 -3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// expt: negative, zero, float bases and exponents
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_expt_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Integer base, integer exponent
  (expt 2 0) (expt 2 1) (expt 2 10) (expt 2 20)
  (expt 3 0) (expt 3 5) (expt 3 10)
  (expt -2 0) (expt -2 1) (expt -2 2) (expt -2 3) (expt -2 4)
  (expt -3 3) (expt -3 4)
  (expt 0 0) (expt 0 1) (expt 0 10)
  (expt 1 0) (expt 1 1) (expt 1 100)
  (expt -1 0) (expt -1 1) (expt -1 2) (expt -1 99) (expt -1 100)
  (expt 10 0) (expt 10 1) (expt 10 5)
  ;; Float base
  (expt 2.0 10) (expt 2.0 -1) (expt 2.0 -10)
  (expt 0.5 2) (expt 0.5 -1) (expt 0.5 -2)
  ;; Float exponent
  (expt 2 10.0) (expt 4.0 0.5) (expt 8.0 (/ 1.0 3.0))
  (expt 9.0 0.5) (expt 16.0 0.25) (expt 100.0 0.5)
  ;; Negative integer exponent: integer truncation for int base
  (expt 2 -1) (expt 3 -1) (expt 10 -1) (expt 10 -2)
  ;; Float base, negative exponent
  (expt 2.0 -1) (expt 10.0 -2) (expt 3.0 -3)
  ;; Verify identity: x^a * x^(-a) ~= 1
  (< (abs (- (* (expt 2.0 7) (expt 2.0 -7)) 1.0)) 1e-10)
  (< (abs (- (* (expt 5.0 4) (expt 5.0 -4)) 1.0)) 1e-10)
  ;; Large exponent
  (expt 2 30)
  (> (expt 2 40) (expt 2 30)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 1024 1048576 1 243 59049 1 -2 4 -8 16 -27 81 1 0 0 1 1 1 1 -1 1 -1 1 1 10 100000 1024.0 0.5 0.0009765625 0.25 2.0 4.0 1024.0 2.0 2.0 3.0 2.0 10.0 0.5 0.3333333333333333 0.1 0.01 0.5 0.01 0.037037037037037035 t t 1073741824 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fixnum boundary: multi-operation chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_fixnum_boundary_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((mpf most-positive-fixnum)
                        (mnf most-negative-fixnum))
  (list
    ;; Overflow and recover
    (- (+ mpf 1) 1)
    (= (- (+ mpf 1) 1) mpf)
    (+ (- mnf 1) 1)
    (= (+ (- mnf 1) 1) mnf)
    ;; Double overflow
    (+ mpf mpf)
    (= (/ (+ mpf mpf) 2) mpf)
    ;; Multiplication overflow and back
    (* mpf 3)
    (= (/ (* mpf 3) 3) mpf)
    ;; Negation at boundaries
    (- mpf)
    (- mnf)
    (+ (- mpf) mpf)
    (+ (- mnf) mnf)
    ;; abs at boundaries
    (abs mpf)
    (abs mnf)
    (> (abs mnf) 0)
    ;; Comparison chains
    (< mnf 0 mpf)
    (<= mnf mnf)
    (>= mpf mpf)
    ;; Mixed fixnum boundary with float
    (+ mpf 0.0)
    (floatp (+ mpf 0.0))
    (+ mnf 0.0)
    (floatp (+ mnf 0.0))
    ;; Multiplication by -1
    (* -1 mpf)
    (* -1 mnf)
    (= (* -1 (* -1 mpf)) mpf)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 t 0 t 4611686018427387902 t 6917529027641081853 t 0 2305843009213693952 0 0 0 2305843009213693952 t t t t 2.305843009213694e+18 t -2.305843009213694e+18 t 0 2305843009213693952 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mixed int/float promotion: type tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_type_promotion_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Int op int -> int
  (integerp (+ 3 4))
  (integerp (- 10 3))
  (integerp (* 6 7))
  (integerp (/ 10 2))
  (integerp (% 10 3))
  (integerp (mod 10 3))
  ;; Float contaminates: one float operand -> float result
  (floatp (+ 3 4.0))
  (floatp (+ 3.0 4))
  (floatp (- 10 3.0))
  (floatp (* 6 7.0))
  (floatp (/ 10 2.0))
  ;; Multi-arg: single float contaminates entire chain
  (floatp (+ 1 2 3 4.0 5))
  (integerp (+ 1 2 3 4 5))
  (floatp (* 1 2 3 4.0))
  (integerp (* 1 2 3 4))
  ;; 1+ and 1- preserve type
  (integerp (1+ 5))
  (floatp (1+ 5.0))
  (integerp (1- 5))
  (floatp (1- 5.0))
  ;; Explicit float conversion
  (floatp (float 42))
  (= (float 42) 42.0)
  ;; Round-trip: int -> float -> truncate -> int
  (integerp (truncate (float 42)))
  (= (truncate (float 42)) 42)
  (integerp (floor (float -7)))
  (= (floor (float -7)) -7))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combined number theory using all arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_combined_number_theory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Primality test using mod and sqrt approximation
  (fset 'neovm--nac-prime-p
    (lambda (n)
      (if (< n 2) nil
        (let ((limit (1+ (truncate (sqrt (float n)))))
              (d 2)
              (prime t))
          (while (and prime (<= d limit))
            (when (= (mod n d) 0)
              (setq prime nil))
            (setq d (1+ d)))
          prime))))

  ;; Sieve of Eratosthenes up to n using a vector as boolean array
  (fset 'neovm--nac-sieve
    (lambda (n)
      (let ((is-prime (make-vector (1+ n) t))
            (primes nil))
        (aset is-prime 0 nil)
        (when (> n 0) (aset is-prime 1 nil))
        (let ((i 2))
          (while (<= i n)
            (when (aref is-prime i)
              (push i primes)
              (let ((j (* i i)))
                (while (<= j n)
                  (aset is-prime j nil)
                  (setq j (+ j i)))))
            (setq i (1+ i))))
        (nreverse primes))))

  ;; Euler's totient function
  (fset 'neovm--nac-totient
    (lambda (n)
      (let ((result 0))
        (dotimes (i n result)
          (let ((k (1+ i)))
            (when (= 1 (let ((a k) (b n))
                          (while (> b 0)
                            (let ((temp b))
                              (setq b (mod a b))
                              (setq a temp)))
                          a))
              (setq result (1+ result))))))))

  (unwind-protect
      (list
        ;; Prime check
        (funcall 'neovm--nac-prime-p 2)
        (funcall 'neovm--nac-prime-p 3)
        (funcall 'neovm--nac-prime-p 4)
        (funcall 'neovm--nac-prime-p 17)
        (funcall 'neovm--nac-prime-p 19)
        (funcall 'neovm--nac-prime-p 20)
        (funcall 'neovm--nac-prime-p 97)
        ;; Sieve
        (funcall 'neovm--nac-sieve 30)
        ;; Totient
        (funcall 'neovm--nac-totient 1)
        (funcall 'neovm--nac-totient 6)
        (funcall 'neovm--nac-totient 10)
        (funcall 'neovm--nac-totient 12)
        ;; Verify: totient(p) = p-1 for prime p
        (= (funcall 'neovm--nac-totient 7) 6)
        (= (funcall 'neovm--nac-totient 13) 12)
        ;; Combined: fast modular exponentiation
        (let ((base 3) (exp 100) (m 97))
          (let ((result 1) (b (mod base m)) (e exp))
            (while (> e 0)
              (when (= (logand e 1) 1)
                (setq result (mod (* result b) m)))
              (setq e (ash e -1))
              (setq b (mod (* b b) m)))
            result)))
    (fmakunbound 'neovm--nac-prime-p)
    (fmakunbound 'neovm--nac-sieve)
    (fmakunbound 'neovm--nac-totient)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil t nil t t nil t (2 3 5 7 11 13 17 19 23 29) 1 2 4 4 t t 81)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ffloor, fceiling, fround, ftruncate (float-returning rounding)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_float_rounding_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((vals '(0.0 0.5 1.0 1.5 2.0 2.5 3.0 3.5
                         -0.5 -1.0 -1.5 -2.0 -2.5 -3.0 -3.5
                         0.1 0.9 -0.1 -0.9 100.5 -100.5)))
  (list
    (mapcar (lambda (v)
              (list v
                    (ffloor v)
                    (fceiling v)
                    (fround v)
                    (ftruncate v)))
            vals)
    ;; Verify all return floats
    (floatp (ffloor 3.5))
    (floatp (fceiling 3.5))
    (floatp (fround 3.5))
    (floatp (ftruncate 3.5))
    ;; Banker's rounding: fround half-to-even
    (fround 0.5)  ;; 0.0
    (fround 1.5)  ;; 2.0
    (fround 2.5)  ;; 2.0
    (fround 3.5)  ;; 4.0
    (fround -0.5) ;; -0.0 or 0.0
    (fround -1.5) ;; -2.0
    (fround -2.5) ;; -2.0
    (fround -3.5) ;; -4.0
    ;; Integer input to float rounding
    (ffloor 5)
    (fceiling 5)
    (fround 5)
    (ftruncate 5)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument floatp 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// abs, min, max combined with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_arith_comp_abs_min_max_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; abs
  (abs 0) (abs 1) (abs -1) (abs 42) (abs -42)
  (abs 0.0) (abs 3.14) (abs -3.14)
  (abs most-positive-fixnum)
  (abs most-negative-fixnum)
  ;; Type preservation
  (integerp (abs -5))
  (floatp (abs -5.0))
  ;; min: 2, many args, mixed types
  (min 3 5)
  (min 5 3)
  (min -5 5)
  (min 1 2 3 4 5)
  (min 5 4 3 2 1)
  (min 3 1 4 1 5 9 2 6)
  (min 3.0 5)
  (min 3 5.0)
  (min most-positive-fixnum most-negative-fixnum)
  ;; max: 2, many args, mixed types
  (max 3 5)
  (max 5 3)
  (max -5 5)
  (max 1 2 3 4 5)
  (max 5 4 3 2 1)
  (max 3 1 4 1 5 9 2 6)
  (max 3.0 5)
  (max 3 5.0)
  (max most-positive-fixnum most-negative-fixnum)
  ;; Combined: clamp value to range
  (let ((clamp (lambda (val lo hi) (min hi (max lo val)))))
    (list
      (funcall clamp 5 0 10)
      (funcall clamp -5 0 10)
      (funcall clamp 15 0 10)
      (funcall clamp 0 0 10)
      (funcall clamp 10 0 10))))"#;
    let expect = expect_test::expect![[
        r#""OK (0 1 1 42 42 0.0 3.14 3.14 0 2305843009213693952 t t 3 3 -5 1 1 1 3.0 3 0 5 5 5 5 5 9 5 5.0 0 (5 0 10 0 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
