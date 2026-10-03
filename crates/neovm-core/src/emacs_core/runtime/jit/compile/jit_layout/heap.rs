//! `jit_layout::heap`: the heap object layouts generated code reads and
//! writes in place (p3-0-integration §3.1, §3.2, §3.8; co-owned by P3.2 L1.0
//! and P3.1).
//!
//! * the `GcHeader` byte map, including the sticky collection-observed byte;
//! * the cons-block trailer (the mark bitmap after the cells);
//! * a float's value word and slot stride;
//! * the `JitHeapState` words (allocation cursors, barrier window);
//! * the vector-storage probes (`LispValueVec`).
//!
//! The owning modules keep the definitions; this is the JIT's one place to
//! import them from.

use crate::tagged::header::{GcHeader, HeapObjectKind};
use std::mem::{offset_of, size_of};

pub(crate) use crate::tagged::gc::{
    CONS_BLOCK_BYTES, CONS_BLOCK_CELLS, CONS_MARK_WORDS, CONS_MARKS_OFFSET, CONS_UNLOGGED_OFFSET,
    FLOAT_SLOT_BYTES, HEAP_JIT_BARRIER_LEN, HEAP_JIT_BARRIER_LO, HEAP_JIT_CONS_CUR,
    HEAP_JIT_CONS_LIM, HEAP_JIT_FLOAT_CUR, HEAP_JIT_FLOAT_LIM,
};
pub(crate) use crate::tagged::header::{
    FLOAT_VALUE_OFFSET, GC_HEADER_COLLECTION_OBSERVED_OFFSET, GC_HEADER_TENURED_OFFSET,
};

/// The bytes of a `GcHeader` (p3-0-integration §3.1). Bytes 0–3 and 6 are
/// live today, as is sticky observation byte 7; 4 and 5 are zero bytes RESERVED,
/// which land by turning the reserved field into a real one at exactly that
/// offset (the header's own const asserts pin every field). Bytes 8–15 are
/// the `next` link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum GcHeaderByte {
    /// `marked`: the tri-state mark byte (P3.1 C2.2): 0 unmarked, 1 or 2
    /// marked at that cycle's parity.
    Marked = 0,
    /// `kind`: the object's [`HeapObjectKind`].
    Kind = 1,
    /// `tenured`: old or permanent, sticky.
    Tenured = 2,
    /// `remembered`: the owner is in the remembered set (P0.7b).
    Remembered = 3,
    /// Reserved: the vectorlike `type_tag` (P3.2 L2).
    TypeTag = 4,
    /// Reserved: object `flags` (P3.2 L2: plain slots, bool-vector; L4b
    /// code constants).
    Flags = 5,
    /// `generation`: the [`GenBits`](crate::tagged::header::GenBits), bit 0
    /// `permanent` (P3.1 C2.1), bit 1 age (C3.3).
    Gen = 6,
    /// Sticky collection-certificate observation mark.
    CollectionObserved = 7,
}

impl GcHeaderByte {
    /// The byte's offset within the header.
    pub(crate) const fn offset(self) -> usize {
        self as usize
    }

    /// Whether the byte is reserved today (always 0) for a later claim.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumer: the header claims (P3.2 L1, L2)")
    )]
    pub(crate) const fn is_reserved(self) -> bool {
        matches!(self, GcHeaderByte::TypeTag | GcHeaderByte::Flags)
    }
}

/// Byte offset of the `next` link, after the eight flag bytes.
pub(crate) const GC_HEADER_NEXT_OFFSET: usize = offset_of!(GcHeader, next);

const _: () = {
    assert!(size_of::<GcHeader>() == 16);
    assert!(offset_of!(GcHeader, marked) == GcHeaderByte::Marked.offset());
    assert!(offset_of!(GcHeader, kind) == GcHeaderByte::Kind.offset());
    assert!(offset_of!(GcHeader, tenured) == GcHeaderByte::Tenured.offset());
    assert!(offset_of!(GcHeader, remembered) == GcHeaderByte::Remembered.offset());
    assert!(offset_of!(GcHeader, generation) == GcHeaderByte::Gen.offset());
    assert!(GC_HEADER_COLLECTION_OBSERVED_OFFSET == GcHeaderByte::CollectionObserved.offset());
    assert!(GC_HEADER_TENURED_OFFSET == GcHeaderByte::Tenured.offset());
    // The live fields are one byte each, so the reserved claims fit in the
    // eight flag bytes without growing the header.
    assert!(size_of::<std::sync::atomic::AtomicU8>() == 1);
    assert!(size_of::<std::sync::atomic::AtomicBool>() == 1);
    assert!(size_of::<HeapObjectKind>() == 1);
    assert!(size_of::<bool>() == 1);
    assert!(size_of::<crate::tagged::header::GenBits>() == 1);
    assert!(GC_HEADER_NEXT_OFFSET == 8);
    // The claims are the four bytes before `next`, in order: two reserved,
    // the live generation byte, one reserved.
    assert!(GcHeaderByte::TypeTag.offset() == GcHeaderByte::Remembered.offset() + 1);
    assert!(GcHeaderByte::Flags.offset() == GcHeaderByte::TypeTag.offset() + 1);
    assert!(GcHeaderByte::Gen.offset() == GcHeaderByte::Flags.offset() + 1);
    assert!(GcHeaderByte::CollectionObserved.offset() == GcHeaderByte::Gen.offset() + 1);
    assert!(GcHeaderByte::CollectionObserved.offset() + 1 == GC_HEADER_NEXT_OFFSET);
    // P0.7's inline `u16` test reads `tenured` and `remembered` as one pair.
    assert!(GcHeaderByte::Remembered.offset() == GcHeaderByte::Tenured.offset() + 1);
};

const _: () = {
    // A float's value is one word after its header, read and written in
    // place; the slot stride covers header and value.
    assert!(FLOAT_VALUE_OFFSET as usize == size_of::<GcHeader>());
    assert!(FLOAT_SLOT_BYTES >= FLOAT_VALUE_OFFSET as usize + size_of::<f64>());
    // The cons-block trailer: the mark words sit right after the cells,
    // inside the block.
    assert!(CONS_MARKS_OFFSET + CONS_MARK_WORDS * size_of::<usize>() <= CONS_BLOCK_BYTES);
    assert!(CONS_BLOCK_BYTES.is_power_of_two());
    assert!(CONS_BLOCK_CELLS > 0);
};

/// `(data pointer, length)` offsets within a `LispValueVec`, shared by its
/// owned and mapped storage (`None` when they differ).
pub(crate) fn value_vec_slice_offsets() -> Option<(usize, usize)> {
    crate::tagged::header::LispValueVec::jit_slice_offsets()
}

/// `(offset, word)` telling a mapped `LispValueVec` from an owned one
/// (`None` when no single word separates them).
pub(crate) fn value_vec_owned_probe() -> Option<(usize, usize)> {
    crate::tagged::header::LispValueVec::jit_owned_probe()
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PlainArrayOffsets {
    pub(crate) type_tag: i32,
    pub(crate) slots: i32,
    pub(crate) length: i32,
    pub(crate) element_shift: i64,
}

const _: () = {
    use crate::tagged::header::{RecordObj, VectorObj};
    assert!(offset_of!(VectorObj, header) == 0);
    assert!(offset_of!(RecordObj, header) == 0);
    assert!(offset_of!(VectorObj, data) == offset_of!(RecordObj, data));
};

/// Checked host-layout composition. LispValueVec's existing two-storage probe
/// compares owned/mapped pointer and length positions. Offsets are relative to
/// the actual VectorObj/RecordObj, whose repr(C) prefix is asserted above.
///
/// Valid storage supports &[TaggedValue]: byte extent <= isize::MAX for both
/// owned Vec and the unsafe mapped-slice contract. Consequently length is
/// <= isize::MAX / sizeof(TaggedValue), strictly within the fixnum domain on
/// this admitted 64-bit one-word layout. Neither a mutable length nor backing
/// pointer is treated as immutable; the emitter reloads them at each access.
pub(crate) fn plain_array_offsets() -> Option<PlainArrayOffsets> {
    use crate::tagged::{
        header::{VecLikeHeader, VectorObj},
        value::TaggedValue,
    };
    if size_of::<usize>() != 8 || size_of::<TaggedValue>() != 8 {
        return None;
    }
    let maximum_length = isize::MAX as usize / size_of::<TaggedValue>();
    if maximum_length > TaggedValue::MOST_POSITIVE_FIXNUM as usize {
        return None;
    }
    let (pointer, length) = value_vec_slice_offsets()?;
    let data = offset_of!(VectorObj, data);
    Some(PlainArrayOffsets {
        type_tag: i32::try_from(offset_of!(VecLikeHeader, type_tag)).ok()?,
        slots: i32::try_from(data.checked_add(pointer)?).ok()?,
        length: i32::try_from(data.checked_add(length)?).ok()?,
        element_shift: i64::from(size_of::<TaggedValue>().trailing_zeros()),
    })
}
