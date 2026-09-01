//! Float and math builtins for the Elisp interpreter.
//!
//! Implements all functions from Emacs `floatfns.c`:
//! - Classification: `copysign`, `frexp`, `ldexp`, `logb`
//! - Rounding (float result): `fceiling`, `ffloor`, `fround`, `ftruncate`

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args;
use crate::emacs_core::value::ValueKind;
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::rounding_modes::RoundingMode;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Extract a numeric argument as `f64` with `numberp` contract semantics.
///
/// Mirrors GNU `extract_float` (`floatfns.c:81`) which accepts any
/// `NUMBERP` value: fixnum, bignum, or float. Bignums are converted
/// to `f64` via `mpz_get_d`, matching GNU's `XFLOATINT` semantics
/// (precision loss is the caller's contract — `logb`, `frexp`, etc.
/// are inherently float operations).
fn extract_number(val: &Value) -> Result<f64, Flow> {
    use crate::emacs_core::value::VecLikeType;
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(val.xfloat()),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            Ok(f64::rounding_from(val.as_bignum().unwrap(), RoundingMode::Nearest).0)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *val],
        )),
    }
}

/// Extract a float argument with `floatp` contract semantics.
fn extract_float(val: &Value) -> Result<f64, Flow> {
    match val.kind() {
        ValueKind::Float => Ok(val.xfloat()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("floatp"), *val],
        )),
    }
}

/// Extract a fixnum argument with `fixnump` contract semantics.
fn extract_fixnum(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), *val],
        )),
    }
}

// ---------------------------------------------------------------------------
// Classification / special float operations
// ---------------------------------------------------------------------------

/// (copysign X1 X2) -- copy sign of X2 to magnitude of X1
pub(crate) fn builtin_copysign(args: Vec<Value>) -> EvalResult {
    expect_args("copysign", &args, 2)?;
    let x1 = extract_float(&args[0])?;
    let x2 = extract_float(&args[1])?;
    Ok(Value::make_float(x1.copysign(x2)))
}

/// (frexp X) -- return (SIGNIFICAND . EXPONENT) cons cell
///
/// Decomposes X into significand * 2^exponent where 0.5 <= |significand| < 1.
/// Uses the C `frexp` convention that Emacs follows.
pub(crate) fn builtin_frexp(args: Vec<Value>) -> EvalResult {
    expect_args("frexp", &args, 1)?;
    let x = extract_number(&args[0])?;

    if x == 0.0 {
        return Ok(Value::cons(Value::make_float(x), Value::fixnum(0)));
    }
    if x.is_nan() {
        return Ok(Value::cons(Value::make_float(x), Value::fixnum(0)));
    }
    if x.is_infinite() {
        return Ok(Value::cons(Value::make_float(x), Value::fixnum(0)));
    }

    // Rust doesn't have frexp in std, so we implement it manually.
    // frexp(x) returns (frac, exp) where x = frac * 2^exp, 0.5 <= |frac| < 1
    let bits = x.to_bits();
    let sign = bits >> 63;
    let exponent_bits = ((bits >> 52) & 0x7FF) as i64;
    let mantissa_bits = bits & 0x000F_FFFF_FFFF_FFFF;

    if exponent_bits == 0 {
        // Subnormal: normalize first
        let normalized = x * (1u64 << 52) as f64;
        let nbits = normalized.to_bits();
        let nexp = ((nbits >> 52) & 0x7FF) as i64;
        let nmant = nbits & 0x000F_FFFF_FFFF_FFFF;
        let exp = nexp - 1022 - 52;
        let frac_bits = (sign << 63) | (0x3FE << 52) | nmant;
        let frac = f64::from_bits(frac_bits);
        return Ok(Value::cons(Value::make_float(frac), Value::fixnum(exp)));
    }

    let exp = exponent_bits - 1022;
    let frac_bits = (sign << 63) | (0x3FE << 52) | mantissa_bits;
    let frac = f64::from_bits(frac_bits);
    Ok(Value::cons(Value::make_float(frac), Value::fixnum(exp)))
}

/// (ldexp SIGNIFICAND EXPONENT) -- return SIGNIFICAND * 2^EXPONENT
pub(crate) fn builtin_ldexp(args: Vec<Value>) -> EvalResult {
    expect_args("ldexp", &args, 2)?;
    let exponent = extract_fixnum(&args[1])?;
    let significand = extract_number(&args[0])?;

    // Use ldexp equivalent: significand * 2.0^exponent
    // Rust doesn't have ldexp in std, but we can use f64::exp2 approach
    // or simply multiply. For correctness with large exponents, we use
    // the powi approach clamped to avoid overflow in intermediate steps.
    let result = if (0..=1023).contains(&exponent) {
        significand * f64::from_bits(((exponent + 1023) as u64) << 52)
    } else if (-1074..0).contains(&exponent) {
        significand * 2.0f64.powi(exponent as i32)
    } else if exponent > 1023 {
        // Very large exponent: will be infinity for any non-zero significand
        if significand == 0.0 {
            0.0
        } else {
            significand * f64::INFINITY
        }
    } else {
        // Very small exponent: will be 0.0
        0.0
    };

    Ok(Value::make_float(result))
}

/// (logb X) -- integer part of base-2 logarithm of |X|
///
/// Returns the integer exponent from frexp, minus 1 (matching Emacs behavior).
/// For X = 0, signals a domain error (like Emacs).
pub(crate) fn builtin_logb(args: Vec<Value>) -> EvalResult {
    expect_args("logb", &args, 1)?;
    let x = extract_number(&args[0])?;

    if x == 0.0 {
        // Emacs returns -infinity as a float for logb(0)
        return Ok(Value::make_float(f64::NEG_INFINITY));
    }
    if x.is_infinite() {
        return Ok(Value::make_float(f64::INFINITY));
    }
    if x.is_nan() {
        return Ok(Value::make_float(x));
    }

    // logb returns floor(log2(|x|)) as an integer, which is the exponent
    // from frexp minus 1 (since frexp normalizes to [0.5, 1.0)).
    let abs_x = x.abs();
    let result = abs_x.log2().floor() as i64;
    Ok(Value::fixnum(result))
}

// ---------------------------------------------------------------------------
// Rounding to float
// ---------------------------------------------------------------------------

/// (fceiling X) -- smallest integer not less than X, as a float
pub(crate) fn builtin_fceiling(args: Vec<Value>) -> EvalResult {
    expect_args("fceiling", &args, 1)?;
    let x = extract_float(&args[0])?;
    Ok(Value::make_float(x.ceil()))
}

/// (ffloor X) -- largest integer not greater than X, as a float
pub(crate) fn builtin_ffloor(args: Vec<Value>) -> EvalResult {
    expect_args("ffloor", &args, 1)?;
    let x = extract_float(&args[0])?;
    Ok(Value::make_float(x.floor()))
}

/// (fround X) -- nearest integer to X, as a float (banker's rounding)
pub(crate) fn builtin_fround(args: Vec<Value>) -> EvalResult {
    expect_args("fround", &args, 1)?;
    let x = extract_float(&args[0])?;
    Ok(Value::make_float(x.round_ties_even()))
}

/// (ftruncate X) -- round X toward zero, as a float
pub(crate) fn builtin_ftruncate(args: Vec<Value>) -> EvalResult {
    expect_args("ftruncate", &args, 1)?;
    let x = extract_float(&args[0])?;
    Ok(Value::make_float(x.trunc()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
