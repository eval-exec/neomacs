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

/// The Lisp value `v` as an exact integer.
fn exact(v: Value) -> Integer {
    match v.as_fixnum() {
        Some(n) => Integer::from(n),
        None => v.as_bignum().expect("an integer result").clone(),
    }
}

/// Demotion is exact: a result is a fixnum iff it fits one.
fn assert_canonical(v: Value, want: &Integer, what: &str) {
    assert_eq!(exact(v), *want, "{what}");
    let fits = *want >= Integer::from(MNF) && *want <= Integer::from(MPF);
    assert_eq!(v.is_fixnum(), fits, "{what}: fixnum iff it fits");
}

const EDGES: &[i64] = &[
    0,
    1,
    -1,
    1 << 31,
    -(1 << 31),
    MPF,
    MNF,
    MPF - 1,
    MNF + 1,
    i64::MAX,
    i64::MIN,
    i64::MAX - 1,
    i64::MIN + 1,
    3,
    -7,
];

#[test]
fn the_i128_overflow_path_matches_malachite() {
    crate::test_utils::init_test_tracing();
    for &a in EDGES {
        for &b in EDGES {
            let want = Integer::from(a) * Integer::from(b);
            assert_canonical(
                integer_value_i128(i128::from(a) * i128::from(b)),
                &want,
                &format!("{a} * {b}"),
            );
            let want = Integer::from(a) + Integer::from(b);
            assert_canonical(
                integer_value_i128(i128::from(a) + i128::from(b)),
                &want,
                &format!("{a} + {b}"),
            );
        }
    }
    // The extremes of an i64 x i64 product.
    for r in [
        i128::from(i64::MIN) * i128::from(i64::MIN),
        i128::from(i64::MIN) * i128::from(i64::MAX),
        (1i128 << 64) - 1,
        1i128 << 64,
        -(1i128 << 64),
        -((1i128 << 64) - 1),
    ] {
        assert_canonical(integer_value_i128(r), &Integer::from(r), &format!("{r}"));
    }
}

/// The operand kinds the in-place constructors take, over fixnum edges and
/// multi-limb magnitudes of both signs, against owned malachite arithmetic.
#[test]
fn int_add_and_mul_values_match_malachite() {
    crate::test_utils::init_test_tracing();
    let two64 = Integer::from(u64::MAX) + Integer::from(1);
    let mut pool: Vec<Integer> = [0, 1, -1, 5, -5, MPF, MNF, MPF - 1, MNF + 1]
        .iter()
        .map(|&n| Integer::from(n))
        .collect();
    pool.extend([
        Integer::from(MPF) + Integer::from(1),
        Integer::from(MNF) - Integer::from(1),
        Integer::from(u64::MAX),
        -Integer::from(u64::MAX),
        two64.clone(),
        -two64.clone(),
        &two64 * &two64 - Integer::from(1),
        -(&two64 * &two64 * Integer::from(12345)),
    ]);
    let values: Vec<Value> = pool
        .iter()
        .map(|x| Value::make_integer(x.clone()))
        .collect();
    for (x, xv) in pool.iter().zip(&values) {
        for (y, yv) in pool.iter().zip(&values) {
            let (xo, yo) = (IntOperand::of(xv).unwrap(), IntOperand::of(yv).unwrap());
            assert_canonical(
                int_add_value(xo, yo, false),
                &(x + y),
                &format!("{x} + {y}"),
            );
            assert_canonical(int_add_value(xo, yo, true), &(x - y), &format!("{x} - {y}"));
            assert_canonical(int_mul_value(xo, yo), &(x * y), &format!("{x} * {y}"));
        }
        if let Some(big) = xv.as_bignum() {
            assert_canonical(bignum_negate_value(big), &-x, &format!("- {x}"));
        }
    }
}

#[test]
fn two_argument_results_are_fresh_objects() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let src = "(let ((b (* most-positive-fixnum 4)) (n (- (* most-positive-fixnum 4))))
                 (list (eq (+ b 0) b) (eq (+ 0 b) b) (eq (- b 0) b) (eq (* b 1) b)
                       (eq (* 1 b) b) (eq (- (- n)) n) (eq (1+ (1- b)) b)
                       (= (+ b 0) b) (= (* 1 b) b) (= (- (- n)) n)))";
    assert_eq!(
        format_eval_result(&ev.eval_str(src)),
        "OK (nil nil nil nil nil nil nil t t t)"
    );
}
