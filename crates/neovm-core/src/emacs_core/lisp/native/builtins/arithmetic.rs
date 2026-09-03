use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use malachite::base::num::arithmetic::traits::{Abs, Pow};
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::num::logic::traits::SignificantBits;
use malachite::base::rounding_modes::RoundingMode;
use malachite::integer::Integer;
use std::sync::Mutex;

// ===========================================================================
// Arithmetic
// ===========================================================================
//
// `+`, `-`, `*` mirror GNU's `arith_driver` (src/data.c:3215): a fast
// fixnum loop using `ckd_add` / `ckd_sub` / `ckd_mul` for overflow
// detection, and a fall-back path that switches to malachite::Integer
// the moment overflow strikes or a bignum operand appears.

/// Pull an integer-valued operand into an `i64`. Accepts fixnums and
/// markers; for any other value (including bignums) returns
/// `Err(()) → caller decides`.  This is the fast-path helper used
/// before promotion to GMP.
fn try_i64_from_value(eval: &super::eval::Context, value: &Value) -> Result<Option<i64>, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(Some(n)),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(None),
        _ if super::marker::is_marker(value) => Ok(Some(
            super::marker::marker_position_as_int_eval(eval, value)?,
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), *value],
        )),
    }
}

#[inline]
fn wrong_number_or_marker(value: &Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("number-or-marker-p"), *value],
    )
}

#[inline]
fn wrong_integer_or_marker(value: &Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("integer-or-marker-p"), *value],
    )
}

/// Materialize an integer-valued operand as a `malachite::Integer`. Used by
/// the bignum slow path. Accepts fixnums, bignums, and markers.
fn integer_from_value(eval: &super::eval::Context, value: &Value) -> Result<Integer, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(Integer::from(n)),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(value.as_bignum().unwrap().clone()),
        _ if super::marker::is_marker(value) => Ok(Integer::from(
            super::marker::marker_position_as_int_eval(eval, value)?,
        )),
        _ => Err(wrong_number_or_marker(value)),
    }
}

/// Eval-aware `+` that reads live marker positions from buffers.
///
/// Mirrors GNU `Fplus` → `arith_driver` (src/data.c:3215, 3271): if
/// every operand is an i64-valued integer or marker and no addition
/// overflows, stay on the fixnum fast path; otherwise promote to GMP
/// via `malachite::Integer`. Float operands divert through `make_float`
/// as before.
///
/// Note: i64 has 64 bits, but fixnums only get 62 bits (the low 2 are
/// the tag). A sum like `most-positive-fixnum + 1` does not overflow
/// i64 yet exceeds fixnum range; the final i64 result therefore returns
/// through `Value::make_int`, mirroring GNU `make_int` (`src/lisp.h`).
pub(crate) fn builtin_add_slice(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> EvalResult {
    match args {
        [] => return Ok(Value::fixnum(0)),
        [arg] => {
            if arg.as_fixnum().is_some() || arg.is_float() || arg.is_bignum() {
                return Ok(*arg);
            }
            if super::marker::is_marker(arg) {
                return Ok(Value::make_int(super::marker::marker_position_as_int_eval(
                    eval, arg,
                )?));
            }
            return Err(wrong_number_or_marker(arg));
        }
        _ => {
            if let Some(sum) = try_small_fixnum_add(args) {
                return Ok(sum);
            }
        }
    }

    // GNU `arith_driver`: stay in the fixnum loop until the current
    // operand forces float or bignum arithmetic.
    let mut sum: i64 = 0;
    for (i, a) in args.iter().enumerate() {
        if let Some(n) = a.as_fixnum() {
            match sum.checked_add(n) {
                Some(s) => {
                    sum = s;
                    continue;
                }
                None => {
                    let mut acc = Integer::from(sum);
                    acc += Integer::from(n);
                    return continue_bignum_add(eval, &args[i + 1..], acc);
                }
            }
        }
        if a.is_float() {
            return continue_float_add(eval, &args[i + 1..], sum as f64 + a.xfloat());
        }
        if let Some(big) = a.as_bignum() {
            let mut acc = Integer::from(sum);
            acc += big;
            return continue_bignum_add(eval, &args[i + 1..], acc);
        }
        if super::marker::is_marker(a) {
            let n = super::marker::marker_position_as_int_eval(eval, a)?;
            match sum.checked_add(n) {
                Some(s) => {
                    sum = s;
                    continue;
                }
                None => {
                    let mut acc = Integer::from(sum);
                    acc += Integer::from(n);
                    return continue_bignum_add(eval, &args[i + 1..], acc);
                }
            }
        }
        return Err(wrong_number_or_marker(a));
    }
    Ok(Value::make_int(sum))
}

#[inline]
fn try_small_fixnum_add(args: &[Value]) -> Option<Value> {
    match args {
        [a, b] => Some(Value::make_int(a.as_fixnum()?.checked_add(b.as_fixnum()?)?)),
        [a, b, c] => {
            let sum = a.as_fixnum()?.checked_add(b.as_fixnum()?)?;
            Some(Value::make_int(sum.checked_add(c.as_fixnum()?)?))
        }
        [a, b, c, d] => {
            let sum = a.as_fixnum()?.checked_add(b.as_fixnum()?)?;
            let sum = sum.checked_add(c.as_fixnum()?)?;
            Some(Value::make_int(sum.checked_add(d.as_fixnum()?)?))
        }
        _ => None,
    }
}

fn continue_float_add(
    eval: &super::super::eval::Context,
    rest: &[Value],
    mut acc: f64,
) -> EvalResult {
    for a in rest {
        acc += expect_number_or_marker_f64_eval(eval, a)?;
    }
    Ok(Value::make_float(acc))
}

fn continue_bignum_add(
    eval: &super::super::eval::Context,
    rest: &[Value],
    mut acc: Integer,
) -> EvalResult {
    for (i, a) in rest.iter().enumerate() {
        if a.is_float() {
            return continue_float_add(
                eval,
                &rest[i + 1..],
                f64::rounding_from(&acc, RoundingMode::Nearest).0 + a.xfloat(),
            );
        }
        let n = integer_from_value(eval, a)?;
        acc += n;
    }
    Ok(Value::make_integer(acc))
}

/// Eval-aware `-` that reads live marker positions from buffers.
///
/// Mirrors GNU `Fminus` (`src/data.c:3282`):
/// * 0 args -> 0
/// * 1 arg  -> negation (with bignum promotion for `MIN_FIXNUM`)
/// * N args -> arith_driver in subtract mode
pub(crate) fn builtin_sub_slice(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::fixnum(0));
    }
    if args.len() == 1 {
        return negate_value(eval, &args[0]);
    }

    let first = &args[0];
    let mut acc: i64 = if let Some(n) = first.as_fixnum() {
        n
    } else if first.is_float() {
        return continue_float_sub(eval, &args[1..], first.xfloat());
    } else if let Some(big) = first.as_bignum() {
        return continue_bignum_sub(eval, &args[1..], big.clone());
    } else if super::marker::is_marker(first) {
        super::marker::marker_position_as_int_eval(eval, first)?
    } else {
        return Err(wrong_number_or_marker(first));
    };

    for (i, a) in args[1..].iter().enumerate() {
        if let Some(n) = a.as_fixnum() {
            match acc.checked_sub(n) {
                Some(s) => {
                    acc = s;
                    continue;
                }
                None => {
                    let mut bacc = Integer::from(acc);
                    bacc -= Integer::from(n);
                    return continue_bignum_sub(eval, &args[i + 2..], bacc);
                }
            }
        }
        if a.is_float() {
            return continue_float_sub(eval, &args[i + 2..], acc as f64 - a.xfloat());
        }
        if let Some(big) = a.as_bignum() {
            let mut bacc = Integer::from(acc);
            bacc -= big;
            return continue_bignum_sub(eval, &args[i + 2..], bacc);
        }
        if super::marker::is_marker(a) {
            let n = super::marker::marker_position_as_int_eval(eval, a)?;
            match acc.checked_sub(n) {
                Some(s) => {
                    acc = s;
                    continue;
                }
                None => {
                    let mut bacc = Integer::from(acc);
                    bacc -= Integer::from(n);
                    return continue_bignum_sub(eval, &args[i + 2..], bacc);
                }
            }
        }
        return Err(wrong_number_or_marker(a));
    }
    // Promote i64 results that exceeded fixnum range (62-bit) but
    // stayed within i64 (64-bit), matching GNU `make_int`.
    Ok(Value::make_int(acc))
}

fn continue_float_sub(
    eval: &super::super::eval::Context,
    rest: &[Value],
    mut acc: f64,
) -> EvalResult {
    for a in rest {
        acc -= expect_number_or_marker_f64_eval(eval, a)?;
    }
    Ok(Value::make_float(acc))
}

fn continue_bignum_sub(
    eval: &super::super::eval::Context,
    rest: &[Value],
    mut acc: Integer,
) -> EvalResult {
    for (i, a) in rest.iter().enumerate() {
        if a.is_float() {
            return continue_float_sub(
                eval,
                &rest[i + 1..],
                f64::rounding_from(&acc, RoundingMode::Nearest).0 - a.xfloat(),
            );
        }
        let n = integer_from_value(eval, a)?;
        acc -= n;
    }
    Ok(Value::make_integer(acc))
}

/// Negate a single value, mirroring GNU `Fminus` 1-arg branch
/// (`src/data.c:3293-3300`). Promotes `MOST_NEGATIVE_FIXNUM` to a
/// bignum because `-MOST_NEGATIVE_FIXNUM` exceeds fixnum range.
fn negate_value(eval: &super::super::eval::Context, value: &Value) -> EvalResult {
    if value.is_float() {
        return Ok(Value::make_float(-value.xfloat()));
    }
    if let Some(big) = value.as_bignum() {
        return Ok(Value::make_integer(-big.clone()));
    }
    let n = match try_i64_from_value(eval, value)? {
        Some(n) => n,
        None => unreachable!(),
    };
    // checked_neg only fails for i64::MIN; for everything else we get
    // an i64 back which still has to clear the fixnum-range hurdle.
    match n.checked_neg() {
        Some(neg) => Ok(Value::make_int(neg)),
        None => Ok(Value::make_integer(-Integer::from(n))),
    }
}

/// `*` with bignum promotion. Mirrors GNU `Ftimes` -> `arith_driver`
/// (`src/data.c:3304`).
pub(crate) fn builtin_mul(args: Vec<Value>) -> EvalResult {
    let mut prod: i64 = 1;
    for (i, a) in args.iter().enumerate() {
        if let Some(n) = a.as_fixnum() {
            match prod.checked_mul(n) {
                Some(p) => {
                    prod = p;
                    continue;
                }
                None => {
                    let mut acc = Integer::from(prod);
                    acc *= Integer::from(n);
                    return continue_bignum_mul(&args[i + 1..], acc);
                }
            }
        }
        if a.is_float() {
            return continue_float_mul(&args[i + 1..], prod as f64 * a.xfloat());
        }
        if let Some(big) = a.as_bignum() {
            let mut acc = Integer::from(prod);
            acc *= big;
            return continue_bignum_mul(&args[i + 1..], acc);
        }
        if super::marker::is_marker(a) {
            let n = super::marker::marker_position_as_int(a)?;
            match prod.checked_mul(n) {
                Some(p) => {
                    prod = p;
                    continue;
                }
                None => {
                    let mut acc = Integer::from(prod);
                    acc *= Integer::from(n);
                    return continue_bignum_mul(&args[i + 1..], acc);
                }
            }
        }
        return Err(wrong_number_or_marker(a));
    }
    Ok(Value::make_int(prod))
}

fn continue_float_mul(rest: &[Value], mut acc: f64) -> EvalResult {
    for a in rest {
        acc *= expect_number_or_marker_f64(a)?;
    }
    Ok(Value::make_float(acc))
}

fn continue_bignum_mul(rest: &[Value], mut acc: Integer) -> EvalResult {
    for (i, a) in rest.iter().enumerate() {
        if a.is_float() {
            return continue_float_mul(
                &rest[i + 1..],
                f64::rounding_from(&acc, RoundingMode::Nearest).0 * a.xfloat(),
            );
        }
        match a.kind() {
            ValueKind::Fixnum(n) => acc *= Integer::from(n),
            ValueKind::Veclike(VecLikeType::Bignum) => acc *= a.as_bignum().unwrap(),
            _ if super::marker::is_marker(a) => {
                acc *= Integer::from(super::marker::marker_position_as_int(a)?);
            }
            _ => return Err(wrong_number_or_marker(a)),
        }
    }
    Ok(Value::make_integer(acc))
}
/// `/` with bignum support. Mirrors GNU `Fquo` (`src/data.c:3315`).
///
/// Truncation toward zero (`tdiv_q` semantics, matching `mpz_tdiv_q`),
/// promoting `i64::MIN / -1` to bignum since `-i64::MIN` overflows i64.
/// Float operands divert through float division as before.
pub(crate) fn builtin_div(args: Vec<Value>) -> EvalResult {
    expect_min_args("/", &args, 1)?;
    // Single argument: return 1 / arg (reciprocal), matching GNU Emacs.
    if args.len() == 1 {
        return div_one_arg(&args[0]);
    }
    if has_float(&args) {
        let mut acc = expect_number_or_marker_f64(&args[0])?;
        for a in &args[1..] {
            let d = expect_number_or_marker_f64(a)?;
            acc /= d;
            if acc.is_nan() {
                // Emacs prints negative-NaN for float zero-divisor paths.
                acc = f64::from_bits(f64::NAN.to_bits() | (1_u64 << 63));
            }
        }
        return Ok(Value::make_float(acc));
    }
    // Integer fast path with bignum promotion on overflow.
    let first = &args[0];
    // If first is a bignum, start GMP path immediately.
    if first.is_bignum() {
        let acc = first.as_bignum().unwrap().clone();
        return continue_bignum_div(&args[1..], acc);
    }
    let mut acc: i64 = expect_integer_or_marker_after_number_check(first)?;
    for (i, a) in args[1..].iter().enumerate() {
        if a.is_bignum() {
            // Promote: convert acc to bignum and divide by this bignum,
            // then continue.
            let mut bacc = Integer::from(acc);
            let big = a.as_bignum().unwrap();
            if *big == 0 {
                return Err(signal(LispCondition::ArithError, vec![]));
            }
            bacc /= big;
            return continue_bignum_div(&args[i + 2..], bacc);
        }
        let d = expect_integer_or_marker_after_number_check(a)?;
        if d == 0 {
            return Err(signal(LispCondition::ArithError, vec![]));
        }
        match acc.checked_div(d) {
            Some(q) => acc = q,
            None => {
                // Only `i64::MIN / -1` triggers this. Promote.
                let bacc = Integer::from(acc) / Integer::from(d);
                return continue_bignum_div(&args[i + 2..], bacc);
            }
        }
    }
    Ok(Value::make_int(acc))
}

fn div_one_arg(arg: &Value) -> EvalResult {
    if arg.is_float() {
        let d = arg.xfloat();
        return Ok(Value::make_float(1.0 / d));
    }
    if let Some(big) = arg.as_bignum() {
        // GNU: dividing 1 by any bignum yields 0 (since |bignum| > MAX_FIXNUM).
        if *big == 0 {
            return Err(signal(LispCondition::ArithError, vec![]));
        }
        return Ok(Value::fixnum(0));
    }
    let d = expect_integer_or_marker_after_number_check(arg)?;
    if d == 0 {
        return Err(signal(LispCondition::ArithError, vec![]));
    }
    Ok(Value::fixnum(1 / d))
}

fn continue_bignum_div(rest: &[Value], mut acc: Integer) -> EvalResult {
    for a in rest {
        if let Some(big) = a.as_bignum() {
            if *big == 0 {
                return Err(signal(LispCondition::ArithError, vec![]));
            }
            acc /= big;
            continue;
        }
        let d = expect_integer_or_marker_after_number_check(a)?;
        if d == 0 {
            return Err(signal(LispCondition::ArithError, vec![]));
        }
        acc /= Integer::from(d);
    }
    Ok(Value::make_integer(acc))
}

/// `(% X Y)` — integer remainder, mirrors GNU `Frem` (`src/data.c:3402`).
///
/// Result has the same sign as the dividend (`mpz_tdiv_r` semantics).
pub(crate) fn builtin_percent(
    _eval: &mut super::eval::Context,
    num: Value,
    den: Value,
) -> EvalResult {
    if let (Some(a), Some(b)) = (num.as_fixnum(), den.as_fixnum())
        && b != 0
    {
        // |a % b| < |b|, so the result always fits a fixnum.
        return Ok(Value::fixnum(a.checked_rem(b).unwrap_or_default()));
    }
    let num = check_integer_coerce_marker(&num)?;
    let den = check_integer_coerce_marker(&den)?;
    integer_remainder(&num, &den, false)
}

/// GNU `check_integer_coerce_marker` (`src/data.c`): fixnums and bignums
/// pass through, markers coerce to their position, anything else signals
/// `integer-or-marker-p` (floats included — `%` is integer-only, unlike
/// `mod`).
fn check_integer_coerce_marker(value: &Value) -> Result<Value, Flow> {
    match value.kind() {
        ValueKind::Fixnum(_) | ValueKind::Veclike(VecLikeType::Bignum) => Ok(*value),
        _ if super::marker::is_marker(value) => {
            Ok(Value::fixnum(super::marker::marker_position_as_int(value)?))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

/// `(mod X Y)` — modulo, mirrors GNU `Fmod` (`src/data.c:3412`).
///
/// Result has the same sign as the divisor.
pub(crate) fn builtin_mod(_eval: &mut super::eval::Context, num: Value, den: Value) -> EvalResult {
    if let (Some(a), Some(b)) = (num.as_fixnum(), den.as_fixnum())
        && b != 0
    {
        let r = a.checked_rem(b).unwrap_or_default();
        // Sign fixup toward the divisor; |r + b| < |b|, no overflow.
        let r = if r != 0 && (r < 0) != (b < 0) {
            r + b
        } else {
            r
        };
        return Ok(Value::fixnum(r));
    }
    if num.is_float() || den.is_float() {
        // GNU `fmod_float` path — float-modulo. Existing behavior.
        let a = expect_number_or_marker_f64(&num)?;
        let b = expect_number_or_marker_f64(&den)?;
        let r = a % b;
        let mut r = if r != 0.0 && (r < 0.0) != (b < 0.0) {
            r + b
        } else {
            r
        };
        if r.is_nan() {
            r = f64::from_bits(f64::NAN.to_bits() | (1_u64 << 63));
        }
        return Ok(Value::make_float(r));
    }
    integer_remainder(&num, &den, true)
}

/// Shared integer remainder for `%` and `mod`. Mirrors GNU
/// `integer_remainder` (`src/data.c:3351`). When `modulo` is true the
/// result is fixed up to have the divisor's sign.
fn integer_remainder(num: &Value, den: &Value, modulo: bool) -> EvalResult {
    // Bignum slow path if either side is a bignum, or if the i64 fast
    // path can't represent the operands (markers always fit).
    if num.is_bignum() || den.is_bignum() {
        let num_big = bignum_or_int_to_integer(num)?;
        let den_big = bignum_or_int_to_integer(den)?;
        if den_big == 0 {
            return Err(signal(LispCondition::ArithError, vec![]));
        }
        let mut r = &num_big % &den_big;
        if modulo {
            let r_neg = r < 0;
            let d_neg = den_big < 0;
            // Wrong sign means r and d have opposite signs.
            if r_neg != d_neg && r != 0 {
                r += &den_big;
            }
        }
        return Ok(Value::make_integer(r));
    }
    // GNU `Fmod` (data.c:3412) does CHECK_NUMBER_COERCE_MARKER on both
    // operands first, so non-numeric values must signal
    // `number-or-marker-p`, not `integer-or-marker-p`. Mirror that by
    // routing through the after-number-check helper.
    let a = expect_integer_or_marker_after_number_check(num)?;
    let b = expect_integer_or_marker_after_number_check(den)?;
    if b == 0 {
        return Err(signal(LispCondition::ArithError, vec![]));
    }
    // i64::MIN % -1 is 0 mathematically, but checked_rem returns None.
    let r: i64 = a.checked_rem(b).unwrap_or_default();
    let r = if modulo && r != 0 && (r < 0) != (b < 0) {
        r + b
    } else {
        r
    };
    Ok(Value::make_int(r))
}

/// Convert a fixnum / bignum / marker operand to `Integer`. Used
/// by the integer remainder slow path.
fn bignum_or_int_to_integer(value: &Value) -> Result<Integer, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(Integer::from(n)),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(value.as_bignum().unwrap().clone()),
        _ if super::marker::is_marker(value) => {
            Ok(Integer::from(super::marker::marker_position_as_int(value)?))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

/// `(1+ NUMBER)` — mirrors GNU `Fadd1` (`src/data.c:3634`).
/// Promotes to bignum on `MOST_POSITIVE_FIXNUM + 1`.
pub(crate) fn builtin_add1_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    add1_value(arg)
}

fn add1_value(arg: Value) -> EvalResult {
    match arg.kind() {
        ValueKind::Fixnum(n) => match n.checked_add(1) {
            Some(s) => Ok(Value::make_int(s)),
            None => Ok(Value::make_integer(Integer::from(n) + Integer::from(1))),
        },
        ValueKind::Float => Ok(Value::make_float(arg.xfloat() + 1.0)),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(Value::make_integer(
            arg.as_bignum().unwrap().clone() + Integer::from(1),
        )),
        _ if arg.is_marker() => {
            let n = super::marker::marker_position_as_int(&arg)?;
            match n.checked_add(1) {
                Some(s) => Ok(Value::make_int(s)),
                None => Ok(Value::make_integer(Integer::from(n) + Integer::from(1))),
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), arg],
        )),
    }
}

/// `(1- NUMBER)` — mirrors GNU `Fsub1` (`src/data.c:3658`).
/// Promotes to bignum on `MOST_NEGATIVE_FIXNUM - 1`.
pub(crate) fn builtin_sub1_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    sub1_value(arg)
}

fn sub1_value(arg: Value) -> EvalResult {
    match arg.kind() {
        ValueKind::Fixnum(n) => match n.checked_sub(1) {
            Some(s) => Ok(Value::make_int(s)),
            None => Ok(Value::make_integer(Integer::from(n) - Integer::from(1))),
        },
        ValueKind::Float => Ok(Value::make_float(arg.xfloat() - 1.0)),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(Value::make_integer(
            arg.as_bignum().unwrap().clone() - Integer::from(1),
        )),
        _ if arg.is_marker() => {
            let n = super::marker::marker_position_as_int(&arg)?;
            match n.checked_sub(1) {
                Some(s) => Ok(Value::make_int(s)),
                None => Ok(Value::make_integer(Integer::from(n) - Integer::from(1))),
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), arg],
        )),
    }
}

pub(crate) fn builtin_max_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    expect_min_args("max", args, 1)?;
    minmax_driver(eval, args, NumCmp::Gt)
}

pub(crate) fn builtin_min_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    expect_min_args("min", args, 1)?;
    minmax_driver(eval, args, NumCmp::Lt)
}

/// Mirrors GNU `minmax_driver` (`src/data.c:3461`). Folds the args with
/// `arithcompare` against the running accumulator: if `arithcompare(val,
/// accum)` satisfies `cmp` the accumulator becomes `val`; otherwise, if
/// `val` is a NaN, it propagates as the result (a NaN never compares
/// greater/less, so it would otherwise be silently dropped). Markers are
/// coerced to their integer position; every other arg is returned
/// unchanged (so `(max 1 2.0 3)` stays the integer `3`, matching GNU).
fn minmax_driver(eval: &super::eval::Context, args: &[Value], cmp: NumCmp) -> EvalResult {
    let coerce = |v: &Value| -> Result<Value, Flow> {
        if super::marker::is_marker(v) {
            Ok(Value::fixnum(super::marker::marker_position_as_int_eval(
                eval, v,
            )?))
        } else {
            // Validate it is a number (signals otherwise).
            expect_number_or_marker_eval(eval, v)?;
            Ok(*v)
        }
    };

    let mut accum = coerce(&args[0])?;
    for a in &args[1..] {
        let val = coerce(a)?;
        let ord = arithcompare(eval, &val, &accum)?;
        if cmp_passes(ord, cmp) {
            accum = val;
        } else if val.is_float() && val.xfloat().is_nan() {
            return Ok(val);
        }
    }
    Ok(accum)
}

/// `(abs ARG)` — mirrors GNU `Fabs` (`src/floatfns.c`).
///
/// Promotes `MOST_NEGATIVE_FIXNUM` to a bignum (audit §2.6) instead
/// of signaling overflow-error.
pub(crate) fn builtin_abs(args: Vec<Value>) -> EvalResult {
    expect_args("abs", &args, 1)?;
    match args[0].kind() {
        ValueKind::Fixnum(n) => match n.checked_abs() {
            // Even non-overflowing |i64| might exceed fixnum range.
            Some(a) => Ok(Value::make_int(a)),
            None => Ok(Value::make_integer(Integer::from(n).abs())),
        },
        ValueKind::Float => Ok(Value::make_float(args[0].xfloat().abs())),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(Value::make_integer(
            args[0].as_bignum().unwrap().clone().abs(),
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), args[0]],
        )),
    }
}

// ===========================================================================
// Logical / bitwise
// ===========================================================================

/// `(logand &rest INTS-OR-MARKERS)` — bitwise AND.
///
/// Mirrors GNU `Flogand` (`src/data.c:3458`) → `arith_driver Alogand`.
/// When any operand is a bignum the whole reduction runs in GMP via
/// `mpz_and`; otherwise we stay on the i64 fast path. Note: bitwise
/// AND of i64 values can never overflow into bignum range, but the
/// final result still has to clear the fixnum-bits hurdle since `&`
/// can produce a value with the high bits set (e.g. `(logand -1 -1)
/// → -1` is fine, but `(logand most-positive-fixnum #x7fffffffffffffff)`
/// could exceed fixnum range). Return through `make_int`.
pub(crate) fn builtin_logand_slice(args: &[Value]) -> EvalResult {
    if let [lhs, rhs] = args
        && let (Some(lhs), Some(rhs)) = (lhs.as_fixnum(), rhs.as_fixnum())
    {
        return Ok(Value::fixnum(lhs & rhs));
    }
    builtin_logop(args, BignumLogop::And)
}

pub(crate) fn builtin_logior_slice(args: &[Value]) -> EvalResult {
    if let [lhs, rhs] = args
        && let (Some(lhs), Some(rhs)) = (lhs.as_fixnum(), rhs.as_fixnum())
    {
        return Ok(Value::fixnum(lhs | rhs));
    }
    builtin_logop(args, BignumLogop::Or)
}

pub(crate) fn builtin_logxor_slice(args: &[Value]) -> EvalResult {
    if let [lhs, rhs] = args
        && let (Some(lhs), Some(rhs)) = (lhs.as_fixnum(), rhs.as_fixnum())
    {
        return Ok(Value::fixnum(lhs ^ rhs));
    }
    builtin_logop(args, BignumLogop::Xor)
}

#[derive(Clone, Copy)]
enum BignumLogop {
    And,
    Or,
    Xor,
}

fn builtin_logop(args: &[Value], op: BignumLogop) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::fixnum(match op {
            BignumLogop::And => -1,
            BignumLogop::Or | BignumLogop::Xor => 0,
        }));
    }

    let first = &args[0];
    if args.len() == 1 {
        if first.as_fixnum().is_some() || first.as_bignum().is_some() {
            return Ok(*first);
        }
        if super::marker::is_marker(first) {
            return Ok(Value::fixnum(super::marker::marker_position_as_int(first)?));
        }
        return Err(wrong_integer_or_marker(first));
    }

    let mut acc = if let Some(n) = first.as_fixnum() {
        n
    } else if let Some(big) = first.as_bignum() {
        return continue_bignum_logop(&args[1..], big.clone(), op);
    } else if super::marker::is_marker(first) {
        super::marker::marker_position_as_int(first)?
    } else {
        return Err(wrong_integer_or_marker(first));
    };

    for (i, a) in args[1..].iter().enumerate() {
        if let Some(n) = a.as_fixnum() {
            apply_i64_logop(&mut acc, n, op);
            continue;
        }
        if let Some(big) = a.as_bignum() {
            let mut bacc = Integer::from(acc);
            apply_bignum_logop(&mut bacc, big, op);
            return continue_bignum_logop(&args[i + 2..], bacc, op);
        }
        if super::marker::is_marker(a) {
            let n = super::marker::marker_position_as_int(a)?;
            apply_i64_logop(&mut acc, n, op);
            continue;
        }
        if a.is_float() {
            return Err(wrong_integer_or_marker(a));
        }
        return Err(wrong_number_or_marker(a));
    }
    Ok(Value::make_int(acc))
}

#[inline]
fn apply_i64_logop(acc: &mut i64, next: i64, op: BignumLogop) {
    match op {
        BignumLogop::And => *acc &= next,
        BignumLogop::Or => *acc |= next,
        BignumLogop::Xor => *acc ^= next,
    }
}

#[inline]
fn apply_bignum_logop(acc: &mut Integer, next: &Integer, op: BignumLogop) {
    match op {
        BignumLogop::And => *acc &= next,
        BignumLogop::Or => *acc |= next,
        BignumLogop::Xor => *acc ^= next,
    }
}

fn continue_bignum_logop(rest: &[Value], mut acc: Integer, op: BignumLogop) -> EvalResult {
    for a in rest {
        if let Some(n) = a.as_fixnum() {
            let next = Integer::from(n);
            apply_bignum_logop(&mut acc, &next, op);
            continue;
        }
        if let Some(big) = a.as_bignum() {
            apply_bignum_logop(&mut acc, big, op);
            continue;
        }
        if super::marker::is_marker(a) {
            let next = Integer::from(super::marker::marker_position_as_int(a)?);
            apply_bignum_logop(&mut acc, &next, op);
            continue;
        }
        if a.is_float() {
            return Err(wrong_integer_or_marker(a));
        }
        return Err(wrong_number_or_marker(a));
    }
    Ok(Value::make_integer(acc))
}

/// `(lognot NUMBER)` — mirrors GNU `Flognot` (`src/data.c:3648`).
pub(crate) fn builtin_lognot(args: Vec<Value>) -> EvalResult {
    expect_args("lognot", &args, 1)?;
    if let Some(big) = args[0].as_bignum() {
        return Ok(Value::make_integer(!big.clone()));
    }
    let n = expect_int(&args[0])?;
    Ok(Value::fixnum(!n))
}

/// Width of a GMP limb in bits on the 64-bit builds Emacs targets
/// (`GMP_NUMB_BITS`). neomacs's bignum backend (malachite) likewise uses
/// 64-bit limbs, so this matches GNU's `mpz_size` semantics.
const GMP_NUMB_BITS: u64 = 64;

/// GNU `GMP_NLIMBS_MAX = min (INT_MAX, ULONG_MAX / GMP_NUMB_BITS)`. On the
/// 64-bit platforms Emacs supports this resolves to `INT_MAX`.
const GMP_NLIMBS_MAX: u64 = i32::MAX as u64;

/// GNU `mul_2exp_extra_limbs` fudge factor (`src/bignum.c:371`).
const MUL_2EXP_EXTRA_LIMBS: u64 = 1;

/// Number of 64-bit GMP limbs needed to represent |value|, matching
/// GNU's `emacs_mpz_size` / `mpz_size` (0 for a zero magnitude).
fn mpz_limb_count(value: &Integer) -> u64 {
    let bits = value.significant_bits();
    if bits == 0 {
        0
    } else {
        bits.div_ceil(GMP_NUMB_BITS)
    }
}

/// Replicates the overflow guard in GNU's `emacs_mpz_mul_2exp`
/// (`src/bignum.c:367`): a left shift by `count` bits overflows when the
/// resulting limb count would exceed Emacs's bignum size limit. Equivalent
/// to GNU's `lim - emacs_mpz_size (op1) < op2 / GMP_NUMB_BITS`.
fn mul_2exp_would_overflow(value: &Integer, count: i64) -> bool {
    debug_assert!(count > 0);
    // GNU: lim = min (NLIMBS_LIMIT, GMP_NLIMBS_MAX - mul_2exp_extra_limbs).
    // On a 64-bit build NLIMBS_LIMIT = MOST_POSITIVE_FIXNUM / GMP_NUMB_BITS,
    // which is far larger than GMP_NLIMBS_MAX (= INT_MAX), so the binding
    // term is GMP_NLIMBS_MAX - 1.
    let lim = GMP_NLIMBS_MAX - MUL_2EXP_EXTRA_LIMBS;
    let op2limbs = (count as u64) / GMP_NUMB_BITS;
    // op1 is `value`; emacs_mpz_size(op1) == mpz_limb_count(value).
    lim.saturating_sub(mpz_limb_count(value)) < op2limbs
}

/// `(ash VALUE COUNT)` — arithmetic shift, mirrors GNU `Fash`
/// (`src/data.c:3519`).
///
/// Positive COUNT shifts left, negative shifts right. Both VALUE and
/// COUNT may be bignums. The result is promoted to bignum on left
/// shifts that exceed fixnum range — most importantly `(ash 1 100)`
/// must return 2^100, not 0 (audit §2.7).
pub(crate) fn builtin_ash_slice(args: &[Value]) -> EvalResult {
    expect_args("ash", args, 2)?;
    let value = &args[0];
    let count_val = &args[1];

    // COUNT must be an integer (fixnum or bignum). If it's a bignum
    // and VALUE is anything but zero, GNU signals overflow-error for
    // positive counts (no machine could represent the result) and
    // returns 0 / -1 for negative counts (the value is shifted away).
    let count_i64 = match count_val.kind() {
        ValueKind::Fixnum(c) => c,
        ValueKind::Veclike(VecLikeType::Bignum) => {
            let big = count_val.as_bignum().unwrap();
            // Zero VALUE is unchanged regardless of COUNT.
            if value
                .as_fixnum()
                .map(|n| n == 0)
                .or_else(|| value.as_bignum().map(|b| *b == 0))
                .unwrap_or(false)
            {
                return Ok(Value::fixnum(0));
            }
            if *big < 0 {
                // Negative count + nonzero value: result is 0 (or -1 for negative).
                let sign_neg = match value.kind() {
                    ValueKind::Fixnum(n) => n < 0,
                    ValueKind::Veclike(VecLikeType::Bignum) => *value.as_bignum().unwrap() < 0,
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("integerp"), *value],
                        ));
                    }
                };
                return Ok(Value::fixnum(if sign_neg { -1 } else { 0 }));
            }
            return Err(signal(LispCondition::OverflowError, vec![]));
        }
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), *count_val],
            ));
        }
    };

    // Materialize VALUE as a Integer once. We could try to keep
    // small fixnum shifts on the i64 path, but ash is rare enough that
    // correctness over branchy fast-pathing is the right tradeoff.
    let value_big = match value.kind() {
        ValueKind::Fixnum(n) => Integer::from(n),
        ValueKind::Veclike(VecLikeType::Bignum) => value.as_bignum().unwrap().clone(),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), *value],
            ));
        }
    };

    if count_i64 == 0 {
        return Ok(Value::make_integer(value_big));
    }
    let result = if count_i64 > 0 {
        // Left shift. Mirror GNU `emacs_mpz_mul_2exp` (src/bignum.c:367):
        // it rejects shifts whose result limb count would overflow GMP /
        // Emacs's bignum size limit, even when VALUE is zero. (GNU's
        // `value == 0` short-circuit lives only in the *bignum* COUNT
        // branch above; for a fixnum COUNT it falls through to this
        // overflow check.) So `(ash 0 (expt 2 50))` must signal
        // `overflow-error`, not return 0.
        if mul_2exp_would_overflow(&value_big, count_i64) {
            return Err(signal(LispCondition::OverflowError, vec![]));
        }
        // The overflow check guarantees `count_i64` fits the bignum size
        // limit, hence well within `u32`, so this conversion is exact.
        let bits = u32::try_from(count_i64).unwrap_or(u32::MAX);
        value_big << bits
    } else {
        // Arithmetic right shift (toward -infinity, i.e. mpz_fdiv_q_2exp).
        // For very large negative counts, the value is shifted away;
        // GNU returns -1 for negative VALUE and 0 otherwise.
        let neg_count = count_i64.checked_neg().unwrap_or(i64::MAX);
        let bits = u32::try_from(neg_count).unwrap_or(u32::MAX);
        // Integer >> u32 does mpz_fdiv_q_2exp (floor division).
        value_big >> bits
    };
    Ok(Value::make_integer(result))
}

// ===========================================================================
// Comparisons
// ===========================================================================
//
// Mirrors GNU `arithcompare` (src/data.c:2682). For two integers
// (fixnum or bignum) we compare exactly via Integer; for any
// pair involving a float we compare the float against the integer
// using Integer::partial_cmp<f64>, which is exact (it accounts
// for whether the float is integer-valued and how it relates to the
// bignum). The previous f64-only path lost precision for any bignum
// outside ±2^53 (audit §1.1 — comparisons part).

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumCmp {
    Lt,
    Le,
    Eq,
    Ne,
    Gt,
    Ge,
}

fn arithcompare(
    eval: &super::super::eval::Context,
    a: &Value,
    b: &Value,
) -> Result<Option<std::cmp::Ordering>, Flow> {
    // Float on either side. GNU `arithcompare` compares a float against
    // an integer (fixnum OR bignum) EXACTLY — it never coerces the
    // integer to a double first (data.c:2734-2758 / 2777-2795 /
    // 2760-2770 / 2818-2829). We mirror that by comparing the exact
    // `Integer` against the `f64` via `Integer::partial_cmp<f64>`, which
    // accounts for the float's fractional part and any magnitude beyond
    // 2^53. Only float-vs-float falls back to native f64 comparison.
    if a.is_float() || b.is_float() {
        // a is the float side, b is the integer-or-marker side.
        if a.is_float() && !b.is_float() {
            let f = a.xfloat();
            if f.is_nan() {
                return Ok(None);
            }
            let bi = integer_or_marker_to_big(eval, b)?;
            // We have bi.partial_cmp(f); reverse to get a.cmp(b).
            return Ok(bi.partial_cmp(&f).map(|o| o.reverse()));
        }
        // b is the float side, a is the integer-or-marker side.
        if b.is_float() && !a.is_float() {
            let f = b.xfloat();
            if f.is_nan() {
                return Ok(None);
            }
            let ai = integer_or_marker_to_big(eval, a)?;
            return Ok(ai.partial_cmp(&f));
        }
        // Both are floats.
        return Ok(a.xfloat().partial_cmp(&b.xfloat()));
    }

    // Both operands are integer-or-marker. Stay on i64 if neither is
    // a bignum.
    if !a.is_bignum() && !b.is_bignum() {
        let ai = expect_integer_or_marker_after_number_check_eval(eval, a)?;
        let bi = expect_integer_or_marker_after_number_check_eval(eval, b)?;
        return Ok(Some(ai.cmp(&bi)));
    }

    // Bignum-aware integer compare.
    let ai = integer_or_marker_to_big(eval, a)?;
    let bi = integer_or_marker_to_big(eval, b)?;
    Ok(Some(ai.cmp(&bi)))
}

/// Materialize an exact integer (fixnum, bignum, or marker position) as
/// a `malachite::Integer`. Signals `number-or-marker-p` for anything
/// else (including floats — callers must handle the float side first).
/// Used by `arithcompare` so integer-vs-float comparisons stay exact
/// instead of lowering the integer through `f64` (GNU `arithcompare`,
/// data.c:2734-2845).
fn integer_or_marker_to_big(
    eval: &super::super::eval::Context,
    v: &Value,
) -> Result<Integer, Flow> {
    match v.kind() {
        ValueKind::Fixnum(n) => Ok(Integer::from(n)),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(v.as_bignum().unwrap().clone()),
        _ if super::marker::is_marker(v) => Ok(Integer::from(
            super::marker::marker_position_as_int_eval(eval, v)?,
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), *v],
        )),
    }
}

fn cmp_passes(ord: Option<std::cmp::Ordering>, op: NumCmp) -> bool {
    use std::cmp::Ordering;
    let Some(ord) = ord else {
        return op == NumCmp::Ne;
    };
    match op {
        NumCmp::Lt => ord == Ordering::Less,
        NumCmp::Le => ord != Ordering::Greater,
        NumCmp::Eq => ord == Ordering::Equal,
        NumCmp::Ne => ord != Ordering::Equal,
        NumCmp::Gt => ord == Ordering::Greater,
        NumCmp::Ge => ord != Ordering::Less,
    }
}

fn arithcompare_chain(
    eval: &super::super::eval::Context,
    args: &[Value],
    op: NumCmp,
) -> EvalResult {
    for pair in args.windows(2) {
        let ord = arithcompare(eval, &pair[0], &pair[1])?;
        if !cmp_passes(ord, op) {
            return Ok(Value::NIL);
        }
    }
    Ok(Value::T)
}

fn arithcompare_chain_or_fast_fixnum_pair(
    eval: &super::super::eval::Context,
    args: &[Value],
    op: NumCmp,
) -> EvalResult {
    if args.len() == 2
        && let (ValueKind::Fixnum(left), ValueKind::Fixnum(right)) =
            (args[0].kind(), args[1].kind())
    {
        return Ok(Value::bool_val(cmp_passes(Some(left.cmp(&right)), op)));
    }
    arithcompare_chain(eval, args, op)
}

pub(crate) fn builtin_num_eq_slice(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("=", args, 1)?;
    arithcompare_chain(eval, args, NumCmp::Eq)
}

pub(crate) fn builtin_num_lt_slice(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("<", args, 1)?;
    arithcompare_chain_or_fast_fixnum_pair(eval, args, NumCmp::Lt)
}

pub(crate) fn builtin_num_le_slice(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("<=", args, 1)?;
    arithcompare_chain_or_fast_fixnum_pair(eval, args, NumCmp::Le)
}

pub(crate) fn builtin_num_gt_slice(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args(">", args, 1)?;
    arithcompare_chain_or_fast_fixnum_pair(eval, args, NumCmp::Gt)
}

pub(crate) fn builtin_num_ge_slice(
    eval: &mut super::super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args(">=", args, 1)?;
    arithcompare_chain_or_fast_fixnum_pair(eval, args, NumCmp::Ge)
}

pub(crate) fn builtin_num_ne_2(
    eval: &mut super::super::eval::Context,
    left: Value,
    right: Value,
) -> EvalResult {
    if let (ValueKind::Fixnum(left), ValueKind::Fixnum(right)) = (left.kind(), right.kind()) {
        return Ok(Value::bool_val(left != right));
    }
    let ord = arithcompare(eval, &left, &right)?;
    Ok(Value::bool_val(cmp_passes(ord, NumCmp::Ne)))
}

// ===========================================================================
// Conversion
// ===========================================================================

pub(crate) fn builtin_float(args: Vec<Value>) -> EvalResult {
    expect_args("float", &args, 1)?;
    match args[0].kind() {
        ValueKind::Fixnum(n) => Ok(Value::make_float(n as f64)),
        ValueKind::Float => Ok(args[0]),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(Value::make_float(
            f64::rounding_from(args[0].as_bignum().unwrap(), RoundingMode::Nearest).0,
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), args[0]],
        )),
    }
}

/// Helper: extract a number as f64, signaling wrong-type-argument if not numeric.
fn value_to_f64(_name: &str, v: &Value) -> Result<f64, Flow> {
    match v.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(v.xfloat()),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            Ok(f64::rounding_from(v.as_bignum().unwrap(), RoundingMode::Nearest).0)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *v],
        )),
    }
}

/// Helper for 1-or-2-arg rounding functions.
/// When called with 2 args, divides first by second, then applies the rounding op.
/// For int/int with no remainder, returns integer directly.
///
/// Mirrors GNU `rounding_driver` (`src/floatfns.c`). The audit
/// (§2.15, §2.17) flagged that NeoMacs used to truncate float
/// results to i64 with `as i64` saturation, silently producing
/// `i64::MAX`/`i64::MIN` for out-of-range floats and not surfacing
/// overflow on infinity / NaN. We now route every integer result
/// through `Value::make_integer`, and floats outside i64 range use
/// `Integer::rounding_from` with `RoundingMode::Down` to produce a bignum.
fn rounding_with_divisor(
    name: &str,
    args: &[Value],
    round_fn: fn(f64) -> f64,
    int_div: fn(i64, i64) -> i64,
) -> EvalResult {
    expect_args_range(name, args, 1, 2)?;
    // GNU `rounding_driver` (`src/floatfns.c`) validates the numerator
    // before doing anything else.  It then treats a nil (or omitted)
    // divisor as the single-argument form, so cl-lib may safely forward an
    // unsupplied `&optional y` as nil.
    let _ = expect_number(&args[0])?;
    if args.len() == 1 || args[1].is_nil() {
        return match args[0].kind() {
            ValueKind::Fixnum(n) => Ok(Value::fixnum(n)),
            ValueKind::Float => float_to_lisp_integer(round_fn(args[0].xfloat())),
            ValueKind::Veclike(VecLikeType::Bignum) => {
                Ok(Value::make_integer(args[0].as_bignum().unwrap().clone()))
            }
            _ => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("numberp"), args[0]],
            )),
        };
    }
    // The non-nil divisor is likewise checked with GNU's `CHECK_NUMBER`
    // before integer/float dispatch.  Keeping this validation at the shared
    // boundary prevents an implementation-specific `integer-or-marker-p`
    // error from leaking out of the integer slow path.
    let _ = expect_number(&args[1])?;
    // 2-arg form: (op ARG DIVISOR)
    if args[1].is_float() {
        let divisor = args[1].xfloat();
        if divisor == 0.0 {
            return Err(signal(LispCondition::ArithError, vec![]));
        }
        let dividend = value_to_f64(name, &args[0])?;
        return float_to_lisp_integer(round_fn(dividend / divisor));
    }
    if let Some(d) = args[1].as_fixnum() {
        if d == 0 {
            return Err(signal(LispCondition::ArithError, vec![]));
        }
        if let Some(a) = args[0].as_fixnum() {
            return Ok(Value::make_int(int_div(a, d)));
        }
    }
    if args[1].is_bignum() && *args[1].as_bignum().unwrap() == 0 {
        return Err(signal(LispCondition::ArithError, vec![]));
    }
    // Mixed bignum / float / fixnum 2-arg fallback. For non-float
    // operands fall through to the float path; this loses precision
    // for very large bignums but matches the existing behavior for
    // the cases the test suite covers. A future pass can wire in
    // mpz_tdiv_q etc. for full bignum-divisor support.
    if args[0].is_float() || args[1].is_float() {
        let dividend = value_to_f64(name, &args[0])?;
        let divisor = value_to_f64(name, &args[1])?;
        if divisor == 0.0 {
            return Err(signal(LispCondition::ArithError, vec![]));
        }
        return float_to_lisp_integer(round_fn(dividend / divisor));
    }
    // Bignum-divisor or bignum-dividend integer path: do GMP
    // truncation and reapply the rounding flavor on the residue.
    let a = bignum_or_int_to_integer(&args[0])?;
    let d = bignum_or_int_to_integer(&args[1])?;
    if d == 0 {
        return Err(signal(LispCondition::ArithError, vec![]));
    }
    // Truncation (toward-zero) division as the building block.
    let q = &a / &d;
    let r = &a - (&q * &d);
    // Apply the same flavor that the int_div lambda would for fixnums,
    // but in GMP. We dispatch by name because the closure type erases
    // intent — and there are only four flavors.
    let adjusted = match name {
        "truncate" => q,
        "floor" => {
            // Toward -inf: if remainder is nonzero and r and d have
            // opposite signs, subtract 1.
            if r != 0 && (r < 0) != (d < 0) {
                q - Integer::from(1)
            } else {
                q
            }
        }
        "ceiling" => {
            // Toward +inf: if remainder is nonzero and r and d have
            // the same sign, add 1.
            if r != 0 && (r < 0) == (d < 0) {
                q + Integer::from(1)
            } else {
                q
            }
        }
        "round" => {
            // Round half to even (banker's rounding).
            let abs_r2 = (&r * Integer::from(2)).abs();
            let abs_d = (&d).abs();
            use std::cmp::Ordering;
            match abs_r2.cmp(&abs_d) {
                Ordering::Greater => {
                    if (r < 0) == (d < 0) {
                        q + Integer::from(1)
                    } else {
                        q - Integer::from(1)
                    }
                }
                Ordering::Equal => {
                    if &q & Integer::from(1) != 0 {
                        if (r < 0) == (d < 0) {
                            q + Integer::from(1)
                        } else {
                            q - Integer::from(1)
                        }
                    } else {
                        q
                    }
                }
                Ordering::Less => q,
            }
        }
        _ => unreachable!("unknown rounding name {name}"),
    };
    Ok(Value::make_integer(adjusted))
}

/// Convert a finite f64 into a Lisp integer (fixnum or bignum). NaN
/// and infinity signal `overflow-error`, mirroring GNU
/// `double_to_integer` (`src/bignum.c:81`).
fn float_to_lisp_integer(value: f64) -> EvalResult {
    if !value.is_finite() {
        return Err(signal(LispCondition::OverflowError, vec![]));
    }
    // i64::MIN..=i64::MAX is the safe `as i64` range; outside that we
    // need a bignum. But fixnum range is even tighter (62-bit), so always
    // funnel through make_integer. Truncate toward zero (Down).
    let big = Integer::rounding_from(value, RoundingMode::Down).0;
    Ok(Value::make_integer(big))
}

pub(crate) fn builtin_truncate(args: Vec<Value>) -> EvalResult {
    rounding_with_divisor(
        "truncate",
        &args,
        |f| f.trunc(),
        |a, d| {
            // Truncation: toward zero
            a / d
        },
    )
}

pub(crate) fn builtin_floor(args: Vec<Value>) -> EvalResult {
    rounding_with_divisor(
        "floor",
        &args,
        |f| f.floor(),
        |a, d| {
            // Floor division: toward negative infinity
            let q = a / d;
            let r = a % d;
            if (r != 0) && ((r ^ d) < 0) { q - 1 } else { q }
        },
    )
}

pub(crate) fn builtin_ceiling(args: Vec<Value>) -> EvalResult {
    rounding_with_divisor(
        "ceiling",
        &args,
        |f| f.ceil(),
        |a, d| {
            // Ceiling division: toward positive infinity
            let q = a / d;
            let r = a % d;
            if (r != 0) && ((r ^ d) >= 0) { q + 1 } else { q }
        },
    )
}

pub(crate) fn builtin_round(args: Vec<Value>) -> EvalResult {
    rounding_with_divisor(
        "round",
        &args,
        |f| f.round_ties_even(),
        |a, d| {
            // Banker's rounding (round half to even)
            let q = a / d;
            let r = a % d;
            let abs_r2 = (r * 2).abs();
            let abs_d = d.abs();
            if abs_r2 > abs_d {
                if (r ^ d) >= 0 { q + 1 } else { q - 1 }
            } else if abs_r2 == abs_d {
                // Tie: round to even
                if q % 2 != 0 {
                    if (r ^ d) >= 0 { q + 1 } else { q - 1 }
                } else {
                    q
                }
            } else {
                q
            }
        },
    )
}

// ===========================================================================
// Math functions (pure)
// ===========================================================================

pub(crate) fn builtin_sqrt(args: Vec<Value>) -> EvalResult {
    expect_args("sqrt", &args, 1)?;
    Ok(Value::make_float(expect_number(&args[0])?.sqrt()))
}

pub(crate) fn builtin_sin(args: Vec<Value>) -> EvalResult {
    expect_args("sin", &args, 1)?;
    Ok(Value::make_float(expect_number(&args[0])?.sin()))
}

pub(crate) fn builtin_cos(args: Vec<Value>) -> EvalResult {
    expect_args("cos", &args, 1)?;
    Ok(Value::make_float(expect_number(&args[0])?.cos()))
}

pub(crate) fn builtin_tan(args: Vec<Value>) -> EvalResult {
    expect_args("tan", &args, 1)?;
    Ok(Value::make_float(expect_number(&args[0])?.tan()))
}

pub(crate) fn builtin_asin(args: Vec<Value>) -> EvalResult {
    expect_args("asin", &args, 1)?;
    Ok(Value::make_float(expect_number(&args[0])?.asin()))
}

pub(crate) fn builtin_acos(args: Vec<Value>) -> EvalResult {
    expect_args("acos", &args, 1)?;
    Ok(Value::make_float(expect_number(&args[0])?.acos()))
}

pub(crate) fn builtin_atan(args: Vec<Value>) -> EvalResult {
    expect_min_args("atan", &args, 1)?;
    if args.len() == 2 {
        let y = expect_number(&args[0])?;
        let x = expect_number(&args[1])?;
        Ok(Value::make_float(y.atan2(x)))
    } else {
        Ok(Value::make_float(expect_number(&args[0])?.atan()))
    }
}

pub(crate) fn builtin_exp(args: Vec<Value>) -> EvalResult {
    expect_args("exp", &args, 1)?;
    Ok(Value::make_float(expect_number(&args[0])?.exp()))
}

pub(crate) fn builtin_log(args: Vec<Value>) -> EvalResult {
    expect_min_args("log", &args, 1)?;
    let val = expect_number(&args[0])?;
    if args.len() == 2 {
        let base = expect_number(&args[1])?;
        let result = if base == 10.0 {
            val.log10()
        } else if base == 2.0 {
            val.log2()
        } else {
            val.ln() / base.ln()
        };
        Ok(Value::make_float(result))
    } else {
        Ok(Value::make_float(val.ln()))
    }
}

/// `(expt BASE EXPONENT)` — mirrors GNU `Fexpt`
/// (`src/floatfns.c`) and `expt_integer` (`src/data.c:3587`).
///
/// Integer base + non-negative integer exponent uses `mpz_pow_ui` to
/// promote on overflow. The headline audit case is `(expt 2 100)`
/// which used to return 0 because `2_i64.wrapping_pow(100)` wraps.
pub(crate) fn builtin_expt(args: Vec<Value>) -> EvalResult {
    expect_args("expt", &args, 2)?;
    // GNU `Fexpt` (data.c) does CHECK_NUMBER on both args first, so any
    // non-numeric argument must signal `numberp`, not the more specific
    // type checks the integer/float dispatch would otherwise emit.
    let _ = expect_number(&args[0])?;
    let _ = expect_number(&args[1])?;
    if has_float(&args) {
        let base = expect_number(&args[0])?;
        let exp = expect_number(&args[1])?;
        return Ok(Value::make_float(base.powf(exp)));
    }
    // Integer-only path. Negative exponent on integer base falls back
    // to float (GNU does the same: a^-n is rarely an integer).
    let exp_val = &args[1];
    let exp_is_neg = match exp_val.kind() {
        ValueKind::Fixnum(n) => n < 0,
        ValueKind::Veclike(VecLikeType::Bignum) => *exp_val.as_bignum().unwrap() < 0,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), *exp_val],
            ));
        }
    };
    if exp_is_neg {
        let base = expect_number(&args[0])?;
        let exp = expect_number(exp_val)?;
        return Ok(Value::make_float(base.powf(exp)));
    }

    // Special cases for -1, 0, 1 — never overflow regardless of exponent.
    let base_val = &args[0];
    if let Some(b) = base_val.as_fixnum() {
        match b {
            0 => {
                // 0^0 = 1 in elisp, 0^positive = 0.
                let exp_zero = match exp_val.kind() {
                    ValueKind::Fixnum(n) => n == 0,
                    ValueKind::Veclike(VecLikeType::Bignum) => *exp_val.as_bignum().unwrap() == 0,
                    _ => false,
                };
                return Ok(Value::fixnum(if exp_zero { 1 } else { 0 }));
            }
            1 => return Ok(Value::fixnum(1)),
            -1 => {
                let odd = match exp_val.kind() {
                    ValueKind::Fixnum(n) => n & 1 == 1,
                    ValueKind::Veclike(VecLikeType::Bignum) => {
                        exp_val.as_bignum().unwrap() & Integer::from(1) != 0
                    }
                    _ => false,
                };
                return Ok(Value::fixnum(if odd { -1 } else { 1 }));
            }
            _ => {}
        }
    }

    // Exponent must fit in u64 for Integer::pow. (GNU bounds it
    // by ULONG_MAX; that's larger than u64 on most platforms but the
    // result becomes astronomically large long before then.)
    let exp_u64: u64 = match exp_val.kind() {
        ValueKind::Fixnum(n) => match u64::try_from(n) {
            Ok(v) => v,
            Err(_) => return Err(signal(LispCondition::OverflowError, vec![])),
        },
        ValueKind::Veclike(VecLikeType::Bignum) => {
            match u64::try_from(exp_val.as_bignum().unwrap()) {
                Ok(v) => v,
                Err(_) => return Err(signal(LispCondition::OverflowError, vec![])),
            }
        }
        _ => unreachable!("non-int exponent handled above"),
    };

    let base_big = bignum_or_int_to_integer(base_val)?;
    Ok(Value::make_integer(base_big.pow(exp_u64)))
}

pub(crate) fn builtin_random(args: Vec<Value>) -> EvalResult {
    expect_max_args("random", &args, 1)?;

    if let Some(limit) = args.first() {
        match limit.kind() {
            ValueKind::T => emacs_init_random(),
            ValueKind::String => {
                let bytes = limit.as_lisp_string().expect("string").as_bytes().to_vec();
                emacs_seed_random(&bytes);
            }
            ValueKind::Fixnum(lim) => {
                if lim <= 0 {
                    return Err(signal(LispCondition::ArgsOutOfRange, vec![*limit]));
                }
                return Ok(Value::fixnum(emacs_get_random_fixnum(lim)));
            }
            _ => {}
        }
    }

    Ok(Value::fixnum(emacs_get_random()))
}

fn emacs_random_lock() -> &'static Mutex<()> {
    static RANDOM_LOCK: Mutex<()> = Mutex::new(());
    &RANDOM_LOCK
}

fn emacs_intmask() -> u64 {
    (1_u64 << emacs_random_fixnum_bits()) - 1
}

fn emacs_random_fixnum_bits() -> u32 {
    62
}

fn emacs_get_random_unlocked() -> i64 {
    const RAND_BITS: u32 = 31;
    const EMACS_INT_WIDTH: u32 = 64;
    let fixnum_bits = emacs_random_fixnum_bits();
    let mut val: u64 = 0;
    for _ in 0..fixnum_bits.div_ceil(RAND_BITS) {
        let r = platform_random_word();
        val = r ^ (val << RAND_BITS) ^ (val >> (EMACS_INT_WIDTH - RAND_BITS));
    }
    val ^= val >> (EMACS_INT_WIDTH - fixnum_bits);
    (val & emacs_intmask()) as i64
}

pub(crate) fn emacs_get_random() -> i64 {
    let _guard = emacs_random_lock().lock().expect("random lock poisoned");
    emacs_get_random_unlocked()
}

fn emacs_get_random_fixnum(limit: i64) -> i64 {
    let lim = limit as u64;
    let intmask = emacs_intmask();
    let difflim = intmask - lim + 1;
    let _guard = emacs_random_lock().lock().expect("random lock poisoned");
    loop {
        let r = emacs_get_random_unlocked() as u64;
        let remainder = r % lim;
        let diff = r - remainder;
        if difflim >= diff {
            return remainder as i64;
        }
    }
}

fn emacs_seed_random(seed: &[u8]) {
    let _guard = emacs_random_lock().lock().expect("random lock poisoned");
    let mut arg = 0u32;
    for (index, byte) in seed.iter().enumerate() {
        arg ^= u32::from(*byte) << ((index % 4) * 8);
    }
    platform_seed_random(arg);
}

fn emacs_init_random() {
    let seed = crate::host::process::id()
        ^ (crate::host::time::wall_time_since_unix_epoch()
            .map(|d| (d.as_secs() as u32) ^ d.subsec_nanos())
            .unwrap_or(0));
    emacs_seed_random(&seed.to_ne_bytes());
}

#[cfg(unix)]
fn platform_seed_random(seed: u32) {
    // GNU sysdep.c uses srandom/random when HAVE_RANDOM is available, with
    // `unsigned int random_seed`.  Unix platforms we support provide that API.
    unsafe { c_random::srandom(seed as libc::c_uint) };
}

#[cfg(unix)]
fn platform_random_word() -> u64 {
    unsafe { c_random::random() as u64 }
}

#[cfg(unix)]
mod c_random {
    unsafe extern "C" {
        pub(super) fn srandom(seed: libc::c_uint);
        pub(super) fn random() -> libc::c_long;
    }
}

#[cfg(not(unix))]
mod fallback_random {
    use std::cell::Cell;

    thread_local! {
        static RANDOM_STATE: Cell<u32> = const { Cell::new(0x1234_5678) };
    }

    pub(super) fn seed(seed: u32) {
        RANDOM_STATE.with(|state| state.set(seed));
    }

    pub(super) fn next() -> u64 {
        RANDOM_STATE.with(|state| {
            let next = state.get().wrapping_mul(1103515245).wrapping_add(12345);
            state.set(next);
            u64::from((next >> 16) & 0x7fff)
        })
    }
}

#[cfg(not(unix))]
fn platform_seed_random(seed: u32) {
    fallback_random::seed(seed);
}

#[cfg(not(unix))]
fn platform_random_word() -> u64 {
    fallback_random::next()
}

pub(crate) fn builtin_isnan(args: Vec<Value>) -> EvalResult {
    expect_args("isnan", &args, 1)?;
    match args[0].kind() {
        ValueKind::Float => Ok(Value::bool_val(args[0].xfloat().is_nan())),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("floatp"), args[0]],
        )),
    }
}

#[cfg(test)]
#[path = "tests/arithmetic_minmax_compare.rs"]
mod arithmetic_minmax_compare_test;

#[cfg(test)]
#[path = "tests/arithmetic_rounding_nil_divisor.rs"]
mod arithmetic_rounding_nil_divisor_test;

#[cfg(test)]
#[path = "tests/arithmetic_ash_overflow.rs"]
mod arithmetic_ash_overflow_test;
