//! Lower-layer DATA definitions with restricted core aliases.

use super::*;

/// An allocation length checked against its GNU domain before reserving storage.
/// Values contain no Lisp handles or shared state and may cross mutator threads.
pub trait AllocLen: sealed::Sealed + TryFrom<Value, Error = Flow> {
    fn capacity(self) -> usize;
}

/// Total record slots, including the type slot, bounded by GNU's 12-bit header.
/// This immutable count is independent of the allocating mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordLen(usize);

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
pub struct ObarrayBits(u32);

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
pub struct HashTableSize(usize);

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
pub struct CharTableExtras(usize);

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

/// A Lisp hash-table size hint whose shape is valid for GNU FIXNATP.
///
/// This proves nil/default or a nonnegative fixnum, without reserving storage
/// or testing backing-slot representability. Keeping those phases separate
/// preserves GNU's weakness-error precedence before allocation errors.
/// This immutable scalar contains no Lisp handles and is Send + Sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HashTableHint(u64);

impl TryFrom<Value> for HashTableHint {
    type Error = Flow;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let size = if value.is_nil() {
            // Preserve the current constructor default. GNU DEFAULT_HASH_SIZE
            // parity is a separate issue, unrelated to error ordering.
            0
        } else if let Some(n) = value.as_fixnum().filter(|n| *n >= 0) {
            n.unsigned_abs()
        } else {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string("Invalid hash table size"), value],
            ));
        };
        Ok(Self(size))
    }
}

impl TryFrom<HashTableHint> for HashTableSize {
    type Error = Flow;

    fn try_from(hint: HashTableHint) -> Result<Self, Self::Error> {
        let size = usize::try_from(hint.0).map_err(|_| memory_exhausted())?;
        // GNU allocates two Lisp_Object slots per entry before its index.
        if size > isize::MAX as usize / (2 * size_of::<Value>()) {
            return Err(memory_exhausted());
        }
        Ok(Self(size))
    }
}

impl TryFrom<Value> for HashTableSize {
    type Error = Flow;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Self::try_from(HashTableHint::try_from(value)?)
    }
}

/// A nonnegative repeat count checked at the Lisp boundary.
/// This immutable scalar is independent of all mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepeatCount(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RepeatCountError {
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
pub struct StringByteLen(usize);

impl StringByteLen {
    pub fn repeated(unit: usize, count: RepeatCount, extra: usize) -> Result<Self, Flow> {
        let len = unit
            .checked_mul(count.0)
            .and_then(|bytes| bytes.checked_add(extra))
            .filter(|bytes| *bytes <= Value::MOST_POSITIVE_FIXNUM as usize)
            .ok_or_else(memory_exhausted)?;
        Ok(Self(len))
    }

    /// Empty text storage for this length, reserved fallibly.
    pub fn reserved_text(self) -> Result<String, Flow> {
        reserved_lisp_text(self.0)
    }
}

/// Bytes of repeated buffer text, checked before allocation and insertion.
/// The count is immutable and carries no mutator-owned Lisp state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferByteLen(usize);

impl BufferByteLen {
    pub fn repeated(unit: usize, count: RepeatCount, existing: usize) -> Result<Self, Flow> {
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

    /// Empty byte storage for this length, reserved fallibly.
    pub fn reserved_bytes(self) -> Result<Vec<u8>, Flow> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(lisp_text_payload_capacity(self.0))
            .map_err(|_| memory_exhausted())?;
        Ok(bytes)
    }

    /// Empty text storage for this length, reserved fallibly.
    pub fn reserved_text(self) -> Result<String, Flow> {
        reserved_lisp_text(self.0)
    }
}

pub fn buffer_overflow() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string("Maximum buffer size exceeded")],
    )
}

/// GNU's default `memory-signal-data`; callers with a Context should select its
/// live binding instead. No global mutable Lisp state is stored here.
pub fn memory_exhausted() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string(
            "Memory exhausted--use C-x s then exit and restart Emacs",
        )],
    )
}

pub fn reserved_values<L: AllocLen>(len: L) -> Result<Vec<Value>, Flow> {
    let mut items = Vec::new();
    items
        .try_reserve_exact(len.capacity())
        .map_err(|_| memory_exhausted())?;
    Ok(items)
}

/// A constructor can reject Lisp arguments or exhaust its backing allocation.
/// The latter must retain its identity until the evaluator selects its live
/// `memory-signal-data` (GNU alloc.c:4140-4142). No state is shared between
/// mutators; the context supplying the signal owns the Lisp payload.
#[derive(Debug, thiserror::Error)]
pub enum AllocationFailure {
    #[error("Lisp argument condition")]
    Lisp(crate::error::Flow),
    #[error("memory exhausted")]
    MemoryExhausted(#[from] std::collections::TryReserveError),
    /// The global allocator rejected a nonzero, validated storage request.
    #[error("allocation returned null")]
    NullAllocation,
    /// A checked allocation layout could not represent the requested extent.
    #[error("invalid allocation layout")]
    InvalidLayout(#[from] std::alloc::LayoutError),
}

impl From<crate::error::Flow> for AllocationFailure {
    fn from(flow: crate::error::Flow) -> Self {
        Self::Lisp(flow)
    }
}

impl AllocationFailure {
    pub fn into_flow(self) -> crate::error::Flow {
        match self {
            Self::Lisp(flow) => flow,
            Self::MemoryExhausted(_) | Self::NullAllocation | Self::InvalidLayout(_) => {
                crate::error::memory_exhausted_error()
            }
        }
    }
}
