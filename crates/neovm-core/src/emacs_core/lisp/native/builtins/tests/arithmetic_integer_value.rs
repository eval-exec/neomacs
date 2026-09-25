//! The integer constructors behind every arithmetic result: GNU
//! `make_integer_mpz` (`src/bignum.c:146`) returns a fixnum exactly when the
//! value fits, and a fresh bignum otherwise. Checked at the fixnum edges
//! (`most-positive-fixnum` is 2^61 - 1, `most-negative-fixnum` is -2^61),
//! at the limb edges (2^64) and through the Lisp builtins that construct
//! results.

use super::*;
use crate::emacs_core::{Context, format_eval_result};

const MPF: i64 = Value::MOST_POSITIVE_FIXNUM;
const MNF: i64 = Value::MOST_NEGATIVE_FIXNUM;

fn assert_fixnum(v: Value, want: i64) {
    assert_eq!(v.as_fixnum(), Some(want), "expected fixnum {want}");
}

fn assert_bignum(v: Value, want: &Integer) {
    assert!(v.is_bignum(), "expected a bignum for {want}");
    assert_eq!(v.as_bignum().unwrap(), want);
}

#[test]
fn fixnum_from_sign_magnitude_matches_the_fixnum_range() {
    let max = MPF as u64;
    assert_eq!(Value::fixnum_from_sign_magnitude(false, 0), Some(0));
    assert_eq!(Value::fixnum_from_sign_magnitude(true, 0), Some(0));
    assert_eq!(Value::fixnum_from_sign_magnitude(false, 1), Some(1));
    assert_eq!(Value::fixnum_from_sign_magnitude(true, 1), Some(-1));
    assert_eq!(Value::fixnum_from_sign_magnitude(false, max), Some(MPF));
    assert_eq!(Value::fixnum_from_sign_magnitude(false, max + 1), None);
    assert_eq!(Value::fixnum_from_sign_magnitude(true, max), Some(-MPF));
    assert_eq!(Value::fixnum_from_sign_magnitude(true, max + 1), Some(MNF));
    assert_eq!(Value::fixnum_from_sign_magnitude(true, max + 2), None);
    assert_eq!(Value::fixnum_from_sign_magnitude(false, u64::MAX), None);
    assert_eq!(Value::fixnum_from_sign_magnitude(true, u64::MAX), None);
    assert_eq!(Value::fixnum_from_sign_magnitude(false, 1 << 63), None);
    assert_eq!(Value::fixnum_from_sign_magnitude(true, 1 << 63), None);
}

#[test]
fn make_integer_demotes_exactly_the_fixnum_range() {
    crate::test_utils::init_test_tracing();
    let two64 = Integer::from(u64::MAX) + Integer::from(1);
    for n in [0, 1, -1, 7, -7, MPF, MNF, MPF - 1, MNF + 1] {
        assert_fixnum(Value::make_integer(Integer::from(n)), n);
    }
    for want in [
        Integer::from(MPF) + Integer::from(1),
        Integer::from(MNF) - Integer::from(1),
        Integer::from(i64::MAX),
        Integer::from(i64::MIN),
        Integer::from(u64::MAX),
        -Integer::from(u64::MAX),
        two64.clone(),
        -two64.clone(),
        &two64 * &two64,
    ] {
        assert_bignum(Value::make_integer(want.clone()), &want);
    }
    // A multi-limb computation that cancels to a small value demotes.
    let big = &two64 * Integer::from(3);
    assert_fixnum(Value::make_integer(&big - &big), 0);
    assert_fixnum(Value::make_integer(&big - (&big - Integer::from(5))), 5);
    assert_fixnum(Value::make_integer(Integer::from(0) * &big), 0);
}

#[test]
fn make_int_promotes_outside_the_fixnum_range() {
    crate::test_utils::init_test_tracing();
    for n in [0, MPF, MNF, -1] {
        assert_fixnum(Value::make_int(n), n);
    }
    for n in [MPF + 1, MNF - 1, i64::MAX, i64::MIN] {
        assert_bignum(Value::make_int(n), &Integer::from(n));
    }
}

/// `(list X (eq X X'))` for X computed twice: a fixnum is `eq` to itself,
/// a bignum result is always a fresh object.
fn check(ev: &mut Context, form: &str, want: &str) {
    let src = format!("(let ((x {form}) (y {form})) (list x (eq x y)))");
    assert_eq!(
        format_eval_result(&ev.eval_str(&src)),
        format!("OK {want}"),
        "{form}"
    );
}

#[test]
fn lisp_results_demote_at_the_fixnum_edges() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    check(
        &mut ev,
        "(1+ most-positive-fixnum)",
        "(2305843009213693952 nil)",
    );
    check(
        &mut ev,
        "(1- most-negative-fixnum)",
        "(-2305843009213693953 nil)",
    );
    check(
        &mut ev,
        "(1- (1+ most-positive-fixnum))",
        "(2305843009213693951 t)",
    );
    check(
        &mut ev,
        "(1+ (1- most-negative-fixnum))",
        "(-2305843009213693952 t)",
    );
    check(
        &mut ev,
        "(- most-negative-fixnum)",
        "(2305843009213693952 nil)",
    );
    check(
        &mut ev,
        "(- (- most-negative-fixnum))",
        "(-2305843009213693952 t)",
    );
    check(
        &mut ev,
        "(* most-negative-fixnum 1)",
        "(-2305843009213693952 t)",
    );
    check(
        &mut ev,
        "(* most-negative-fixnum -1)",
        "(2305843009213693952 nil)",
    );
    check(&mut ev, "(- (expt 2 70) (expt 2 70))", "(0 t)");
    check(&mut ev, "(* (expt 2 70) 0)", "(0 t)");
    check(&mut ev, "(* 0 (expt 2 70))", "(0 t)");
    check(&mut ev, "(- (expt 2 61) 1)", "(2305843009213693951 t)");
    check(&mut ev, "(+ (expt 2 64) (- (expt 2 64)) -7)", "(-7 t)");
}
