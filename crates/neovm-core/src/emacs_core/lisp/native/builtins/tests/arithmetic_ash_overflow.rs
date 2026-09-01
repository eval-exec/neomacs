//! Tests for GNU-faithful overflow handling in `ash` (and therefore the
//! `lsh` elisp wrapper) when COUNT is a large *fixnum*.
//!
//! GNU's `Fash` (src/data.c:3556) places its `value == 0` short-circuit
//! ONLY in the non-fixnum (bignum) COUNT branch. For a *fixnum* COUNT it
//! falls through to `emacs_mpz_mul_2exp` (src/bignum.c:367), whose limb-count
//! overflow guard fires regardless of whether VALUE is zero:
//!
//!   overflow when:  lim - emacs_mpz_size(value) < count / GMP_NUMB_BITS
//!   with  lim = GMP_NLIMBS_MAX - 1 = INT_MAX - 1 = 2147483646  (64-bit build)
//!         GMP_NUMB_BITS = 64
//!
//! So `(ash 0 (expt 2 50))` must signal `overflow-error`, even though the
//! mathematical result (0 << huge) is just 0. NeoMacs previously
//! short-circuited `0 << n => 0` unconditionally for any COUNT, returning 0.
//!
//! Oracle values were produced with GNU Emacs:
//!   emacs --batch --eval \
//!     '(prin1 (condition-case e (ash 0 (expt 2 50)) (overflow-error (quote OVERFLOW))))'
//!   => OVERFLOW
//!
//! The exact threshold (count / 64 > 2147483646) was confirmed against the
//! GNU oracle:
//!   (ash 0 137438953407) => 0          ; 137438953407 / 64 == 2147483646
//!   (ash 0 137438953408) => OVERFLOW   ; 137438953408 / 64 == 2147483647

use crate::emacs_core::{Context, format_eval_result};

/// Evaluate `src`, catching `overflow-error` so we can distinguish a
/// silent-0 bug (returns "OK 0") from GNU's signal ("OK OVERFLOW").
fn eval_catching_overflow(expr: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let src = format!("(condition-case e {expr} (overflow-error 'OVERFLOW))");
    format_eval_result(&ev.eval_str(&src))
}

/// Plain evaluation (no condition-case wrapper).
fn eval_one(src: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    format_eval_result(&ev.eval_str(src))
}

// -----------------------------------------------------------------------
// The reported bug: (ash 0 BIG-FIXNUM) must signal overflow-error, not 0.
// -----------------------------------------------------------------------

#[test]
fn ash_zero_value_huge_fixnum_count_signals_overflow() {
    // GNU: (ash 0 (expt 2 50)) => overflow-error.  2^50 / 64 >> 2147483646.
    assert_eq!(eval_catching_overflow("(ash 0 (expt 2 50))"), "OK OVERFLOW");
    // GNU: (ash 0 1000000000000) => overflow-error.
    assert_eq!(
        eval_catching_overflow("(ash 0 1000000000000)"),
        "OK OVERFLOW"
    );
}

#[test]
fn ash_nonzero_value_huge_fixnum_count_signals_overflow() {
    // GNU: (ash 1 137438953408) => overflow-error (and must not hang trying
    // to allocate a ~17 GB bignum).
    assert_eq!(
        eval_catching_overflow("(ash 1 137438953408)"),
        "OK OVERFLOW"
    );
}

// -----------------------------------------------------------------------
// Exact threshold parity with GNU (count / GMP_NUMB_BITS vs GMP_NLIMBS_MAX-1).
// -----------------------------------------------------------------------

#[test]
fn ash_zero_value_count_at_overflow_boundary() {
    // 137438953407 / 64 == 2147483646 == lim  =>  NOT overflow, returns 0.
    assert_eq!(eval_one("(ash 0 137438953407)"), "OK 0");
    // 137438953408 / 64 == 2147483647 >  lim  =>  overflow.
    assert_eq!(
        eval_catching_overflow("(ash 0 137438953408)"),
        "OK OVERFLOW"
    );
}

// -----------------------------------------------------------------------
// Bignum COUNT branch is unchanged: GNU's value==0 short-circuit lives there,
// so (ash 0 (expt 2 100)) is 0 while a nonzero value still overflows.
// -----------------------------------------------------------------------

#[test]
fn ash_bignum_count_branch_unchanged() {
    // Bignum COUNT, zero VALUE => 0 (short-circuit in the bignum branch).
    assert_eq!(eval_one("(ash 0 (expt 2 100))"), "OK 0");
    assert_eq!(eval_one("(ash 0 (expt 2 61))"), "OK 0");
    // Bignum COUNT, nonzero VALUE => overflow.
    assert_eq!(eval_catching_overflow("(ash 5 (expt 2 61))"), "OK OVERFLOW");
}

// -----------------------------------------------------------------------
// No regression on the ordinary cases the bug report calls out.
// -----------------------------------------------------------------------

#[test]
fn ash_ordinary_cases_unchanged() {
    assert_eq!(eval_one("(ash 0 0)"), "OK 0");
    assert_eq!(eval_one("(ash 0 100)"), "OK 0");
    assert_eq!(eval_one("(ash 5 10)"), "OK 5120");
    assert_eq!(
        eval_one("(ash 1 100)"),
        "OK 1267650600228229401496703205376"
    );
    // Negative counts (right shift) are untouched.
    assert_eq!(eval_one("(ash -1 -1)"), "OK -1");
    assert_eq!(eval_one("(ash (ash 1 100) -100)"), "OK 1");
}
