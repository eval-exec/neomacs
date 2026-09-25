//! Bool-vectors: GNU `PVEC_BOOL_VECTOR` (`lisp.h:1805-1847`,
//! `alloc.c:2126-2200`, `data.c:3709-4016`).
//!
//! A bool-vector is a [`BoolVectorObj`] (`VecLikeType::BoolVector`): `nbits`
//! bits packed into words, trailing bits zero. Until P3.2 L0.8 deletes it,
//! the older in-band encoding still exists: an ordinary vector
//! `[--bool-vector-- N b0 b1 ...]` whose slot 0 is a marker symbol and whose
//! bits are fixnums 0/1 (the *legacy* representation).
//!
//! `NEOVM_BOOL_VECTOR_REPR=legacy|packed` picks the representation NEW
//! bool-vectors get (read once per process; a same-binary A/B). Every reader
//! here accepts both, so the two kinds may meet in one operation.
//!
//! Operations follow GNU exactly: `wrong-length-argument` data, the
//! destination argument of the set operations ("the destination if it
//! changed, else nil"), and `bool-vector-not`'s unconditional destination.

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::tagged::header::BoolVectorObj;
use std::borrow::Cow;
use std::mem::size_of;

// ---------------------------------------------------------------------------
// The representation knob
// ---------------------------------------------------------------------------

/// Which representation new bool-vectors get (`NEOVM_BOOL_VECTOR_REPR`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoolVectorRepr {
    /// The in-band tagged vector `[--bool-vector-- N 0/1 ...]`.
    Legacy,
    /// A [`BoolVectorObj`] with packed words (GNU's layout).
    Packed,
}

#[cfg(test)]
thread_local! {
    static REPR_OVERRIDE: std::cell::Cell<Option<BoolVectorRepr>> =
        const { std::cell::Cell::new(None) };
}

/// Test hook: the representation bool-vectors created on this thread get
/// (`None` restores the environment's).
#[cfg(test)]
pub(crate) fn set_bool_vector_repr_for_test(repr: Option<BoolVectorRepr>) {
    REPR_OVERRIDE.with(|cell| cell.set(repr));
}

/// The representation new bool-vectors get: `NEOVM_BOOL_VECTOR_REPR`, read
/// once per process. Default: legacy.
pub(crate) fn bool_vector_repr() -> BoolVectorRepr {
    #[cfg(test)]
    if let Some(repr) = REPR_OVERRIDE.with(|cell| cell.get()) {
        return repr;
    }
    static REPR: std::sync::OnceLock<BoolVectorRepr> = std::sync::OnceLock::new();
    *REPR.get_or_init(
        || match std::env::var("NEOVM_BOOL_VECTOR_REPR").ok().as_deref() {
            Some("packed") => {
                tracing::info!(
                    target: "neovm::boolvec::knobs",
                    "NEOVM_BOOL_VECTOR_REPR=packed is on in this process"
                );
                BoolVectorRepr::Packed
            }
            _ => BoolVectorRepr::Legacy,
        },
    )
}

// ---------------------------------------------------------------------------
// Reading either representation
// ---------------------------------------------------------------------------

/// Slot 0 of a legacy bool-vector.
const LEGACY_TAG: &str = "--bool-vector--";
/// Slot index of a legacy bool-vector's bit count.
const LEGACY_SIZE: usize = 1;
/// Slot index of a legacy bool-vector's first bit.
const LEGACY_BITS: usize = 2;

/// The legacy tag symbol's id.
pub(crate) fn legacy_tag_sym_id() -> super::intern::SymId {
    static ID: std::sync::OnceLock<super::intern::SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| super::intern::intern(LEGACY_TAG))
}

/// Is `slots` (a vector's slots) the legacy encoding?
#[inline]
fn is_legacy_slots(slots: &[Value]) -> bool {
    slots.len() >= LEGACY_BITS && slots[0].as_symbol_id() == Some(legacy_tag_sym_id())
}

/// A read view of a bool-vector in either representation.
#[derive(Clone, Copy)]
pub(crate) enum BoolVectorView<'a> {
    Packed(&'a BoolVectorObj),
    Legacy(&'a [Value]),
}

impl<'a> BoolVectorView<'a> {
    /// The view of `value`, or `None` when it is not a bool-vector.
    #[inline]
    pub(crate) fn of(value: &Value) -> Option<BoolVectorView<'static>> {
        if let Some(obj) = value.as_bool_vector_obj() {
            return Some(BoolVectorView::Packed(obj));
        }
        if value.is_vector() {
            let slots = value.as_vector_data()?;
            if is_legacy_slots(slots) {
                return Some(BoolVectorView::Legacy(slots));
            }
        }
        None
    }

    /// The number of bits.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        match self {
            BoolVectorView::Packed(obj) => obj.nbits,
            BoolVectorView::Legacy(slots) => match slots[LEGACY_SIZE].kind() {
                ValueKind::Fixnum(n) if n > 0 => n as usize,
                _ => 0,
            },
        }
    }

    /// Bit `index` (`index < len`).
    #[inline]
    pub(crate) fn get(&self, index: usize) -> bool {
        match self {
            BoolVectorView::Packed(obj) => obj.get(index),
            BoolVectorView::Legacy(slots) => {
                let bit = slots
                    .get(LEGACY_BITS + index)
                    .copied()
                    .unwrap_or(Value::NIL);
                match bit.kind() {
                    ValueKind::Fixnum(n) => n != 0,
                    ValueKind::Nil => false,
                    _ => bit.is_truthy(),
                }
            }
        }
    }

    /// The bits as words (bit `i` in word `i / 64` at bit `i % 64`),
    /// trailing bits zero: borrowed from a packed bool-vector, built for a
    /// legacy one.
    pub(crate) fn words(&self) -> Cow<'a, [u64]> {
        match self {
            BoolVectorView::Packed(obj) => Cow::Borrowed(obj.words()),
            BoolVectorView::Legacy(_) => {
                let nbits = self.len();
                let mut words = vec![0u64; BoolVectorObj::words_for(nbits)];
                for index in 0..nbits {
                    if self.get(index) {
                        words[index / BoolVectorObj::WORD_BITS] |=
                            1u64 << (index % BoolVectorObj::WORD_BITS);
                    }
                }
                Cow::Owned(words)
            }
        }
    }

    /// GNU's byte `index` of the bit data (the printer's and `sxhash`'s
    /// host-independent view).
    pub(crate) fn byte(&self, index: usize) -> u8 {
        match self {
            BoolVectorView::Packed(obj) => obj.byte(index),
            BoolVectorView::Legacy(_) => {
                let nbits = self.len();
                let mut byte = 0u8;
                for bit in 0..8 {
                    let i = index * 8 + bit;
                    if i < nbits && self.get(i) {
                        byte |= 1 << bit;
                    }
                }
                byte
            }
        }
    }
}

/// Is `value` a bool-vector (either representation)?
#[inline]
pub(crate) fn is_bool_vector(value: &Value) -> bool {
    value.is_bool_vector_obj()
        || (value.is_vector()
            && value
                .as_vector_data()
                .is_some_and(|slots| is_legacy_slots(slots)))
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
    if value.is_bool_vector_obj() {
        return value
            .with_bool_vector_mut(|obj| {
                if index < obj.nbits {
                    obj.set(index, bit);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
    }
    match BoolVectorView::of(value) {
        Some(view @ BoolVectorView::Legacy(_)) if index < view.len() => {
            value.set_vector_slot(LEGACY_BITS + index, Value::fixnum(bit as i64))
        }
        _ => false,
    }
}

/// Overwrite every bit of the bool-vector `dest` (of `words.len()` words'
/// worth of bits) from `words`, in place.
fn store_words(dest: &Value, words: &[u64]) {
    if dest.is_bool_vector_obj() {
        let _ = dest.with_bool_vector_mut(|obj| {
            obj.words_mut().copy_from_slice(words);
            obj.clear_trailing_bits();
        });
        return;
    }
    let Some(view) = BoolVectorView::of(dest) else {
        return;
    };
    let nbits = view.len();
    let Some(slots) = dest.as_vector_data() else {
        return;
    };
    let mut slots = slots.to_vec();
    for index in 0..nbits {
        let bit = words[index / BoolVectorObj::WORD_BITS] >> (index % BoolVectorObj::WORD_BITS) & 1;
        slots[LEGACY_BITS + index] = Value::fixnum(bit as i64);
    }
    let _ = dest.replace_vector_data(slots);
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

/// A tagged vector (a legacy bool-vector) is being created: the JIT's
/// measurement knob `NEOVM_JIT_AREF_SKIP_SLOT0` counts it (its inline
/// `aref`/`aset` no longer tell tagged vectors apart).
#[inline]
fn note_tagged_vector_created() {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::compile::note_tagged_vector_under_skip_slot0();
}

/// A new bool-vector of `nbits` bits from `words` (exactly
/// `⌈nbits/64⌉` of them; bits past `nbits` are ignored), in the
/// representation [`bool_vector_repr`] picks.
pub(crate) fn make_bool_vector_from_words(nbits: usize, words: Vec<u64>) -> Value {
    debug_assert_eq!(words.len(), BoolVectorObj::words_for(nbits));
    match bool_vector_repr() {
        BoolVectorRepr::Packed => Value::make_bool_vector(nbits, words),
        BoolVectorRepr::Legacy => {
            note_tagged_vector_created();
            let mut slots = Vec::with_capacity(LEGACY_BITS + nbits);
            slots.push(Value::from_sym_id(legacy_tag_sym_id()));
            slots.push(Value::fixnum(nbits as i64));
            for index in 0..nbits {
                let bit = words[index / BoolVectorObj::WORD_BITS]
                    >> (index % BoolVectorObj::WORD_BITS)
                    & 1;
                slots.push(Value::fixnum(bit as i64));
            }
            Value::vector(slots)
        }
    }
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

/// The `equal`-table key of a bool-vector of `nbits <= 128` bits `bits`
/// in the representation [`bool_vector_repr`] gives new bool-vectors: what
/// `to_hash_key` builds for one, without allocating it.
pub(crate) fn bool_vector_equal_key_u128(nbits: usize, bits: u128) -> HashKey {
    debug_assert!(nbits <= 128);
    match bool_vector_repr() {
        BoolVectorRepr::Legacy => HashKey::BoolVec(Box::new((nbits, bits))),
        BoolVectorRepr::Packed => {
            let words = [bits as u64, (bits >> 64) as u64];
            HashKey::BoolVector(Box::new((
                nbits,
                words[..BoolVectorObj::words_for(nbits)].into(),
            )))
        }
    }
}

/// A new bool-vector with the same bits as `value` (a bool-vector): GNU
/// `copy-sequence`.
pub(crate) fn copy_bool_vector(value: &Value) -> Option<Value> {
    let view = BoolVectorView::of(value)?;
    Some(make_bool_vector_from_words(
        view.len(),
        view.words().into_owned(),
    ))
}

/// GNU's `memory_full` signal (`alloc.c:4104`): `memory-signal-data`'s
/// `(error "Memory exhausted--...")`.
fn memory_exhausted() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string(
            "Memory exhausted--use M-x save-some-buffers then exit and restart Emacs",
        )],
    )
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
    expect_args("make-bool-vector", &args, 2)?;
    let length = check_fixnat(&args[0])? as usize;
    // GNU allows any fixnum length and reports `memory_full` when the
    // allocation fails; a request whose byte size cannot even be named
    // fails here the same way instead of aborting the process.
    if BoolVectorObj::words_for(length) > isize::MAX as usize / size_of::<u64>() {
        return Err(memory_exhausted());
    }
    Ok(make_bool_vector_filled(length, args[1].is_truthy()))
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
    let mut words = dest_words.into_owned();
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
#[path = "tests/mod.rs"]
mod tests;
