//! Sealed descriptors for pdump/image-mapped GC ranges and objects (value-gc C8, G1).
//!
//! Keeping raw pointer and length fields private and gating construction
//! behind `unsafe fn new` ensures safe code in `tagged::gc` cannot construct
//! or mutate a mapped-object entry with an arbitrary pointer/length that the
//! collector later dereferences during mark, sweep, or inventory walks.

use super::{
    CONS_MARK_BITS_PER_WORD, ConsBlock, ConsCell, FloatObj, StringObj, VecLikeHeader,
    cons_mark_words,
};
use std::mem::size_of;

pub struct MappedConsRange {
    start: *mut ConsCell,
    len: usize,
    mark_bits: Vec<usize>,
}

impl MappedConsRange {
    /// Wrap a pdump-mapped array of `len` contiguous `ConsCell`s starting at
    /// `start`.
    ///
    /// # Safety
    /// If `len > 0`, `start` must be non-null, `ConsCell`-aligned, and point
    /// to `len` initialized `ConsCell` objects that remain mapped and live for
    /// the lifetime of the owning heap (with `len * size_of::<ConsCell>()` not
    /// overflowing `usize` or the allocation's address range).
    pub unsafe fn new(start: *mut ConsCell, len: usize) -> Self {
        Self {
            start,
            len,
            mark_bits: vec![0; cons_mark_words(len)],
        }
    }

    #[inline]
    pub fn start(&self) -> *mut ConsCell {
        self.start
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn contains_ptr(&self, ptr: *const ConsCell) -> bool {
        if ptr.is_null() || self.len == 0 {
            return false;
        }
        let start = self.start as usize;
        let end = start + self.len * size_of::<ConsCell>();
        let ptr = ptr as usize;
        start <= ptr && ptr < end && (ptr - start).is_multiple_of(size_of::<ConsCell>())
    }

    #[inline]
    pub fn index_of_ptr(&self, ptr: *const ConsCell) -> usize {
        (ptr as usize - self.start as usize) / size_of::<ConsCell>()
    }

    #[inline]
    pub fn is_marked_ptr(&self, ptr: *const ConsCell) -> bool {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        (self.mark_bits[mark.word_index] & mark.mask) != 0
    }

    #[inline]
    pub fn mark_ptr(&mut self, ptr: *const ConsCell) {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        self.mark_bits[mark.word_index] |= mark.mask;
    }

    pub fn clear_marks(&mut self) {
        self.mark_bits.fill(0);
    }

    /// Mark every cell in the range live (dump-partition: born black). Sets
    /// exactly `len` bits so `live_count` stays exact.
    pub fn mark_all(&mut self) {
        self.mark_bits.fill(!0);
        let rem = self.len % CONS_MARK_BITS_PER_WORD;
        if rem != 0
            && let Some(last) = self.mark_bits.last_mut()
        {
            *last = (1usize << rem) - 1;
        }
    }

    pub fn live_count(&self) -> usize {
        self.mark_bits
            .iter()
            .enumerate()
            .map(|(word_index, word)| {
                let full_words = self.len / CONS_MARK_BITS_PER_WORD;
                let tail_bits = self.len % CONS_MARK_BITS_PER_WORD;
                if word_index < full_words || tail_bits == 0 {
                    word.count_ones() as usize
                } else {
                    let mask = (1usize << tail_bits) - 1;
                    (word & mask).count_ones() as usize
                }
            })
            .sum()
    }
}

pub struct MappedFloatRange {
    start: *mut FloatObj,
    len: usize,
    mark_bits: Vec<usize>,
}

impl MappedFloatRange {
    /// Wrap a pdump-mapped array of `len` contiguous `FloatObj`s starting at
    /// `start`.
    ///
    /// # Safety
    /// If `len > 0`, `start` must be non-null, `FloatObj`-aligned, and point
    /// to `len` initialized `FloatObj` objects that remain mapped and live for
    /// the lifetime of the owning heap (with `len * size_of::<FloatObj>()` not
    /// overflowing `usize` or the allocation's address range).
    pub unsafe fn new(start: *mut FloatObj, len: usize) -> Self {
        Self {
            start,
            len,
            mark_bits: vec![0; cons_mark_words(len)],
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn contains_ptr(&self, ptr: *const FloatObj) -> bool {
        if ptr.is_null() || self.len == 0 {
            return false;
        }
        let start = self.start as usize;
        let end = start + self.len * size_of::<FloatObj>();
        let ptr = ptr as usize;
        start <= ptr && ptr < end && (ptr - start).is_multiple_of(size_of::<FloatObj>())
    }

    #[inline]
    pub fn index_of_ptr(&self, ptr: *const FloatObj) -> usize {
        (ptr as usize - self.start as usize) / size_of::<FloatObj>()
    }

    #[inline]
    pub fn is_marked_ptr(&self, ptr: *const FloatObj) -> bool {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        (self.mark_bits[mark.word_index] & mark.mask) != 0
    }

    #[inline]
    pub fn mark_ptr(&mut self, ptr: *const FloatObj) {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        self.mark_bits[mark.word_index] |= mark.mask;
    }

    pub fn clear_marks(&mut self) {
        self.mark_bits.fill(0);
    }

    /// Mark every cell in the range live (dump-partition: born black). Sets
    /// exactly `len` bits so `live_count` stays exact.
    pub fn mark_all(&mut self) {
        self.mark_bits.fill(!0);
        let rem = self.len % CONS_MARK_BITS_PER_WORD;
        if rem != 0
            && let Some(last) = self.mark_bits.last_mut()
        {
            *last = (1usize << rem) - 1;
        }
    }

    pub fn live_count(&self) -> usize {
        self.mark_bits
            .iter()
            .enumerate()
            .map(|(word_index, word)| {
                let full_words = self.len / CONS_MARK_BITS_PER_WORD;
                let tail_bits = self.len % CONS_MARK_BITS_PER_WORD;
                if word_index < full_words || tail_bits == 0 {
                    word.count_ones() as usize
                } else {
                    let mask = (1usize << tail_bits) - 1;
                    (word & mask).count_ones() as usize
                }
            })
            .sum()
    }
}

pub struct MappedVecLikeObject {
    header: *mut VecLikeHeader,
    byte_len: usize,
    marked: bool,
}

impl MappedVecLikeObject {
    /// Wrap a pdump-mapped vectorlike object at `header` spanning `byte_len`
    /// bytes.
    ///
    /// # Safety
    /// `header` must be non-null, `VecLikeHeader`-aligned, and point to a
    /// complete, initialized vectorlike object matching its `type_tag` that
    /// remains mapped and live for the lifetime of the owning heap.
    pub unsafe fn new(header: *mut VecLikeHeader, byte_len: usize) -> Self {
        Self {
            header,
            byte_len,
            marked: false,
        }
    }

    #[inline]
    pub fn header(&self) -> *mut VecLikeHeader {
        self.header
    }

    #[inline]
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    #[inline]
    pub fn is_marked(&self) -> bool {
        self.marked
    }

    #[inline]
    pub fn set_marked(&mut self, marked: bool) {
        self.marked = marked;
    }
}

pub struct MappedStringObject {
    ptr: *mut StringObj,
    byte_len: usize,
    marked: bool,
}

impl MappedStringObject {
    /// Wrap a pdump-mapped string object at `ptr` spanning `byte_len` bytes.
    ///
    /// # Safety
    /// `ptr` must be non-null, `StringObj`-aligned, and point to a complete,
    /// initialized `StringObj` that remains mapped and live for the lifetime
    /// of the owning heap.
    pub unsafe fn new(ptr: *mut StringObj, byte_len: usize) -> Self {
        Self {
            ptr,
            byte_len,
            marked: false,
        }
    }

    #[inline]
    pub fn ptr(&self) -> *mut StringObj {
        self.ptr
    }

    #[inline]
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    #[inline]
    pub fn is_marked(&self) -> bool {
        self.marked
    }

    #[inline]
    pub fn set_marked(&mut self, marked: bool) {
        self.marked = marked;
    }
}
