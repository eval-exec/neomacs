//! Bootstrap-facing subset of GNU Emacs's `alloc.c`.
//!
//! GNU exposes several GC / memory-management variables from C before Lisp
//! startup runs.  Keep those defaults here so Lisp like `jit-lock.el` can rely
//! on the same low-level variables during runtime and bootstrap.

use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

use crate::emacs_core::error::{Flow, LispCondition, signal, signal_with_data};

fn expect_wholenump(value: &Value) -> Result<i64, Flow> {
    value.as_fixnum().filter(|n| *n >= 0).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *value],
        )
    })
}

mod sealed {
    pub trait Sealed {}
}

/// An allocation length checked against its GNU domain before reserving storage.
/// Values contain no Lisp handles or shared state and may cross mutator threads.
pub(crate) trait AllocLen: sealed::Sealed + TryFrom<Value, Error = Flow> {
    fn capacity(self) -> usize;
}

/// Total record slots, including the type slot, bounded by GNU's 12-bit header.
/// This immutable count is independent of the allocating mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecordLen(usize);

impl sealed::Sealed for RecordLen {}
impl AllocLen for RecordLen {
    fn capacity(self) -> usize {
        self.0
    }
}
impl TryFrom<Value> for RecordLen {
    type Error = Flow;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let total = expect_wholenump(&value)? + 1;
        if total > 4095 {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(format!(
                    "Attempt to allocate a record of {total} slots; max is 4095"
                ))],
            ));
        }
        Ok(Self(total as usize))
    }
}

/// GNU's validated obarray bucket exponent (lread.c:obarray_max_bits).
/// Immutable and independent of the allocating mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ObarrayBits(u32);

impl sealed::Sealed for ObarrayBits {}
impl AllocLen for ObarrayBits {
    fn capacity(self) -> usize {
        1usize << self.0
    }
}
impl TryFrom<Value> for ObarrayBits {
    type Error = Flow;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if value.is_nil() {
            return Ok(Self(3));
        }
        let hint = expect_wholenump(&value)? as u64;
        let bits = u64::BITS - hint.leading_zeros();
        let max_bits = (i32::BITS.min(usize::BITS - 6)) - 1;
        if bits > max_bits {
            return Err(signal_with_data(LispCondition::ArgsOutOfRange, value));
        }
        Ok(Self(bits))
    }
}

/// A nonnegative Lisp hash-table capacity with representable backing slots.
/// Immutable and independent of the allocating mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HashTableSize(usize);

impl sealed::Sealed for HashTableSize {}
impl AllocLen for HashTableSize {
    fn capacity(self) -> usize {
        self.0
    }
}

/// GNU's C-int extra-slot count, normalized before Rust allocation.
/// Negative counts after GNU narrowing have no accessible extra slots; the
/// Rust table retains its complete standard contents instead of a short object.
/// This immutable scalar is independent of the allocating mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CharTableExtras(usize);

impl sealed::Sealed for CharTableExtras {}
impl AllocLen for CharTableExtras {
    fn capacity(self) -> usize {
        self.0
    }
}
impl TryFrom<Value> for CharTableExtras {
    type Error = Flow;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let raw = expect_wholenump(&value)?.to_le_bytes();
        // GNU chartab.c assigns the fixnat to an int before adding the 68
        // standard slots. Express that external representation explicitly.
        let extras = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        if extras.checked_add(68).is_none_or(|total| total < 0) {
            return Err(memory_exhausted());
        }
        Ok(Self(extras.max(0) as usize))
    }
}
impl TryFrom<Value> for HashTableSize {
    type Error = Flow;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let size = if value.is_nil() {
            0
        } else if let Some(n) = value.as_fixnum().filter(|n| *n >= 0) {
            n as usize
        } else {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string("Invalid hash table size"), value],
            ));
        };
        // GNU allocates two Lisp_Object slots per entry before its index.
        if size > isize::MAX as usize / (2 * size_of::<Value>()) {
            return Err(memory_exhausted());
        }
        Ok(Self(size))
    }
}

/// A nonnegative repeat count checked at the Lisp boundary.
/// This immutable scalar is independent of all mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RepeatCount(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RepeatCountError {
    #[error("repeat count must be nonnegative and fit usize")]
    OutOfRange,
}

impl TryFrom<i64> for RepeatCount {
    type Error = RepeatCountError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        usize::try_from(value)
            .map(Self)
            .map_err(|_| RepeatCountError::OutOfRange)
    }
}

impl From<RepeatCount> for usize {
    fn from(value: RepeatCount) -> Self {
        value.0
    }
}

/// A representable repeated Lisp string byte length, checked before reserving.
/// This immutable scalar is independent of all mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StringByteLen(usize);

impl StringByteLen {
    pub(crate) fn repeated(unit: usize, count: RepeatCount, extra: usize) -> Result<Self, Flow> {
        let len = unit
            .checked_mul(count.0)
            .and_then(|bytes| bytes.checked_add(extra))
            .filter(|bytes| *bytes <= Value::MOST_POSITIVE_FIXNUM as usize)
            .ok_or_else(memory_exhausted)?;
        Ok(Self(len))
    }

    pub(crate) fn capacity(self) -> usize {
        self.0
    }
}

/// Bytes of repeated buffer text, checked before allocation and insertion.
/// The count is immutable and carries no mutator-owned Lisp state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferByteLen(usize);

impl BufferByteLen {
    pub(crate) fn repeated(unit: usize, count: RepeatCount, existing: usize) -> Result<Self, Flow> {
        let len = unit
            .checked_mul(count.0)
            .filter(|len| {
                existing
                    .checked_add(*len)
                    .is_some_and(|total| total <= (Value::MOST_POSITIVE_FIXNUM - 1) as usize)
            })
            .ok_or_else(buffer_overflow)?;
        Ok(Self(len))
    }

    pub(crate) fn capacity(self) -> usize {
        self.0
    }
}

pub(crate) fn buffer_overflow() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string("Maximum buffer size exceeded")],
    )
}

/// GNU's default `memory-signal-data`; callers with a Context should select its
/// live binding instead. No global mutable Lisp state is stored here.
pub(crate) fn memory_exhausted() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string(
            "Memory exhausted--use C-x s then exit and restart Emacs",
        )],
    )
}

pub(crate) fn reserved_values<L: AllocLen>(len: L) -> Result<Vec<Value>, Flow> {
    let mut items = Vec::new();
    items
        .try_reserve_exact(len.capacity())
        .map_err(|_| memory_exhausted())?;
    Ok(items)
}

static_assertions::assert_impl_all!(RecordLen: Send, Sync);
static_assertions::assert_impl_all!(ObarrayBits: Send, Sync);
static_assertions::assert_impl_all!(HashTableSize: Send, Sync);
static_assertions::assert_impl_all!(CharTableExtras: Send, Sync);
static_assertions::assert_impl_all!(BufferByteLen: Send, Sync);
static_assertions::assert_impl_all!(RepeatCount: Send, Sync);
static_assertions::assert_impl_all!(StringByteLen: Send, Sync);

/// Register bootstrap variables owned by the allocation / GC subsystem.
pub fn register_bootstrap_vars(obarray: &mut Obarray) {
    obarray.define_int_variable("gc-cons-threshold", 800_000);
    obarray.set_symbol_value("gc-cons-percentage", Value::make_float(0.1));
    obarray.make_special("gc-cons-percentage");
    obarray.set_symbol_value("post-gc-hook", Value::NIL);
    obarray.make_special("post-gc-hook");
    obarray.set_symbol_value(
        "memory-signal-data",
        Value::list(vec![
            Value::symbol("error"),
            Value::string(
                "Memory exhausted--use M-x save-some-buffers then exit and restart Emacs",
            ),
        ]),
    );
    obarray.make_special("memory-signal-data");
    obarray.set_symbol_value("memory-full", Value::NIL);
    obarray.make_special("memory-full");
    obarray.set_symbol_value("gc-elapsed", Value::make_float(0.0));
    obarray.make_special("gc-elapsed");
    obarray.define_int_variable("gcs-done", 0);
    obarray.define_int_variable("pure-bytes-used", 0);
    // `src/alloc.c:7448' DEFVAR_INT, no initializer -- the C global starts at 0
    // and `allocate_string' counts up from there.  Neomacs does not track it
    // yet, like its five siblings in `eval.rs' (`cons-cells-consed' and
    // friends), so it reads 0 where GNU reads whatever it has allocated.
    obarray.define_int_variable("strings-consed", 0);
}

#[cfg(test)]
#[path = "tests/alloc_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/validated_lengths.rs"]
mod validated_lengths;
