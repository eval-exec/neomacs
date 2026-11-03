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

use crate::error::LispCondition;
use crate::error::{EvalResult, Flow, signal};
use crate::error::{expect_args, expect_max_args, expect_min_args};
use crate::tagged::header::BoolVectorObj;
use crate::value::*;
use std::alloc::Layout;
use std::num::NonZeroUsize;
use std::ptr::NonNull;

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Overwrite every bit of the bool-vector `dest` (of `words.len()` words'
/// worth of bits) from `words`, in place.
fn store_words(dest: &Value, words: &[u64]) {
    let _ = dest.with_bool_vector_mut(|obj| {
        obj.words_mut().copy_from_slice(words);
        obj.clear_trailing_bits();
    });
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

/// Checked backing extent in u64 words. Nonempty layouts have exactly the
/// size and alignment of `words` u64 elements; empty storage never reaches
/// the allocator. This witness retains no Lisp or mutator state and can be
/// shared between mutators without a thread-local allocator assumption.
#[derive(Clone, Copy, Debug)]
enum BoolVectorWordLayout {
    Empty,
    Nonempty { words: NonZeroUsize, layout: Layout },
}

static_assertions::assert_impl_all!(BoolVectorWordLayout: Send, Sync);

impl TryFrom<usize> for BoolVectorWordLayout {
    type Error = crate::alloc::AllocationFailure;

    #[inline]
    fn try_from(words: usize) -> Result<Self, Self::Error> {
        let Some(words) = NonZeroUsize::new(words) else {
            return Ok(Self::Empty);
        };
        let layout = Layout::array::<u64>(words.get())?;
        Ok(Self::Nonempty { words, layout })
    }
}

/// Allocate initialized zero words through the global allocator, preserving
/// GNU's clear-backing path (alloc.c:2145-2169, 2183-2190). Layout and null
/// failures remain typed until the caller selects its live memory condition.
/// Immediate Vec ownership supplies ordinary RAII reclamation; u64 storage
/// contains no Lisp references or mutator-confined state.
#[inline]
#[deny(clippy::wildcard_enum_match_arm)]
fn allocate_zeroed_bool_vector_words(
    extent: BoolVectorWordLayout,
) -> Result<Vec<u64>, crate::alloc::AllocationFailure> {
    match extent {
        BoolVectorWordLayout::Empty => Ok(Vec::new()),
        BoolVectorWordLayout::Nonempty { words, layout } => {
            // SAFETY: the witness validated this nonzero u64 array's size and
            // alignment. alloc_zeroed uses Rust's global allocator; null is
            // handled before the allocation is adopted or dereferenced.
            let pointer = unsafe { std::alloc::alloc_zeroed(layout) };
            let pointer = NonNull::new(pointer)
                .ok_or(crate::alloc::AllocationFailure::NullAllocation)?
                .cast::<u64>();
            // SAFETY: this unique global allocation has exactly Vec<u64>'s
            // layout for capacity words. Every element is initialized, since
            // all-zero bits are valid u64 values. Immediate adoption has no
            // alias, intervening fallible step or mismatched deallocator.
            Ok(unsafe { Vec::from_raw_parts(pointer.as_ptr(), words.get(), words.get()) })
        }
    }
}

/// Allocate empty u64 word backing for the truthy fill path (GNU
/// alloc.c:2145-2169, 2183-2190). The checked extent is arithmetic-only and
/// allocator-independent; the storage contains no Lisp or mutator state.
/// Vec owns the Global allocation immediately, with zero initialized words;
/// the existing resize supplies all-one words before bool-vector adoption.
/// InvalidLayout and NullAllocation remain typed for live memory delivery.
#[inline]
#[deny(clippy::wildcard_enum_match_arm)]
fn allocate_bool_vector_word_capacity(
    extent: BoolVectorWordLayout,
) -> Result<Vec<u64>, crate::alloc::AllocationFailure> {
    match extent {
        BoolVectorWordLayout::Empty => Ok(Vec::new()),
        BoolVectorWordLayout::Nonempty { words, layout } => {
            // SAFETY: the witness checked this nonzero Layout::array::<u64>,
            // including alignment and isize size limits. alloc uses the
            // Global allocator; null is handled before storage adoption.
            let pointer = unsafe { std::alloc::alloc(layout) };
            let pointer = NonNull::new(pointer)
                .ok_or(crate::alloc::AllocationFailure::NullAllocation)?
                .cast::<u64>();
            // SAFETY: unique Global storage has exactly Vec<u64>'s layout for
            // capacity words. Length zero exposes no uninitialized word.
            // Immediate ownership has no alias or intervening fallible step;
            // Vec growth and Drop retain the same allocator and exact layout.
            Ok(unsafe { Vec::from_raw_parts(pointer.as_ptr(), 0, words.get()) })
        }
    }
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

// Definitions are private here; the old core paths retain their visibility.
mod data;
#[doc(hidden)]
pub use data::BoolVectorView;
#[doc(hidden)]
pub use data::bool_vector_elements;
#[doc(hidden)]
pub use data::bool_vector_equal_key_u128;
#[doc(hidden)]
pub use data::bool_vector_fill;
#[doc(hidden)]
pub use data::bool_vector_from_bits;
#[doc(hidden)]
pub use data::bool_vector_from_bytes;
#[doc(hidden)]
pub use data::bool_vector_from_u128;
#[doc(hidden)]
pub use data::bool_vector_length;
#[doc(hidden)]
pub use data::bool_vector_ref_value;
#[doc(hidden)]
pub use data::bool_vector_set;
#[doc(hidden)]
pub use data::bool_vector_u128;
#[doc(hidden)]
pub use data::builtin_bool_vector;
#[doc(hidden)]
pub use data::builtin_bool_vector_count_consecutive;
#[doc(hidden)]
pub use data::builtin_bool_vector_count_population;
#[doc(hidden)]
pub use data::builtin_bool_vector_exclusive_or;
#[doc(hidden)]
pub use data::builtin_bool_vector_intersection;
#[doc(hidden)]
pub use data::builtin_bool_vector_not;
#[doc(hidden)]
pub use data::builtin_bool_vector_p;
#[doc(hidden)]
pub use data::builtin_bool_vector_set_difference;
#[doc(hidden)]
pub use data::builtin_bool_vector_subsetp;
#[doc(hidden)]
pub use data::builtin_bool_vector_union;
#[doc(hidden)]
pub use data::builtin_make_bool_vector;
#[doc(hidden)]
pub use data::copy_bool_vector;
#[doc(hidden)]
pub use data::is_bool_vector;
#[doc(hidden)]
pub use data::make_bool_vector;
#[doc(hidden)]
pub use data::nreverse_bool_vector;
#[doc(hidden)]
pub use data::reverse_bool_vector;

#[cfg(test)]
#[path = "tests/zeroed_storage_test.rs"]
mod zeroed_storage_tests;
