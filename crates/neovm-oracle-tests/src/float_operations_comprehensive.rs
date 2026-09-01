//! Comprehensive oracle parity tests for floating-point operations:
//! `float`, `truncate`, `floor`, `ceiling`, `round`, `ffloor`, `fceiling`,
//! `fround`, `ftruncate`, `isnan`, `frexp`, `ldexp`, `copysign`, `logb`,
//! special values (infinity, NaN, negative zero), and arithmetic edge cases.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// float coercion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_float_coercion_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0.0""#]];
    // Integer to float
    crate::common::assert_oracle_parity_expect("(float 0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0""#]];
    crate::common::assert_oracle_parity_expect("(float 1)", expect);
    let expect = expect_test::expect![[r#""OK -1.0""#]];
    crate::common::assert_oracle_parity_expect("(float -1)", expect);
    let expect = expect_test::expect![[r#""OK 42.0""#]];
    crate::common::assert_oracle_parity_expect("(float 42)", expect);
    let expect = expect_test::expect![[r#""OK 2.305843009213694e+18""#]];
    crate::common::assert_oracle_parity_expect("(float most-positive-fixnum)", expect);
    let expect = expect_test::expect![[r#""OK -2.305843009213694e+18""#]];
    crate::common::assert_oracle_parity_expect("(float most-negative-fixnum)", expect);
    let expect = expect_test::expect![[r#""OK 3.14""#]];
    // Float to float (idempotent)
    crate::common::assert_oracle_parity_expect("(float 3.14)", expect);
    let expect = expect_test::expect![[r#""OK -0.0""#]];
    crate::common::assert_oracle_parity_expect("(float -0.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(float 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK -1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(float -1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    // Verify type
    crate::common::assert_oracle_parity_expect("(floatp (float 7))", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(integerp (float 7))", expect);
}

// ---------------------------------------------------------------------------
// truncate with all parameter variations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_truncate_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    // Single argument: toward zero
    crate::common::assert_oracle_parity_expect("(truncate 2.7)", expect);
    let expect = expect_test::expect![[r#""OK -2""#]];
    crate::common::assert_oracle_parity_expect("(truncate -2.7)", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(truncate 2.3)", expect);
    let expect = expect_test::expect![[r#""OK -2""#]];
    crate::common::assert_oracle_parity_expect("(truncate -2.3)", expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(truncate 0.0)", expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(truncate -0.0)", expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(truncate 0.5)", expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(truncate -0.5)", expect);
    let expect = expect_test::expect![[r#""OK 10000000000""#]];
    crate::common::assert_oracle_parity_expect("(truncate 1.0e10)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    // Two-argument division + truncate
    crate::common::assert_oracle_parity_expect("(truncate 10 3)", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(truncate -10 3)", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(truncate 10 -3)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(truncate -10 -3)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(truncate 7.5 2.5)", expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(truncate 1 3)", expect);
    let expect = expect_test::expect![[r#""OK 5""#]];
    // Integer input (no-op)
    crate::common::assert_oracle_parity_expect("(truncate 5)", expect);
    let expect = expect_test::expect![[r#""OK -5""#]];
    crate::common::assert_oracle_parity_expect("(truncate -5)", expect);
}

// ---------------------------------------------------------------------------
// floor, ceiling, round — all parameter forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_floor_ceiling_round_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    // floor: toward negative infinity
    crate::common::assert_oracle_parity_expect("(floor 2.7)", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(floor -2.7)", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(floor 2.5)", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(floor -2.5)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(floor 10 3)", expect);
    let expect = expect_test::expect![[r#""OK -4""#]];
    crate::common::assert_oracle_parity_expect("(floor -10 3)", expect);
    let expect = expect_test::expect![[r#""OK -4""#]];
    crate::common::assert_oracle_parity_expect("(floor 10 -3)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(floor -10 -3)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(floor 7.0 2.0)", expect);

    let expect = expect_test::expect![[r#""OK 3""#]];
    // ceiling: toward positive infinity
    crate::common::assert_oracle_parity_expect("(ceiling 2.3)", expect);
    let expect = expect_test::expect![[r#""OK -2""#]];
    crate::common::assert_oracle_parity_expect("(ceiling -2.3)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(ceiling 2.5)", expect);
    let expect = expect_test::expect![[r#""OK -2""#]];
    crate::common::assert_oracle_parity_expect("(ceiling -2.5)", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(ceiling 10 3)", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(ceiling -10 3)", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(ceiling 10 -3)", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(ceiling -10 -3)", expect);

    let expect = expect_test::expect![[r#""OK 2""#]];
    // round: banker's rounding (to even)
    crate::common::assert_oracle_parity_expect("(round 2.5)", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(round 3.5)", expect);
    let expect = expect_test::expect![[r#""OK -2""#]];
    crate::common::assert_oracle_parity_expect("(round -2.5)", expect);
    let expect = expect_test::expect![[r#""OK -4""#]];
    crate::common::assert_oracle_parity_expect("(round -3.5)", expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(round 0.5)", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(round 1.5)", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(round 2.49999)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(round 10 3)", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(round -10 3)", expect);
}

// ---------------------------------------------------------------------------
// ffloor, fceiling, fround, ftruncate (return float, not int)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ffloor_fceiling_fround_ftruncate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2.0""#]];
    // ffloor
    crate::common::assert_oracle_parity_expect("(ffloor 2.7)", expect);
    let expect = expect_test::expect![[r#""OK -3.0""#]];
    crate::common::assert_oracle_parity_expect("(ffloor -2.7)", expect);
    let expect = expect_test::expect![[r#""OK 2.0""#]];
    crate::common::assert_oracle_parity_expect("(ffloor 2.0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(floatp (ffloor 2.7))", expect);

    let expect = expect_test::expect![[r#""OK 3.0""#]];
    // fceiling
    crate::common::assert_oracle_parity_expect("(fceiling 2.3)", expect);
    let expect = expect_test::expect![[r#""OK -2.0""#]];
    crate::common::assert_oracle_parity_expect("(fceiling -2.3)", expect);
    let expect = expect_test::expect![[r#""OK 2.0""#]];
    crate::common::assert_oracle_parity_expect("(fceiling 2.0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(floatp (fceiling 2.3))", expect);

    let expect = expect_test::expect![[r#""OK 2.0""#]];
    // fround
    crate::common::assert_oracle_parity_expect("(fround 2.5)", expect);
    let expect = expect_test::expect![[r#""OK 4.0""#]];
    crate::common::assert_oracle_parity_expect("(fround 3.5)", expect);
    let expect = expect_test::expect![[r#""OK -0.0""#]];
    crate::common::assert_oracle_parity_expect("(fround -0.5)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(floatp (fround 2.5))", expect);

    let expect = expect_test::expect![[r#""OK 2.0""#]];
    // ftruncate
    crate::common::assert_oracle_parity_expect("(ftruncate 2.7)", expect);
    let expect = expect_test::expect![[r#""OK -2.0""#]];
    crate::common::assert_oracle_parity_expect("(ftruncate -2.7)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(ftruncate 0.0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(floatp (ftruncate 2.7))", expect);
}

// ---------------------------------------------------------------------------
// isnan
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_isnan_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(isnan 0.0e+NaN)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(isnan 0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(isnan 1.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(isnan -0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(isnan 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(isnan -1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(isnan (/ 0.0 0.0))", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(isnan (- 1.0e+INF 1.0e+INF))", expect);
}

// ---------------------------------------------------------------------------
// frexp and ldexp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_frexp_ldexp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0.5 . 1)""#]];
    // frexp returns (significand . exponent) where 0.5 <= |sig| < 1.0
    crate::common::assert_oracle_parity_expect("(frexp 1.0)", expect);
    let expect = expect_test::expect![[r#""OK (0.5 . 2)""#]];
    crate::common::assert_oracle_parity_expect("(frexp 2.0)", expect);
    let expect = expect_test::expect![[r#""OK (0.5 . 0)""#]];
    crate::common::assert_oracle_parity_expect("(frexp 0.5)", expect);
    let expect = expect_test::expect![[r#""OK (-0.5 . 3)""#]];
    crate::common::assert_oracle_parity_expect("(frexp -4.0)", expect);
    let expect = expect_test::expect![[r#""OK (0.0 . 0)""#]];
    crate::common::assert_oracle_parity_expect("(frexp 0.0)", expect);
    let expect = expect_test::expect![[r#""OK (0.5 . 11)""#]];
    crate::common::assert_oracle_parity_expect("(frexp 1024.0)", expect);
    let expect = expect_test::expect![[r#""OK (0.5 . -2)""#]];
    crate::common::assert_oracle_parity_expect("(frexp 0.125)", expect);

    let expect = expect_test::expect![[r#""OK 1.0""#]];
    // ldexp: significand * 2^exponent
    crate::common::assert_oracle_parity_expect("(ldexp 0.5 1)", expect);
    let expect = expect_test::expect![[r#""OK 2.0""#]];
    crate::common::assert_oracle_parity_expect("(ldexp 0.5 2)", expect);
    let expect = expect_test::expect![[r#""OK 768.0""#]];
    crate::common::assert_oracle_parity_expect("(ldexp 0.75 10)", expect);
    let expect = expect_test::expect![[r#""OK 1.0""#]];
    crate::common::assert_oracle_parity_expect("(ldexp 1.0 0)", expect);
    let expect = expect_test::expect![[r#""OK -4.0""#]];
    crate::common::assert_oracle_parity_expect("(ldexp -0.5 3)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(ldexp 0.0 100)", expect);

    // Round-trip: (ldexp (car (frexp x)) (cdr (frexp x))) == x
    let form = r#"(let* ((x 42.5)
                          (fr (frexp x)))
                     (= (ldexp (car fr) (cdr fr)) x))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copysign
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copysign_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK -1.0""#]];
    crate::common::assert_oracle_parity_expect("(copysign 1.0 -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0""#]];
    crate::common::assert_oracle_parity_expect("(copysign 1.0 1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0""#]];
    crate::common::assert_oracle_parity_expect("(copysign -1.0 1.0)", expect);
    let expect = expect_test::expect![[r#""OK -1.0""#]];
    crate::common::assert_oracle_parity_expect("(copysign -1.0 -1.0)", expect);
    let expect = expect_test::expect![[r#""OK -0.0""#]];
    crate::common::assert_oracle_parity_expect("(copysign 0.0 -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(copysign 0.0 1.0)", expect);
    let expect = expect_test::expect![[r#""OK -3.14""#]];
    crate::common::assert_oracle_parity_expect("(copysign 3.14 -0.0)", expect);
    let expect = expect_test::expect![[r#""OK -1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(copysign 1.0e+INF -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(copysign 1.0e+INF 1.0)", expect);
}

// ---------------------------------------------------------------------------
// logb
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logb_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(logb 1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect("(logb 2.0)", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(logb 4.0)", expect);
    let expect = expect_test::expect![[r#""OK -1""#]];
    crate::common::assert_oracle_parity_expect("(logb 0.5)", expect);
    let expect = expect_test::expect![[r#""OK -2""#]];
    crate::common::assert_oracle_parity_expect("(logb 0.25)", expect);
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect("(logb 1024.0)", expect);
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect("(logb 3.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(logb 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(logb 10)", expect);
}

// ---------------------------------------------------------------------------
// Special values: infinity, NaN, negative zero
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_float_special_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1.0e+INF""#]];
    // Infinity arithmetic
    crate::common::assert_oracle_parity_expect("(+ 1.0e+INF 1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(+ 1.0e+INF 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK -0.0e+NaN""#]];
    crate::common::assert_oracle_parity_expect("(- 1.0e+INF 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK 1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(* 1.0e+INF 2.0)", expect);
    let expect = expect_test::expect![[r#""OK -0.0e+NaN""#]];
    crate::common::assert_oracle_parity_expect("(* 1.0e+INF 0.0)", expect);
    let expect = expect_test::expect![[r#""OK -1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(* 1.0e+INF -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(/ 1.0 0.0)", expect);
    let expect = expect_test::expect![[r#""OK -1.0e+INF""#]];
    crate::common::assert_oracle_parity_expect("(/ -1.0 0.0)", expect);
    let expect = expect_test::expect![[r#""OK -0.0e+NaN""#]];
    crate::common::assert_oracle_parity_expect("(/ 0.0 0.0)", expect);

    let expect = expect_test::expect![[r#""OK 0.0e+NaN""#]];
    // NaN propagation
    crate::common::assert_oracle_parity_expect("(+ 0.0e+NaN 1.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.0e+NaN""#]];
    crate::common::assert_oracle_parity_expect("(* 0.0e+NaN 0.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.0e+NaN""#]];
    crate::common::assert_oracle_parity_expect("(- 0.0e+NaN 0.0e+NaN)", expect);

    let expect = expect_test::expect![[r#""OK 0.0""#]];
    // Negative zero
    crate::common::assert_oracle_parity_expect("(+ 0.0 -0.0)", expect);
    let expect = expect_test::expect![[r#""OK -0.0""#]];
    crate::common::assert_oracle_parity_expect("(- 0.0)", expect);
    let expect = expect_test::expect![[r#""OK -0.0""#]];
    crate::common::assert_oracle_parity_expect("(* -1.0 0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(eql 0.0 -0.0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(= 0.0 -0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(equal 0.0 -0.0)", expect);
}

// ---------------------------------------------------------------------------
// Comparison edge cases with floats
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_float_comparison_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // NaN comparisons: NaN is not equal to anything, not even itself
    crate::common::assert_oracle_parity_expect("(= 0.0e+NaN 0.0e+NaN)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(< 0.0e+NaN 0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(> 0.0e+NaN 0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(<= 0.0e+NaN 0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(>= 0.0e+NaN 0.0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(/= 0.0e+NaN 0.0e+NaN)", expect);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Infinity comparisons
    crate::common::assert_oracle_parity_expect("(< 1.0e+INF 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(<= 1.0e+INF 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(> -1.0e+INF 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(< -1.0e+INF 1.0e+INF)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(= 1.0e+INF 1.0e+INF)", expect);

    let expect = expect_test::expect![[r#""OK t""#]];
    // Mixed int/float comparisons
    crate::common::assert_oracle_parity_expect("(= 1 1.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(eql 1 1.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(equal 1 1.0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(< 1 1.0000000000001)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(> most-positive-fixnum (float most-positive-fixnum))",
        expect,
    );
}

// ---------------------------------------------------------------------------
// Chained float rounding combined with arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_float_chained_rounding_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 5""#]];
    // Combine rounding modes in expressions
    crate::common::assert_oracle_parity_expect("(+ (floor 2.7) (ceiling 2.3))", expect);
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect("(- (round 3.5) (truncate -2.7))", expect);
    let expect = expect_test::expect![[r#""OK 6.0""#]];
    crate::common::assert_oracle_parity_expect("(* (ffloor 2.7) (fceiling 2.3))", expect);
    let expect = expect_test::expect![[r#""OK 3.3333333333333335""#]];
    crate::common::assert_oracle_parity_expect("(/ (fround 10.0) (ftruncate 3.7))", expect);

    let expect = expect_test::expect![[r#""OK 3""#]];
    // Nested rounding
    crate::common::assert_oracle_parity_expect("(floor (ceiling 2.3))", expect);
    let expect = expect_test::expect![[r#""OK -3""#]];
    crate::common::assert_oracle_parity_expect("(round (floor -2.7))", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(truncate (fround 3.5))", expect);

    // Complex expression with type mixing
    let form = r#"(let* ((a 2.7)
                          (b -3.2)
                          (f (floor a))
                          (c (ceiling b))
                          (r (round (+ a b)))
                          (t2 (truncate (* a b))))
                     (list f c r t2
                           (floatp (ffloor a))
                           (integerp (floor a))))"#;
    let expect = expect_test::expect![[r#""OK (2 -3 0 -8 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Two-argument division forms: comprehensive remainder behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_float_two_arg_division_remainder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Emacs (floor x y) returns quotient; remainder via mod/% semantics
    // Verify quotient * divisor + remainder == dividend
    let form = r#"(let* ((a 17) (b 5)
                          (q-floor (floor a b))
                          (q-ceil (ceiling a b))
                          (q-round (round a b))
                          (q-trunc (truncate a b)))
                     (list q-floor q-ceil q-round q-trunc
                           (- a (* q-floor b))
                           (- a (* q-ceil b))
                           (- a (* q-round b))
                           (- a (* q-trunc b))))"#;
    let expect = expect_test::expect![[r#""OK (3 4 3 3 2 -3 2 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Negative dividend
    let form2 = r#"(let* ((a -17) (b 5))
                      (list (floor a b) (ceiling a b)
                            (round a b) (truncate a b)))"#;
    let expect = expect_test::expect![[r#""OK (-4 -3 -3 -3)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Float arguments
    let form3 = r#"(let* ((a 17.0) (b 3.0))
                      (list (floor a b) (ceiling a b)
                            (round a b) (truncate a b)))"#;
    let expect = expect_test::expect![[r#""OK (5 6 6 5)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    let expect = expect_test::expect![[r#""OK 3""#]];
    // Mixed int/float
    crate::common::assert_oracle_parity_expect("(floor 10 3.0)", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(ceiling 10.0 3)", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(round 7 2.0)", expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(truncate 7.0 2)", expect);
}
