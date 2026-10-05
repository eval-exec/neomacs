//! Bool-vectors: GNU `PVEC_BOOL_VECTOR` (`lisp.h:1805-1847`,
//! `alloc.c:2126-2200`, `data.c:3709-4016`).
//!
//! A bool-vector is a [`BoolVectorObj`] (`VecLikeType::BoolVector`): `nbits`
//! bits packed into words, trailing bits zero. (The older in-band encoding,
//! an ordinary vector `[--bool-vector-- N 0/1 ...]`, is gone: a vector whose
//! slot 0 happens to be that symbol is a plain vector, as in GNU.)
//!
//! Operations follow GNU exactly: `wrong-length-argument` data, the
//! destination argument of the set operations ("the destination if it
//! changed, else nil"), and `bool-vector-not`'s unconditional destination.

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::tagged::header::BoolVectorObj;

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// A read view of a bool-vector.
#[derive(Clone, Copy)]
pub(crate) struct BoolVectorView<'a>(&'a BoolVectorObj);

impl<'a> BoolVectorView<'a> {
    /// The view of `value`, or `None` when it is not a bool-vector.
    #[inline]
    pub(crate) fn of(value: &Value) -> Option<BoolVectorView<'static>> {
        value.as_bool_vector_obj().map(BoolVectorView)
    }

    /// The number of bits.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.0.nbits
    }

    /// Bit `index` (`index < len`).
    #[inline]
    pub(crate) fn get(&self, index: usize) -> bool {
        self.0.get(index)
    }

    /// The bits as words (bit `i` in word `i / 64` at bit `i % 64`),
    /// trailing bits zero.
    #[inline]
    pub(crate) fn words(&self) -> &'a [u64] {
        self.0.words()
    }

    /// GNU's byte `index` of the bit data (the printer's and `sxhash`'s
    /// host-independent view).
    #[inline]
    pub(crate) fn byte(&self, index: usize) -> u8 {
        self.0.byte(index)
    }
}

/// Is `value` a bool-vector?
#[inline]
pub(crate) fn is_bool_vector(value: &Value) -> bool {
    value.is_bool_vector_obj()
}

/// The bit count of a bool-vector, or `None` for anything else.
#[inline]
pub(crate) fn bool_vector_length(value: &Value) -> Option<i64> {
    BoolVectorView::of(value).map(|view| view.len() as i64)
}

/// Bit `index` as GNU's `bool_vector_ref` exposes it (`t`/`nil`), or `None`
/// when `value` is not a bool-vector or `index` is out of range.
#[inline]
pub(crate) fn bool_vector_ref_value(value: &Value, index: usize) -> Option<Value> {
    let view = BoolVectorView::of(value)?;
    (index < view.len()).then(|| Value::bool_val(view.get(index)))
}

/// Set bit `index` of the bool-vector `value` in place. `false` when `value`
/// is not a bool-vector or `index` is out of range (nothing stored).
pub(crate) fn bool_vector_set(value: &Value, index: usize, bit: bool) -> bool {
    value
        .with_bool_vector_mut(|obj| {
            if index < obj.nbits {
                obj.set(index, bit);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}

/// Overwrite every bit of the bool-vector `dest` (of `words.len()` words'
/// worth of bits) from `words`, in place.
fn store_words(dest: &Value, words: &[u64]) {
    let _ = dest.with_bool_vector_mut(|obj| {
        obj.words_mut().copy_from_slice(words);
        obj.clear_trailing_bits();
    });
}

/// Fill every bit of the bool-vector `value` with `bit` (GNU
/// `bool_vector_fill`). `false` when `value` is not a bool-vector.
pub(crate) fn bool_vector_fill(value: &Value, bit: bool) -> bool {
    let Some(view) = BoolVectorView::of(value) else {
        return false;
    };
    let nbits = view.len();
    let pattern = if bit { u64::MAX } else { 0 };
    let words = vec![pattern; BoolVectorObj::words_for(nbits)];
    store_words(value, &words);
    true
}

/// The bits as `t`/`nil`, in order: the elements `vconcat`, `append`,
/// `elt` and the mapping functions see. `None` for a non-bool-vector.
pub(crate) fn bool_vector_elements(value: &Value) -> Option<Vec<Value>> {
    let view = BoolVectorView::of(value)?;
    Some(
        (0..view.len())
            .map(|i| Value::bool_val(view.get(i)))
            .collect(),
    )
}

/// A new bool-vector with the bits of `value` in reverse order (GNU
/// `Freverse`). `None` for a non-bool-vector.
pub(crate) fn reverse_bool_vector(value: &Value) -> Option<Value> {
    let view = BoolVectorView::of(value)?;
    let nbits = view.len();
    let bits: Vec<bool> = (0..nbits).rev().map(|i| view.get(i)).collect();
    Some(bool_vector_from_bits(&bits))
}

/// Reverse the bits of `value` in place (GNU `Fnreverse`). `false` for a
/// non-bool-vector.
pub(crate) fn nreverse_bool_vector(value: &Value) -> bool {
    let Some(view) = BoolVectorView::of(value) else {
        return false;
    };
    let nbits = view.len();
    let mut words = vec![0u64; BoolVectorObj::words_for(nbits)];
    for (to, from) in (0..nbits).rev().enumerate() {
        if view.get(from) {
            words[to / BoolVectorObj::WORD_BITS] |= 1u64 << (to % BoolVectorObj::WORD_BITS);
        }
    }
    store_words(value, &words);
    true
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

/// A new bool-vector of `nbits` bits from `words` (exactly
/// `⌈nbits/64⌉` of them; bits past `nbits` are cleared).
#[inline]
pub(crate) fn make_bool_vector_from_words(nbits: usize, words: Vec<u64>) -> Value {
    debug_assert_eq!(words.len(), BoolVectorObj::words_for(nbits));
    Value::make_bool_vector(nbits, words)
}

/// A new bool-vector of `nbits` bits, all `init`.
pub(crate) fn make_bool_vector_filled(nbits: usize, init: bool) -> Value {
    let pattern = if init { u64::MAX } else { 0 };
    make_bool_vector_from_words(nbits, vec![pattern; BoolVectorObj::words_for(nbits)])
}

/// A new bool-vector holding `bits`.
pub(crate) fn bool_vector_from_bits(bits: &[bool]) -> Value {
    let mut words = vec![0u64; BoolVectorObj::words_for(bits.len())];
    for (index, &bit) in bits.iter().enumerate() {
        if bit {
            words[index / BoolVectorObj::WORD_BITS] |= 1u64 << (index % BoolVectorObj::WORD_BITS);
        }
    }
    make_bool_vector_from_words(bits.len(), words)
}

/// A new bool-vector with the bits of `bytes` (GNU's byte order: bit `i` in
/// byte `i / 8` at bit `i % 8`), `nbits` long; bytes past the end are
/// ignored and missing ones read as zero. The reader's `#&N"..."`.
pub(crate) fn bool_vector_from_bytes(nbits: usize, bytes: &[u8]) -> Value {
    let mut words = vec![0u64; BoolVectorObj::words_for(nbits)];
    for (index, &byte) in bytes.iter().take(nbits.div_ceil(8)).enumerate() {
        words[index / 8] |= u64::from(byte) << ((index % 8) * 8);
    }
    make_bool_vector_from_words(nbits, words)
}

/// A new bool-vector of `nbits <= 128` bits from `bits` (bit `i` of the
/// integer is element `i`): category sets and the compact equal-hash key.
pub(crate) fn bool_vector_from_u128(nbits: usize, bits: u128) -> Value {
    debug_assert!(nbits <= 128);
    let words = [bits as u64, (bits >> 64) as u64];
    make_bool_vector_from_words(nbits, words[..BoolVectorObj::words_for(nbits)].to_vec())
}

/// The bits of a bool-vector of at most 128 bits as one integer (bit `i`
/// is element `i`), with its length; `None` for anything else.
pub(crate) fn bool_vector_u128(value: &Value) -> Option<(usize, u128)> {
    let view = BoolVectorView::of(value)?;
    let nbits = view.len();
    if nbits > 128 {
        return None;
    }
    let words = view.words();
    let lo = words.first().copied().unwrap_or(0);
    let hi = words.get(1).copied().unwrap_or(0);
    Some((nbits, u128::from(lo) | (u128::from(hi) << 64)))
}

/// The `equal`-table key of a bool-vector of `nbits <= 128` bits `bits`:
/// what `to_hash_key` builds for one, without allocating it.
pub(crate) fn bool_vector_equal_key_u128(nbits: usize, bits: u128) -> HashKey {
    debug_assert!(nbits <= 128);
    let words = [bits as u64, (bits >> 64) as u64];
    HashKey::BoolVector(Box::new((
        nbits,
        words[..BoolVectorObj::words_for(nbits)].into(),
    )))
}

/// A new bool-vector with the same bits as `value` (a bool-vector): GNU
/// `copy-sequence`.
pub(crate) fn copy_bool_vector(value: &Value) -> Option<Value> {
    let view = BoolVectorView::of(value)?;
    Some(make_bool_vector_from_words(
        view.len(),
        view.words().to_vec(),
    ))
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn wrong_type(pred: &str, got: &Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol(pred), *got],
    )
}

/// GNU `CHECK_BOOL_VECTOR`.
fn check_bool_vector(value: &Value) -> Result<BoolVectorView<'static>, Flow> {
    BoolVectorView::of(value).ok_or_else(|| wrong_type("bool-vector-p", value))
}

/// GNU `CHECK_FIXNAT`: `(wrong-type-argument wholenump VALUE)` unless a
/// non-negative fixnum.
fn check_fixnat(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if n >= 0 => Ok(n),
        _ => Err(wrong_type("wholenump", value)),
    }
}

/// GNU `wrong_length_argument (a1, a2, a3)` (`data.c:119`): the three
/// objects' bool-vector sizes, the third only when it is non-nil.
fn wrong_length_argument(a: &BoolVectorView, b: &BoolVectorView, c: Option<usize>) -> Flow {
    let mut data = vec![Value::fixnum(a.len() as i64), Value::fixnum(b.len() as i64)];
    if let Some(c) = c {
        data.push(Value::fixnum(c as i64));
    }
    signal(LispCondition::WrongLengthArgument, data)
}

/// The optional destination of the set operations: GNU's `NILP (dest)`
/// means "allocate", and an omitted argument is nil.
fn optional_arg(args: &[Value], index: usize) -> Value {
    args.get(index).copied().unwrap_or(Value::NIL)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// `(make-bool-vector LENGTH INIT)`.
pub(crate) fn builtin_make_bool_vector(args: Vec<Value>) -> EvalResult {
    make_bool_vector(args).map_err(crate::emacs_core::alloc::AllocationFailure::into_flow)
}

pub(crate) fn builtin_make_bool_vector_in_context(
    context: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    make_bool_vector(args).map_err(|failure| failure.into_flow_in_context(context))
}

fn make_bool_vector(
    args: Vec<Value>,
) -> Result<Value, crate::emacs_core::alloc::AllocationFailure> {
    expect_args("make-bool-vector", &args, 2)?;
    let length = check_fixnat(&args[0])? as usize;
    // GNU alloc.c:2183-2190: failed backing storage is memory_full.
    let nwords = BoolVectorObj::words_for(length);
    let mut words = Vec::new();
    words.try_reserve_exact(nwords)?;
    words.resize(nwords, if args[1].is_truthy() { u64::MAX } else { 0 });
    Ok(make_bool_vector_from_words(length, words))
}

/// `(bool-vector &rest OBJECTS)`.
pub(crate) fn builtin_bool_vector(args: Vec<Value>) -> EvalResult {
    let bits: Vec<bool> = args.iter().map(|v| v.is_truthy()).collect();
    Ok(bool_vector_from_bits(&bits))
}

/// `(bool-vector-p OBJECT)`.
pub(crate) fn builtin_bool_vector_p(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-p", &args, 1)?;
    Ok(Value::bool_val(is_bool_vector(&args[0])))
}

/// The two-operand set operations of `bool_vector_binop_driver`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    ExclusiveOr,
    Union,
    Intersection,
    SetDifference,
}

impl BinOp {
    #[inline]
    fn apply(self, a: u64, b: u64) -> u64 {
        match self {
            BinOp::ExclusiveOr => a ^ b,
            BinOp::Union => a | b,
            BinOp::Intersection => a & b,
            BinOp::SetDifference => a & !b,
        }
    }
}

/// GNU `bool_vector_binop_driver` (`data.c:3725`) for the four set
/// operations: the result into a fresh bool-vector when `dest` is nil, else
/// into `dest`, returning it only if some word changed (nil otherwise).
fn binop_driver(a: Value, b: Value, dest: Value, op: BinOp) -> EvalResult {
    let a_view = check_bool_vector(&a)?;
    let b_view = check_bool_vector(&b)?;
    let nbits = a_view.len();
    if b_view.len() != nbits {
        let dest_len = if dest.is_nil() {
            None
        } else {
            Some(bool_vector_length(&dest).unwrap_or(0) as usize)
        };
        return Err(wrong_length_argument(&a_view, &b_view, dest_len));
    }
    let a_words = a_view.words();
    let b_words = b_view.words();
    if dest.is_nil() {
        let words: Vec<u64> = a_words
            .iter()
            .zip(b_words.iter())
            .map(|(&x, &y)| op.apply(x, y))
            .collect();
        return Ok(make_bool_vector_from_words(nbits, words));
    }
    let dest_view = check_bool_vector(&dest)?;
    if dest_view.len() != nbits {
        return Err(wrong_length_argument(
            &a_view,
            &b_view,
            Some(dest_view.len()),
        ));
    }
    let dest_words = dest_view.words();
    let first_change = a_words
        .iter()
        .zip(b_words.iter())
        .zip(dest_words.iter())
        .position(|((&x, &y), &d)| d != op.apply(x, y));
    let Some(first_change) = first_change else {
        return Ok(Value::NIL);
    };
    // Copy out the operands before writing: DEST may be A or B.
    let mut words = dest_words.to_vec();
    for i in first_change..words.len() {
        words[i] = op.apply(a_words[i], b_words[i]);
    }
    store_words(&dest, &words);
    Ok(dest)
}

/// `(bool-vector-exclusive-or A B &optional C)`.
pub(crate) fn builtin_bool_vector_exclusive_or(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-exclusive-or", &args, 2)?;
    expect_max_args("bool-vector-exclusive-or", &args, 3)?;
    binop_driver(args[0], args[1], optional_arg(&args, 2), BinOp::ExclusiveOr)
}

/// `(bool-vector-union A B &optional C)`.
pub(crate) fn builtin_bool_vector_union(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-union", &args, 2)?;
    expect_max_args("bool-vector-union", &args, 3)?;
    binop_driver(args[0], args[1], optional_arg(&args, 2), BinOp::Union)
}

/// `(bool-vector-intersection A B &optional C)`.
pub(crate) fn builtin_bool_vector_intersection(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-intersection", &args, 2)?;
    expect_max_args("bool-vector-intersection", &args, 3)?;
    binop_driver(
        args[0],
        args[1],
        optional_arg(&args, 2),
        BinOp::Intersection,
    )
}

/// `(bool-vector-set-difference A B &optional C)`.
pub(crate) fn builtin_bool_vector_set_difference(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-set-difference", &args, 2)?;
    expect_max_args("bool-vector-set-difference", &args, 3)?;
    binop_driver(
        args[0],
        args[1],
        optional_arg(&args, 2),
        BinOp::SetDifference,
    )
}

/// `(bool-vector-subsetp A B)`: GNU runs the driver with `dest = b`, so a
/// length mismatch reports B's size twice.
pub(crate) fn builtin_bool_vector_subsetp(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-subsetp", &args, 2)?;
    let a_view = check_bool_vector(&args[0])?;
    let b_view = check_bool_vector(&args[1])?;
    if a_view.len() != b_view.len() {
        return Err(wrong_length_argument(&a_view, &b_view, Some(b_view.len())));
    }
    let a_words = a_view.words();
    let b_words = b_view.words();
    let subset = a_words
        .iter()
        .zip(b_words.iter())
        .all(|(&x, &y)| x & !y == 0);
    Ok(Value::bool_val(subset))
}

/// `(bool-vector-not A &optional B)`: always returns the destination.
pub(crate) fn builtin_bool_vector_not(args: Vec<Value>) -> EvalResult {
    expect_min_args("bool-vector-not", &args, 1)?;
    expect_max_args("bool-vector-not", &args, 2)?;
    let a_view = check_bool_vector(&args[0])?;
    let nbits = a_view.len();
    let dest = optional_arg(&args, 1);
    if !dest.is_nil() {
        let dest_view = check_bool_vector(&dest)?;
        if dest_view.len() != nbits {
            return Err(wrong_length_argument(&a_view, &dest_view, None));
        }
    }
    let mut words: Vec<u64> = a_view.words().iter().map(|&w| !w).collect();
    if let Some(last) = words.last_mut() {
        *last &= BoolVectorObj::last_word_mask(nbits);
    }
    if dest.is_nil() {
        return Ok(make_bool_vector_from_words(nbits, words));
    }
    store_words(&dest, &words);
    Ok(dest)
}

/// `(bool-vector-count-population A)`.
pub(crate) fn builtin_bool_vector_count_population(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-count-population", &args, 1)?;
    let view = check_bool_vector(&args[0])?;
    let count: u64 = view.words().iter().map(|w| u64::from(w.count_ones())).sum();
    Ok(Value::fixnum(count as i64))
}

/// `(bool-vector-count-consecutive A B I)`: GNU's word scan
/// (`data.c:3943`): XOR with the twiddle turns "count equal bits" into
/// "count zero bits".
pub(crate) fn builtin_bool_vector_count_consecutive(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-count-consecutive", &args, 3)?;
    let view = check_bool_vector(&args[0])?;
    let start = check_fixnat(&args[2])?;
    let nbits = view.len();
    if start as u64 > nbits as u64 {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], args[2]],
        ));
    }
    let start = start as usize;
    let words = view.words();
    let twiddle = if args[1].is_nil() { 0 } else { u64::MAX };
    let bits = BoolVectorObj::WORD_BITS;
    let nwords = words.len();
    let mut pos = start / bits;
    let offset = start % bits;
    let mut count = 0usize;
    if pos < nwords && offset != 0 {
        let mut mword = (words[pos] ^ twiddle) >> offset;
        // Do not count the pad bits.
        mword |= 1u64 << (bits - offset);
        count = mword.trailing_zeros() as usize;
        pos += 1;
        if count + offset < bits {
            return Ok(Value::fixnum(count as i64));
        }
    }
    let pos0 = pos;
    while pos < nwords && words[pos] == twiddle {
        pos += 1;
    }
    count += (pos - pos0) * bits;
    if pos < nwords {
        count += (words[pos] ^ twiddle).trailing_zeros() as usize;
    } else if nbits % bits != 0 {
        // Overshot by the spare bits at the end of the last word.
        count -= bits - nbits % bits;
    }
    Ok(Value::fixnum(count as i64))
}

#[cfg(test)]
#[path = "tests/boolvec_test.rs"]
mod tests;
