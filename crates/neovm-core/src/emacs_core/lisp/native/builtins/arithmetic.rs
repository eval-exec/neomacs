use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use malachite::base::num::arithmetic::traits::{Abs, DivRound, Pow};
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::num::logic::traits::SignificantBits;
use malachite::base::rounding_modes::RoundingMode;
use malachite::integer::Integer;
use malachite::natural::Natural;
use std::mem::MaybeUninit;
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

/// GNU `CHECK_NUMBER`: signal `numberp` unless `value` is a number. Only the
/// type — `expect_number` also converts, and a bignum's conversion to a
/// double was 200 instructions thrown away by every caller that only wanted
/// the check.
#[inline]
fn check_number(value: &Value) -> Result<(), Flow> {
    if value.is_fixnum() || value.is_float() || value.is_bignum() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *value],
        ))
    }
}

// ---------------------------------------------------------------------------
// Limb kernels for the bignum paths
// ---------------------------------------------------------------------------
//
// malachite's entry points for these shapes do more than the result needs:
// `&x + &y` pushes its sum a limb at a time (13 instructions a limb on
// pidigits), a product by one limb reserves no room for its carry, and
// `div_round` computes a quotient AND a remainder with calloc'd scratch even
// to truncate. The kernels below write each result once, into a vector
// allocated at its final size. GMP's are hand-written assembly; the BMI2
// product and the carry chains here land within about 1.5x of them.

/// `dst = xs * m` over `xs.len()` limbs; returns the carry-out limb.
#[inline]
fn limbs_mul_limb_to(xs: &[u64], m: u64, dst: &mut [MaybeUninit<u64>]) -> u64 {
    let dst = &mut dst[..xs.len()];
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("bmi2") {
        // SAFETY: the CPU supports BMI2, checked just above.
        return unsafe { limbs_mul_limb_to_bmi2(xs, m, dst) };
    }
    limbs_mul_limb_to_generic(xs, m, dst, 0)
}

fn limbs_mul_limb_to_generic(
    xs: &[u64],
    m: u64,
    dst: &mut [MaybeUninit<u64>],
    mut carry: u64,
) -> u64 {
    for (d, &x) in dst.iter_mut().zip(xs) {
        let p = u128::from(x) * u128::from(m) + u128::from(carry);
        d.write(p as u64);
        carry = (p >> 64) as u64;
    }
    carry
}

/// Four independent `mulx` products, then ONE add-with-carry chain over the
/// low halves and the previous high halves (`mulx` leaves the flags alone),
/// as GMP's `mul_1` does: 5 instructions a limb against 8.5 for the u128
/// loop.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "bmi2")]
fn limbs_mul_limb_to_bmi2(xs: &[u64], m: u64, dst: &mut [MaybeUninit<u64>]) -> u64 {
    use std::arch::x86_64::{_addcarry_u64, _mulx_u64};
    let mut carry = 0u64;
    let mut xc = xs.chunks_exact(4);
    let mut dc = dst.chunks_exact_mut(4);
    for (x, d) in (&mut xc).zip(&mut dc) {
        let (mut h0, mut h1, mut h2, mut h3) = (0u64, 0u64, 0u64, 0u64);
        let l0 = _mulx_u64(x[0], m, &mut h0);
        let l1 = _mulx_u64(x[1], m, &mut h1);
        let l2 = _mulx_u64(x[2], m, &mut h2);
        let l3 = _mulx_u64(x[3], m, &mut h3);
        let (mut o0, mut o1, mut o2, mut o3) = (0u64, 0u64, 0u64, 0u64);
        let c = _addcarry_u64(0, l0, carry, &mut o0);
        let c = _addcarry_u64(c, l1, h0, &mut o1);
        let c = _addcarry_u64(c, l2, h1, &mut o2);
        let c = _addcarry_u64(c, l3, h2, &mut o3);
        // h3 <= m - 1, so the carry-in cannot overflow it.
        _addcarry_u64(c, h3, 0, &mut carry);
        d[0].write(o0);
        d[1].write(o1);
        d[2].write(o2);
        d[3].write(o3);
    }
    limbs_mul_limb_to_generic(xc.remainder(), m, dc.into_remainder(), carry)
}

/// `dst = xs + ys` over `xs.len()` limbs, `xs.len() >= ys.len()`; returns the
/// carry out of the top limb.
fn limbs_add_to(xs: &[u64], ys: &[u64], dst: &mut [MaybeUninit<u64>]) -> bool {
    debug_assert!(xs.len() >= ys.len() && dst.len() >= xs.len());
    let (lo, hi) = xs.split_at(ys.len());
    let (dlo, dhi) = dst[..xs.len()].split_at_mut(ys.len());
    let mut carry = limbs_add_same_to(lo, ys, dlo);
    for (d, &x) in dhi.iter_mut().zip(hi) {
        let (s, c) = x.overflowing_add(u64::from(carry));
        d.write(s);
        carry = c;
    }
    carry
}

#[cfg(target_arch = "x86_64")]
fn limbs_add_same_to(xs: &[u64], ys: &[u64], dst: &mut [MaybeUninit<u64>]) -> bool {
    use std::arch::x86_64::_addcarry_u64;
    let mut c = 0u8;
    let mut xc = xs.chunks_exact(4);
    let mut yc = ys.chunks_exact(4);
    let mut dc = dst.chunks_exact_mut(4);
    for ((x, y), d) in (&mut xc).zip(&mut yc).zip(&mut dc) {
        let (mut o0, mut o1, mut o2, mut o3) = (0u64, 0u64, 0u64, 0u64);
        c = _addcarry_u64(c, x[0], y[0], &mut o0);
        c = _addcarry_u64(c, x[1], y[1], &mut o1);
        c = _addcarry_u64(c, x[2], y[2], &mut o2);
        c = _addcarry_u64(c, x[3], y[3], &mut o3);
        d[0].write(o0);
        d[1].write(o1);
        d[2].write(o2);
        d[3].write(o3);
    }
    for ((&x, &y), d) in xc
        .remainder()
        .iter()
        .zip(yc.remainder())
        .zip(dc.into_remainder())
    {
        let mut o = 0u64;
        c = _addcarry_u64(c, x, y, &mut o);
        d.write(o);
    }
    c != 0
}

#[cfg(not(target_arch = "x86_64"))]
fn limbs_add_same_to(xs: &[u64], ys: &[u64], dst: &mut [MaybeUninit<u64>]) -> bool {
    let mut carry = false;
    for ((&x, &y), d) in xs.iter().zip(ys).zip(dst) {
        let (s1, c1) = x.overflowing_add(y);
        let (s2, c2) = s1.overflowing_add(u64::from(carry));
        d.write(s2);
        carry = c1 | c2;
    }
    carry
}

/// `dst = xs - ys` over `xs.len()` limbs, for `xs >= ys` as numbers (so no
/// borrow leaves the top limb).
fn limbs_sub_to(xs: &[u64], ys: &[u64], dst: &mut [MaybeUninit<u64>]) {
    debug_assert!(xs.len() >= ys.len() && dst.len() >= xs.len());
    let (lo, hi) = xs.split_at(ys.len());
    let (dlo, dhi) = dst[..xs.len()].split_at_mut(ys.len());
    let mut borrow = limbs_sub_same_to(lo, ys, dlo);
    for (d, &x) in dhi.iter_mut().zip(hi) {
        let (s, b) = x.overflowing_sub(u64::from(borrow));
        d.write(s);
        borrow = b;
    }
    debug_assert!(!borrow, "limbs_sub_to needs xs >= ys");
}

#[cfg(target_arch = "x86_64")]
fn limbs_sub_same_to(xs: &[u64], ys: &[u64], dst: &mut [MaybeUninit<u64>]) -> bool {
    use std::arch::x86_64::_subborrow_u64;
    let mut b = 0u8;
    let mut xc = xs.chunks_exact(4);
    let mut yc = ys.chunks_exact(4);
    let mut dc = dst.chunks_exact_mut(4);
    for ((x, y), d) in (&mut xc).zip(&mut yc).zip(&mut dc) {
        let (mut o0, mut o1, mut o2, mut o3) = (0u64, 0u64, 0u64, 0u64);
        b = _subborrow_u64(b, x[0], y[0], &mut o0);
        b = _subborrow_u64(b, x[1], y[1], &mut o1);
        b = _subborrow_u64(b, x[2], y[2], &mut o2);
        b = _subborrow_u64(b, x[3], y[3], &mut o3);
        d[0].write(o0);
        d[1].write(o1);
        d[2].write(o2);
        d[3].write(o3);
    }
    for ((&x, &y), d) in xc
        .remainder()
        .iter()
        .zip(yc.remainder())
        .zip(dc.into_remainder())
    {
        let mut o = 0u64;
        b = _subborrow_u64(b, x, y, &mut o);
        d.write(o);
    }
    b != 0
}

#[cfg(not(target_arch = "x86_64"))]
fn limbs_sub_same_to(xs: &[u64], ys: &[u64], dst: &mut [MaybeUninit<u64>]) -> bool {
    let mut borrow = false;
    for ((&x, &y), d) in xs.iter().zip(ys).zip(dst) {
        let (s1, b1) = x.overflowing_sub(y);
        let (s2, b2) = s1.overflowing_sub(u64::from(borrow));
        d.write(s2);
        borrow = b1 | b2;
    }
    borrow
}

/// A limb vector filled by `fill` over its first `len` limbs, with room for
/// one carry limb; `fill` returns that carry (0 for none).
fn natural_from_kernel(len: usize, fill: impl FnOnce(&mut [MaybeUninit<u64>]) -> u64) -> Natural {
    let mut out: Vec<u64> = Vec::with_capacity(len + 1);
    let carry = fill(&mut out.spare_capacity_mut()[..len]);
    // SAFETY: `fill` initialized the first `len` limbs.
    unsafe { out.set_len(len) };
    if carry != 0 {
        out.push(carry);
    }
    Natural::from_owned_limbs_asc(out)
}

/// `x * n` in one pass and one allocation, reading `x` by reference. GNU
/// multiplies by a fixnum with `mpz_mul_si` into a fresh result the same way;
/// seeding `Integer::from(1) *= x` first cloned `x`.
fn integer_mul_i64(x: &Integer, n: i64) -> Integer {
    if n == 0 || *x == 0 {
        return Integer::from(0);
    }
    let negative = (*x < 0) != (n < 0);
    let xs = x.unsigned_abs_ref().as_limbs_asc();
    let m = n.unsigned_abs();
    let magnitude = natural_from_kernel(xs.len(), |dst| limbs_mul_limb_to(xs, m, dst));
    Integer::from_sign_and_abs(!negative, magnitude)
}

/// `|a| + |b|`.
fn natural_add_limbs(a: &[u64], b: &[u64]) -> Natural {
    let (xs, ys) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    natural_from_kernel(xs.len(), |dst| u64::from(limbs_add_to(xs, ys, dst)))
}

/// `|a| - |b|` for `|a| >= |b|`.
fn natural_sub_limbs(a: &[u64], b: &[u64]) -> Natural {
    natural_from_kernel(a.len(), |dst| {
        limbs_sub_to(a, b, dst);
        0
    })
}

/// `a + b` if `b_negative` is `b`'s sign, `a - b` if it is the opposite:
/// one allocation, both operands by reference.
fn integer_add_signed(a: &Integer, b: &Natural, b_negative: bool) -> Integer {
    let an = a.unsigned_abs_ref();
    let a_negative = *a < 0;
    if a_negative == b_negative {
        return Integer::from_sign_and_abs(
            !a_negative,
            natural_add_limbs(an.as_limbs_asc(), b.as_limbs_asc()),
        );
    }
    match an.cmp(b) {
        std::cmp::Ordering::Equal => Integer::from(0),
        std::cmp::Ordering::Greater => Integer::from_sign_and_abs(
            !a_negative,
            natural_sub_limbs(an.as_limbs_asc(), b.as_limbs_asc()),
        ),
        std::cmp::Ordering::Less => Integer::from_sign_and_abs(
            !b_negative,
            natural_sub_limbs(b.as_limbs_asc(), an.as_limbs_asc()),
        ),
    }
}

/// `a + b` for two integers, by reference.
fn integer_add_ref(a: &Integer, b: &Integer) -> Integer {
    integer_add_signed(a, b.unsigned_abs_ref(), *b < 0)
}

/// `a - b` for two integers, by reference.
fn integer_sub_ref(a: &Integer, b: &Integer) -> Integer {
    integer_add_signed(a, b.unsigned_abs_ref(), *b > 0)
}

/// `floor (x / 2^k)` for a magnitude `x` of at most `k + 128` bits.
fn limbs_shr_to_u128(xs: &[u64], k: u64) -> u128 {
    let i = (k / 64) as usize;
    let b = (k % 64) as u32;
    let limb = |j: usize| u128::from(xs.get(j).copied().unwrap_or(0));
    let v = (limb(i) | (limb(i + 1) << 64)) >> b;
    if b == 0 {
        v
    } else {
        v | (limb(i + 2) << (128 - b))
    }
}

/// `floor (|n| / |d|)` when the top 128 bits of both operands PROVE it,
/// `None` otherwise (caller divides in full).
///
/// With `N = floor (n / 2^k)`, `D = floor (d / 2^k)`, `q = floor (N / D)` and
/// `R = N - q*D`: `n - q*d` lies strictly between `(R - q) * 2^k` and
/// `(R + 1) * 2^k <= d`. So `q <= floor (n / d)` always, and `q` IS the
/// quotient whenever `R >= q`. For the small quotients of `(truncate a b)`
/// on similar-sized bignums `R` is essentially a random remainder below a
/// divisor of at least 2^63, so the check practically always holds. GMP's
/// `mpn_div_q` takes the same shortcut from an approximate quotient.
fn natural_div_small_quotient(ns: &[u64], ds: &[u64]) -> Option<u128> {
    let bit_len = |xs: &[u64]| -> u64 {
        xs.last().map_or(0, |&top| {
            xs.len() as u64 * 64 - u64::from(top.leading_zeros())
        })
    };
    let n_bits = bit_len(ns);
    let d_bits = bit_len(ds);
    if d_bits == 0 {
        return None;
    }
    if n_bits < d_bits {
        return Some(0);
    }
    let k = n_bits.saturating_sub(128);
    let n_top = limbs_shr_to_u128(ns, k);
    let d_top = limbs_shr_to_u128(ds, k);
    if d_top == 0 {
        return None;
    }
    let q = n_top / d_top;
    let r = n_top - q * d_top;
    // With k == 0 nothing was truncated: the division was exact.
    (k == 0 || r >= q).then_some(q)
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
            // GNU's `arith_driver` seeds the accumulator with `args[0]`, not
            // with the identity, and for a SIGNED ZERO that is observable:
            // `0.0 + -0.0` is `+0.0`, so seeding with 0 turned
            // `(+ -0.0 -0.0)` into `0.0` where GNU gives `-0.0`. When this is
            // the first operand there is nothing accumulated yet, so start
            // from it directly.
            let acc = if i == 0 {
                a.xfloat()
            } else {
                sum as f64 + a.xfloat()
            };
            return continue_float_add(eval, &args[i + 1..], acc);
        }
        if let Some(big) = a.as_bignum() {
            // Read the bignum by reference: `Integer::from(sum) += big`
            // cloned it (an allocation and a copy of every limb) only to add
            // to it. Two bignums in a row add in one pass into one result.
            let rest = &args[i + 1..];
            if sum == 0
                && let Some(next) = rest.first().and_then(|v| v.as_bignum())
            {
                return continue_bignum_add(eval, &rest[1..], integer_add_ref(big, next));
            }
            let acc = if sum == 0 {
                big.clone()
            } else {
                big + Integer::from(sum)
            };
            return continue_bignum_add(eval, rest, acc);
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
        match a.kind() {
            ValueKind::Fixnum(n) => acc += Integer::from(n),
            // By reference: `integer_from_value` would clone it to read it.
            ValueKind::Veclike(VecLikeType::Bignum) => {
                acc = integer_add_ref(&acc, a.as_bignum().unwrap());
            }
            _ => acc += integer_from_value(eval, a)?,
        }
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
        // By reference, as in `builtin_add_slice`: the next integer operand
        // is subtracted into the one result instead of into a clone.
        let rest = &args[1..];
        match rest.first().map(|v| v.kind()) {
            Some(ValueKind::Fixnum(n)) => {
                return continue_bignum_sub(eval, &rest[1..], big - Integer::from(n));
            }
            Some(ValueKind::Veclike(VecLikeType::Bignum)) => {
                return continue_bignum_sub(
                    eval,
                    &rest[1..],
                    integer_sub_ref(big, rest[0].as_bignum().unwrap()),
                );
            }
            _ => return continue_bignum_sub(eval, rest, big.clone()),
        }
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
        match a.kind() {
            ValueKind::Fixnum(n) => acc -= Integer::from(n),
            ValueKind::Veclike(VecLikeType::Bignum) => {
                acc = integer_sub_ref(&acc, a.as_bignum().unwrap());
            }
            _ => acc -= integer_from_value(eval, a)?,
        }
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
/// Takes a SLICE, not a `Vec`: it only reads its arguments, and the VM's
/// stack dispatcher has to materialize an owned vector for a `Many` subr. On
/// `nbody` that was a malloc and a free for a million multiplications.
pub(crate) fn builtin_mul(args: &[Value]) -> EvalResult {
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
            // `Integer::from(prod) *= big` cloned the bignum before the real
            // multiplication; multiply by reference into one result instead.
            let rest = &args[i + 1..];
            if prod != 1 {
                return continue_bignum_mul(rest, integer_mul_i64(big, prod));
            }
            match rest.first().map(|v| v.kind()) {
                Some(ValueKind::Fixnum(n)) => {
                    return continue_bignum_mul(&rest[1..], integer_mul_i64(big, n));
                }
                Some(ValueKind::Veclike(VecLikeType::Bignum)) => {
                    return continue_bignum_mul(&rest[1..], big * rest[0].as_bignum().unwrap());
                }
                _ => return continue_bignum_mul(rest, big.clone()),
            }
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
            ValueKind::Fixnum(n) => acc = integer_mul_i64(&acc, n),
            ValueKind::Veclike(VecLikeType::Bignum) => acc *= a.as_bignum().unwrap(),
            _ if super::marker::is_marker(a) => {
                acc = integer_mul_i64(&acc, super::marker::marker_position_as_int(a)?);
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
/// Slice-taking for the same reason as [`builtin_mul`].
pub(crate) fn builtin_div(args: &[Value]) -> EvalResult {
    expect_min_args("/", args, 1)?;
    // Single argument: return 1 / arg (reciprocal), matching GNU Emacs.
    if args.len() == 1 {
        return div_one_arg(&args[0]);
    }
    if has_float(args) {
        let mut acc = expect_number_or_marker_f64(&args[0])?;
        for a in &args[1..] {
            let d = expect_number_or_marker_f64(a)?;
            let inputs_ordered = !acc.is_nan() && !d.is_nan();
            acc /= d;
            if acc.is_nan() && inputs_ordered {
                // An INVALID operation (`0.0/0.0`, `inf/inf`) yields the
                // hardware default NaN, which is NEGATIVE on x86 and is what
                // GNU prints (`-0.0e+NaN`) — pin it against const-folding.
                // A NaN INPUT propagates with its own sign: GNU's
                // `(/ 0.0e+NaN 1)` is `0.0e+NaN`. Forcing every NaN negative
                // diverged there (found by the mixed-operand oracle).
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
            // We have bi.partial_cmp(f); reverse to get a.cmp(b).
            if let Some(bi) = b.as_bignum() {
                return Ok(bi.partial_cmp(&f).map(|o| o.reverse()));
            }
            let bi = integer_or_marker_to_big(eval, b)?;
            return Ok(bi.partial_cmp(&f).map(|o| o.reverse()));
        }
        // b is the float side, a is the integer-or-marker side.
        if b.is_float() && !a.is_float() {
            let f = b.xfloat();
            if f.is_nan() {
                return Ok(None);
            }
            if let Some(ai) = a.as_bignum() {
                return Ok(ai.partial_cmp(&f));
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

    // Bignum-aware integer compare, reading bignums by reference (cloning
    // both operands cost two allocations and two limb copies per `<`).
    // Exactly one side may be a non-bignum here.
    Ok(Some(match (a.as_bignum(), b.as_bignum()) {
        (Some(ai), Some(bi)) => ai.cmp(bi),
        (Some(ai), None) => {
            let bi = expect_integer_or_marker_after_number_check_eval(eval, b)?;
            ai.partial_cmp(&bi).expect("integers are totally ordered")
        }
        (None, Some(bi)) => {
            let ai = expect_integer_or_marker_after_number_check_eval(eval, a)?;
            bi.partial_cmp(&ai)
                .expect("integers are totally ordered")
                .reverse()
        }
        (None, None) => unreachable!("the fixnum case returned above"),
    }))
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
    check_number(&args[0])?;
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
    check_number(&args[1])?;
    // 2-arg form: (op ARG DIVISOR)
    if args[1].is_float() && args[1].xfloat() == 0.0 {
        return Err(signal(LispCondition::ArithError, vec![]));
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
    // The four flavors, as `int_div` is for fixnums: `round` is half to
    // even (malachite's `Nearest`, GNU's `rounddiv_q`).
    let mode = match name {
        "truncate" => RoundingMode::Down,
        "floor" => RoundingMode::Floor,
        "ceiling" => RoundingMode::Ceiling,
        "round" => RoundingMode::Nearest,
        _ => unreachable!("unknown rounding name {name}"),
    };
    if args[0].is_float() || args[1].is_float() {
        return rounding_float_exact(&args[0], &args[1], mode);
    }
    // Bignum-divisor or bignum-dividend integer path, GNU's `rounddiv_q` and
    // friends: one division in the requested rounding mode. Bignums are read
    // by reference; this used to clone both operands, divide toward zero,
    // and then compute the remainder `a - q*d` (a multiplication and a
    // subtraction the width of the operands) even for `truncate`, which
    // never looks at it.
    let a_small;
    let a: &Integer = match args[0].as_bignum() {
        Some(big) => big,
        None => {
            a_small = bignum_or_int_to_integer(&args[0])?;
            &a_small
        }
    };
    let d_small;
    let d: &Integer = match args[1].as_bignum() {
        Some(big) => big,
        None => {
            d_small = bignum_or_int_to_integer(&args[1])?;
            &d_small
        }
    };
    if *d == 0 {
        return Err(signal(LispCondition::ArithError, vec![]));
    }
    // A rounding that is truncation for this sign pair needs only the
    // quotient's magnitude, which the top 128 bits usually prove.
    let negative = (*a < 0) != (*d < 0);
    let truncating = match mode {
        RoundingMode::Down => true,
        RoundingMode::Floor => !negative,
        RoundingMode::Ceiling => negative,
        _ => false,
    };
    if truncating
        && let Some(q) = natural_div_small_quotient(
            a.unsigned_abs_ref().as_limbs_asc(),
            d.unsigned_abs_ref().as_limbs_asc(),
        )
    {
        let q = Integer::from(q);
        return Ok(Value::make_integer(if negative { -q } else { q }));
    }
    Ok(Value::make_integer(a.div_round(d, mode).0))
}

/// GNU `double_integer_scale` (`src/floatfns.c`) for IEEE doubles: the
/// power of two that scales `d` to an integer with `d`'s full precision —
/// `DBL_MANT_DIG - 1 - ilogb (d)` — or 1074 for zero and subnormals, 1075
/// for an infinity, 1076 for a NaN.
fn double_integer_scale(d: f64) -> i32 {
    const MAX_SCALE: i32 = 1074; // DBL_MANT_DIG - DBL_MIN_EXP
    if d.is_nan() {
        return MAX_SCALE + 2;
    }
    if d.is_infinite() {
        return MAX_SCALE + 1;
    }
    let biased = ((d.to_bits() >> 52) & 0x7ff) as i32;
    if biased == 0 {
        // Zero or subnormal: `ilogb` is below `DBL_MIN_EXP - 1`.
        return MAX_SCALE;
    }
    // ilogb (d) = biased - 1023.
    52 - (biased - 1023)
}

/// A finite double times `2^double_integer_scale (d)`, exactly: its signed
/// 53-bit significand (GNU `mpz_set_d (scalbn (d, scale))`).
fn double_scaled_significand(d: f64) -> Integer {
    let bits = d.to_bits();
    let fraction = bits & ((1u64 << 52) - 1);
    let significand = if (bits >> 52) & 0x7ff == 0 {
        fraction
    } else {
        fraction | (1u64 << 52)
    };
    let magnitude = Integer::from(significand);
    if bits >> 63 == 1 {
        -magnitude
    } else {
        magnitude
    }
}

/// The two-argument rounding functions with a float operand, exactly as GNU
/// `rounding_driver` computes them: scale both operands by the same power
/// of two until both are integers, then divide in the requested mode. The
/// quotient of the DOUBLES was wrong whenever it is not representable:
/// `(truncate 1e19 3)` is 3333333333333333333, not 3333333333333333504. The
/// divisor has already been checked non-zero.
fn rounding_float_exact(n: &Value, d: &Value, mode: RoundingMode) -> EvalResult {
    const MAX_SCALE: i32 = 1074;
    let nscale = if n.is_float() {
        double_integer_scale(n.xfloat())
    } else {
        0
    };
    let dscale = if d.is_float() {
        double_integer_scale(d.xfloat())
    } else {
        0
    };
    // A finite numerator over an infinite denominator: the quotient is zero.
    if dscale == MAX_SCALE + 1 && nscale < dscale {
        return Ok(Value::fixnum(0));
    }
    let rescale = |v: &Value, own: i32| -> Result<Integer, Flow> {
        if v.is_float() {
            // An infinity or a NaN has no integer scaling.
            if own > MAX_SCALE {
                return Err(signal(LispCondition::OverflowError, vec![]));
            }
            Ok(double_scaled_significand(v.xfloat()))
        } else {
            bignum_or_int_to_integer(v)
        }
    };
    let scale = nscale.max(dscale);
    let n_int = rescale(n, nscale)? << ((scale - nscale) as u64);
    let d_int = rescale(d, dscale)? << ((scale - dscale) as u64);
    Ok(Value::make_integer(n_int.div_round(d_int, mode).0))
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

/// `sqrt` as a one-slot subr: no argument vector per call.
pub(crate) fn builtin_sqrt_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::make_float(expect_number(&arg)?.sqrt()))
}

/// `sin` as a one-slot subr: no argument vector per call.
pub(crate) fn builtin_sin_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::make_float(expect_number(&arg)?.sin()))
}

/// `cos` as a one-slot subr: no argument vector per call.
pub(crate) fn builtin_cos_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::make_float(expect_number(&arg)?.cos()))
}

/// `tan` as a one-slot subr: no argument vector per call.
pub(crate) fn builtin_tan_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::make_float(expect_number(&arg)?.tan()))
}

/// `asin` as a one-slot subr: no argument vector per call.
pub(crate) fn builtin_asin_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::make_float(expect_number(&arg)?.asin()))
}

/// `acos` as a one-slot subr: no argument vector per call.
pub(crate) fn builtin_acos_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::make_float(expect_number(&arg)?.acos()))
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

/// `exp` as a one-slot subr: no argument vector per call.
pub(crate) fn builtin_exp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::make_float(expect_number(&arg)?.exp()))
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
    check_number(&args[0])?;
    check_number(&args[1])?;
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

#[cfg(test)]
#[path = "tests/arithmetic_bignum_borrowed.rs"]
mod arithmetic_bignum_borrowed_test;

#[cfg(test)]
#[path = "tests/arithmetic_rounding_float_exact.rs"]
mod arithmetic_rounding_float_exact_test;

#[cfg(test)]
#[path = "tests/arithmetic_limb_kernels.rs"]
mod arithmetic_limb_kernels_test;
