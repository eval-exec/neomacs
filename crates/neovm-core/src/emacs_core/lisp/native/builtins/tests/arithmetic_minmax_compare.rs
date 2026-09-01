//! Tests for GNU-faithful `max`/`min` NaN propagation and exact
//! integer-vs-float comparison in `arithcompare`.
//!
//! Oracle values were produced with GNU Emacs:
//!   emacs --batch --eval '(prin1 ...)'

use crate::emacs_core::{Context, format_eval_result};

/// Evaluate `src` and return the printed result (mirrors `prin1`),
/// prefixed with `OK`/`ERR` by `format_eval_result`.
fn eval_one(src: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    format_eval_result(&ev.eval_str(src))
}

// -----------------------------------------------------------------------
// Bug A: max/min must propagate a NaN argument (not silently drop it),
// and once the NaN wins it is returned (so the result is a float NaN).
// -----------------------------------------------------------------------

#[test]
fn max_propagates_nan_in_middle() {
    // GNU: (max 1.0 (/ 0.0 0.0)) => -0.0e+NaN
    assert_eq!(eval_one("(max 1.0 (/ 0.0 0.0))"), "OK -0.0e+NaN");
}

#[test]
fn min_propagates_nan_in_middle() {
    // GNU: (min 1.0 2.0 (/ 0.0 0.0) 3.0) => -0.0e+NaN
    assert_eq!(eval_one("(min 1.0 2.0 (/ 0.0 0.0) 3.0)"), "OK -0.0e+NaN");
}

#[test]
fn max_with_integer_accumulator_and_nan_returns_float_nan() {
    // GNU: (max 5 (/ 0.0 0.0)) => -0.0e+NaN  (result is a float, not 5)
    assert_eq!(eval_one("(max 5 (/ 0.0 0.0))"), "OK -0.0e+NaN");
    // The returned value must be a float.
    assert_eq!(eval_one("(type-of (max 5 (/ 0.0 0.0)))"), "OK float");
}

#[test]
fn max_nan_first_still_propagates() {
    // GNU: (max (/ 0.0 0.0) 1.0) => -0.0e+NaN
    assert_eq!(eval_one("(max (/ 0.0 0.0) 1.0)"), "OK -0.0e+NaN");
}

#[test]
fn min_nan_first_still_propagates() {
    // GNU: (min (/ 0.0 0.0) 1.0) => -0.0e+NaN
    assert_eq!(eval_one("(min (/ 0.0 0.0) 1.0)"), "OK -0.0e+NaN");
}

#[test]
fn max_min_without_nan_return_unchanged_argument() {
    // No NaN present: the winning argument is returned verbatim, not
    // coerced to float just because a float was an argument.
    // GNU: (max 1 2.0 3) => 3 ; (min 1 2.0 3) => 1 ; (max 3 2 1) => 3
    assert_eq!(eval_one("(max 1 2.0 3)"), "OK 3");
    assert_eq!(eval_one("(min 1 2.0 3)"), "OK 1");
    assert_eq!(eval_one("(max 3 2 1)"), "OK 3");
    assert_eq!(eval_one("(min 1 2 3)"), "OK 1");
    // A float winner stays a float.
    assert_eq!(eval_one("(max 1 2.5 2)"), "OK 2.5");
}

// -----------------------------------------------------------------------
// Bug B: integer-vs-float comparison must be EXACT (no lossy coercion of
// the integer to a double before comparing).
// -----------------------------------------------------------------------

#[test]
fn eq_fixnum_vs_float_exact_beyond_2_pow_53() {
    // 2^53 + 1 is not representable as a double; float() rounds to 2^53.
    // GNU: (= (+ (expt 2 53) 1) (float (+ (expt 2 53) 1))) => nil
    assert_eq!(
        eval_one("(= (+ (expt 2 53) 1) (float (+ (expt 2 53) 1)))"),
        "OK nil"
    );
}

#[test]
fn gt_integer_vs_float_exact_beyond_2_pow_53() {
    // GNU: (> (1+ (expt 2 53)) 9007199254740992.0) => t
    assert_eq!(eval_one("(> (1+ (expt 2 53)) 9007199254740992.0)"), "OK t");
}

#[test]
fn eq_most_positive_fixnum_vs_its_float_is_nil() {
    // most-positive-fixnum (2^61-1) does not survive a float round-trip.
    // GNU: (= most-positive-fixnum (float most-positive-fixnum)) => nil
    assert_eq!(
        eval_one("(= most-positive-fixnum (float most-positive-fixnum))"),
        "OK nil"
    );
}

#[test]
fn small_integer_vs_float_still_compares_correctly() {
    // Sanity: ordinary in-range comparisons are unchanged.
    assert_eq!(eval_one("(= 2 2.0)"), "OK t");
    assert_eq!(eval_one("(< 2 2.5)"), "OK t");
    assert_eq!(eval_one("(> 3 2.5)"), "OK t");
    assert_eq!(eval_one("(<= 2 2.0)"), "OK t");
    assert_eq!(eval_one("(>= 2 2.0)"), "OK t");
    // NaN comparisons are all false except /=.
    assert_eq!(eval_one("(= 1 (/ 0.0 0.0))"), "OK nil");
    assert_eq!(eval_one("(/= 1 (/ 0.0 0.0))"), "OK t");
    assert_eq!(eval_one("(< 1 (/ 0.0 0.0))"), "OK nil");
}

#[test]
fn bignum_vs_float_comparison_stays_exact() {
    // The refactored arithcompare float-branch must keep handling
    // bignum-vs-float exactly (was already correct; guard the path).
    // GNU: (< (expt 10 30) 1.0e40) => t ; (> (expt 10 30) 1.0e20) => t
    assert_eq!(eval_one("(< (expt 10 30) 1.0e40)"), "OK t");
    assert_eq!(eval_one("(> (expt 10 30) 1.0e20)"), "OK t");
    // GNU: (= (expt 2 70) (expt 2.0 70)) => t
    assert_eq!(eval_one("(= (expt 2 70) (expt 2.0 70))"), "OK t");
    // max/min over bignum + float still pick the right value.
    // GNU: (max 1.0e40 (expt 10 30)) => 1e+40 ; (min (expt 10 30) 5) => 5
    assert_eq!(eval_one("(max 1.0e40 (expt 10 30))"), "OK 1e+40");
    assert_eq!(eval_one("(min (expt 10 30) 5)"), "OK 5");
}
