//! Oracle parity tests for advanced arithmetic: mixed int/float promotion,
//! division-by-zero semantics, mod vs %, ash edge cases, combined bitwise ops,
//! expt edge cases, rounding functions, and near-fixnum-boundary arithmetic.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Mixed integer and float promotion rules
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_mixed_int_float_promotion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Integer + float promotes to float; verify exact results and types
    let form = r#"(let ((results nil))
                    ;; int + float -> float
                    (setq results (cons (+ 3 2.5) results))
                    ;; float + int -> float
                    (setq results (cons (+ 2.5 3) results))
                    ;; int * float -> float
                    (setq results (cons (* 4 0.5) results))
                    ;; int - float -> float
                    (setq results (cons (- 10 3.5) results))
                    ;; int / float -> float
                    (setq results (cons (/ 7 2.0) results))
                    ;; float / int -> float
                    (setq results (cons (/ 7.0 2) results))
                    ;; multi-arg: one float taints the whole chain
                    (setq results (cons (+ 1 2 3 4.0 5) results))
                    ;; type checks
                    (setq results (cons (floatp (+ 1 1.0)) results))
                    (setq results (cons (integerp (+ 1 1)) results))
                    (nreverse results))"#;
    let expect = expect_test::expect![r#""OK (5.5 5.5 2.0 6.5 3.5 3.5 15.0 t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_float_integer_division_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Integer division truncates toward zero; float division gives exact result
    let form = "(list
                  (/ 7 2)       ;; 3 (truncated)
                  (/ -7 2)      ;; -3 (truncated toward zero)
                  (/ 7 -2)      ;; -3
                  (/ -7 -2)     ;; 3
                  (/ 7 2.0)     ;; 3.5 (exact)
                  (/ -7 2.0)    ;; -3.5
                  (/ 7.0 2)     ;; 3.5
                  (/ 1 3)       ;; 0
                  (/ 1 3.0))    ;; 0.333...";
    let expect = expect_test::expect![r#""OK (3 -3 -3 3 3.5 -3.5 3.5 0 0.3333333333333333)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Division by zero: integer vs float
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_division_by_zero_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Integer division by zero signals arith-error
    let form = "(condition-case err
                  (/ 42 0)
                (arith-error (list 'caught (car err))))";
    let expect = expect_test::expect![[r#""OK (caught arith-error)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(caught arith-error)", &o, &n);
}

#[test]
fn oracle_prop_arith_adv_division_by_zero_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Float division by zero produces infinity, not an error
    // 1.0 / 0.0 -> 1.0e+INF; -1.0 / 0.0 -> -1.0e+INF
    let form = "(list
                  (/ 1.0 0.0)
                  (/ -1.0 0.0)
                  (> (/ 1.0 0.0) 0)
                  (< (/ -1.0 0.0) 0))";
    let expect = expect_test::expect![r#""OK (1.0e+INF -1.0e+INF t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_mod_by_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // mod by zero signals arith-error; % by zero also
    let form = "(list
                  (condition-case err (% 10 0) (arith-error 'pct-caught))
                  (condition-case err (mod 10 0) (arith-error 'mod-caught)))";
    let expect = expect_test::expect![[r#""OK (pct-caught mod-caught)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(pct-caught mod-caught)", &o, &n);
}

// ---------------------------------------------------------------------------
// mod vs % with negative operands
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_mod_vs_percent_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Key difference: % truncates toward zero, mod rounds toward -infinity
    // (% -7 3) = -1 but (mod -7 3) = 2
    // (% 7 -3) = 1 but (mod 7 -3) = -2
    let form = "(list
                  (% 7 3)       ;; 1
                  (mod 7 3)     ;; 1  (same when both positive)
                  (% -7 3)      ;; -1
                  (mod -7 3)    ;; 2  (different!)
                  (% 7 -3)      ;; 1
                  (mod 7 -3)    ;; -2 (different!)
                  (% -7 -3)     ;; -1
                  (mod -7 -3))  ;; -1 (same when both negative)";
    let expect = expect_test::expect![r#""OK (1 1 -1 2 1 -2 -1 -1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_mod_float_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // mod with floats follows same sign-of-divisor rule
    let form = "(list
                  (mod 7.5 3.0)
                  (mod -7.5 3.0)
                  (mod 7.5 -3.0)
                  (mod -7.5 -3.0)
                  (mod 10.0 3.0)
                  (mod -10.0 3.0))";
    let expect = expect_test::expect![r#""OK (1.5 1.5 -1.5 -1.5 1.0 2.0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ash (arithmetic shift) with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_ash_large_shifts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Large left shifts, negative value shifts, shift of zero
    let form = "(list
                  (ash 1 20)         ;; 1048576
                  (ash 1 30)         ;; 1073741824
                  (ash -1 10)        ;; -1024
                  (ash -1024 -10)    ;; -1
                  (ash 0 100)        ;; 0
                  (ash 0 -100)       ;; 0
                  ;; Right shift of negative preserves sign (arithmetic shift)
                  (ash -256 -4)      ;; -16
                  (ash -1 -1)        ;; -1 (arithmetic shift of -1 stays -1)
                  (ash -1 -100))     ;; -1";
    let expect = expect_test::expect![r#""OK (1048576 1073741824 -1024 -1 0 0 -16 -1 -1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_ash_power_of_two() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // ash 1 n == 2^n; verify with expt
    let form = "(let ((results nil))
                  (dolist (n '(0 1 2 3 4 5 8 10 16 20))
                    (setq results
                          (cons (= (ash 1 n) (expt 2 n))
                                results)))
                  (nreverse results))";
    let expect = expect_test::expect![r#""OK (t t t t t t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_bignum_results_remain_integers_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU data.c arithmetic promotes overflow and large shifts to GMP-backed
    // bignums; those results are still integers and must be accepted by
    // integer primitives such as `logcount` and `number-to-string`.
    let form = r#"
(list
 (+ 1000000000000000000000000000000 1)
 (* 1000000000000000000000000000000 2)
 (ash 1 100)
 (integerp (ash 1 100))
 (logcount (ash 1 100))
 (number-to-string (ash 1 100)))
"#;
    let expect = expect_test::expect![[
        r#""OK (1000000000000000000000000000001 2000000000000000000000000000000 1267650600228229401496703205376 t 1 \"1267650600228229401496703205376\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined bitwise operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_bitwise_combined_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex bitwise: implement a simple bit-field packer/unpacker
    let form = "(let ((pack (lambda (r g b)
                    ;; Pack RGB into single integer: R<<16 | G<<8 | B
                    (logior (ash r 16) (ash g 8) b)))
                  (unpack-r (lambda (rgb) (logand (ash rgb -16) #xff)))
                  (unpack-g (lambda (rgb) (logand (ash rgb -8) #xff)))
                  (unpack-b (lambda (rgb) (logand rgb #xff))))
              (let* ((color (funcall pack 200 128 64))
                     (r (funcall unpack-r color))
                     (g (funcall unpack-g color))
                     (b (funcall unpack-b color))
                     ;; Blend two colors: (c1 + c2) / 2
                     (c2 (funcall pack 100 200 50))
                     (blend-r (/ (+ (funcall unpack-r color) (funcall unpack-r c2)) 2))
                     (blend-g (/ (+ (funcall unpack-g color) (funcall unpack-g c2)) 2))
                     (blend-b (/ (+ (funcall unpack-b color) (funcall unpack-b c2)) 2)))
                (list r g b blend-r blend-g blend-b)))";
    let expect = expect_test::expect![r#""OK (200 128 64 150 164 57)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_bitwise_demorgan_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify De Morgan's laws in a chain and XOR properties
    let form = "(let ((a #xABCD) (b #x1234) (c #x5678))
                  (list
                    ;; De Morgan: NOT(a AND b) == (NOT a) OR (NOT b)
                    (= (lognot (logand a b))
                       (logior (lognot a) (lognot b)))
                    ;; De Morgan: NOT(a OR b) == (NOT a) AND (NOT b)
                    (= (lognot (logior a b))
                       (logand (lognot a) (lognot b)))
                    ;; XOR associativity: (a XOR b) XOR c == a XOR (b XOR c)
                    (= (logxor (logxor a b) c)
                       (logxor a (logxor b c)))
                    ;; XOR self-inverse: a XOR b XOR b == a
                    (= (logxor (logxor a b) b) a)
                    ;; NOT(NOT(a)) == a
                    (= (lognot (lognot a)) a)
                    ;; Absorption: a AND (a OR b) == a
                    (= (logand a (logior a b)) a)))";
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t t t t t t)", &o, &n);
}

// ---------------------------------------------------------------------------
// expt with float/integer combinations and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_expt_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list
                  ;; Integer base and exponent
                  (expt 2 10)       ;; 1024
                  (expt 3 5)        ;; 243
                  (expt -2 3)       ;; -8
                  (expt -2 4)       ;; 16 (even exponent)
                  ;; Zero exponent
                  (expt 0 0)        ;; 1
                  (expt 100 0)      ;; 1
                  (expt -5 0)       ;; 1
                  ;; Expt with float
                  (expt 2.0 10)     ;; 1024.0
                  (expt 2 10.0)     ;; 1024.0
                  (expt 4.0 0.5)    ;; 2.0 (square root)
                  (expt 27.0 (/ 1.0 3.0)))  ;; cube root of 27";
    let expect = expect_test::expect![r#""OK (1024 243 -8 16 1 1 1 1024.0 1024.0 2.0 3.0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_expt_negative_exponent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Negative exponents with integer base require float result
    let form = "(list
                  (expt 2 -1)     ;; 0 (integer truncation!)
                  (expt 2.0 -1)   ;; 0.5
                  (expt 10.0 -2)  ;; 0.01
                  (expt 2.0 -10)  ;; ~0.000976
                  ;; Verify: expt(x,n) * expt(x,-n) ≈ 1 for floats
                  (< (abs (- (* (expt 3.0 7) (expt 3.0 -7)) 1.0)) 1e-10))";
    let expect = expect_test::expect![r#""OK (0.5 0.5 0.01 0.0009765625 t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ceiling/floor/round/truncate on various float values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_rounding_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematic test of all four rounding modes on key values
    let form = "(let ((vals '(2.0 2.3 2.5 2.7 3.5 -2.0 -2.3 -2.5 -2.7 -3.5 0.0 0.5 -0.5)))
                  (mapcar (lambda (v)
                            (list v
                                  (floor v)
                                  (ceiling v)
                                  (round v)
                                  (truncate v)))
                          vals))";
    let expect = expect_test::expect![
        r#""OK ((2.0 2 2 2 2) (2.3 2 3 2 2) (2.5 2 3 2 2) (2.7 2 3 3 2) (3.5 3 4 4 3) (-2.0 -2 -2 -2 -2) (-2.3 -3 -2 -2 -2) (-2.5 -3 -2 -2 -2) (-2.7 -3 -2 -3 -2) (-3.5 -4 -3 -4 -3) (0.0 0 0 0 0) (0.5 0 1 0 0) (-0.5 -1 0 0 0))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_rounding_with_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // floor/ceiling/round/truncate with two-argument form (divisor)
    let form = "(list
                  ;; floor(n, d) = floor(n/d)
                  (floor 7 2)       ;; 3
                  (floor -7 2)      ;; -4
                  (floor 7 -2)      ;; -4
                  (floor -7 -2)     ;; 3
                  ;; ceiling(n, d)
                  (ceiling 7 2)     ;; 4
                  (ceiling -7 2)    ;; -3
                  (ceiling 7 -2)    ;; -3
                  (ceiling -7 -2)   ;; 4
                  ;; truncate(n, d)
                  (truncate 7 2)    ;; 3
                  (truncate -7 2)   ;; -3
                  (truncate 7 -2)   ;; -3
                  (truncate -7 -2)  ;; 3
                  ;; round(n, d)
                  (round 7 2)       ;; 4 (banker's rounding: 3.5 -> 4)
                  (round 9 2)       ;; 4 (4.5 -> 4, rounds to even)
                  (round -7 2))     ;; -4";
    let expect = expect_test::expect![r#""OK (3 -4 -4 3 4 -3 -3 4 3 -3 -3 3 4 4 -4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_bankers_rounding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Banker's rounding (round half to even) is Emacs's behavior
    let form = "(list
                  (round 0.5)   ;; 0 (even)
                  (round 1.5)   ;; 2 (even)
                  (round 2.5)   ;; 2 (even)
                  (round 3.5)   ;; 4 (even)
                  (round 4.5)   ;; 4 (even)
                  (round -0.5)  ;; 0 (even)
                  (round -1.5)  ;; -2 (even)
                  (round -2.5)  ;; -2 (even)
                  (round -3.5)  ;; -4 (even)
                  (round -4.5)) ;; -4 (even)";
    let expect = expect_test::expect![r#""OK (0 2 2 4 4 0 -2 -2 -4 -4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: near fixnum boundary arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_arith_adv_large_number_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Operations with large numbers near typical fixnum limits
    let form = "(let ((big 536870911)    ;; 2^29 - 1 (30-bit fixnum max)
                      (big2 1073741823)) ;; 2^30 - 1
                  (list
                    (+ big 1)
                    (- (- big) 1)
                    (* big 2)
                    (+ big big)
                    (+ big2 1)
                    (* big2 2)
                    ;; Verify arithmetic still works
                    (= (- (+ big 100) 100) big)
                    (= (/ (* big 7) 7) big)))";
    let expect = expect_test::expect![
        r#""OK (536870912 -536870912 1073741822 1073741822 1073741824 2147483646 t t)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_arith_adv_complex_expression_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex nested arithmetic expression combining many operations
    let form = "(let ((a 17) (b 5) (c 3) (d 2.0))
                  (list
                    ;; Nested: ((a mod b) * c + floor(a/b)) ^ 2
                    (expt (+ (* (mod a b) c) (/ a b)) 2)
                    ;; Mixed chain: float promotes midway
                    (+ (/ a b) (* (/ a d) c))
                    ;; Bitwise + arithmetic interleaved
                    (+ (logand a #xff) (ash (mod b c) 4) (lognot -100))
                    ;; Multi-op reduction
                    (apply '+ (mapcar (lambda (x) (* x x)) '(1 2 3 4 5)))
                    ;; Chained mod
                    (mod (mod (mod 1000 37) 7) 3)))";
    let expect = expect_test::expect![r#""OK (81 28.5 148 55 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
