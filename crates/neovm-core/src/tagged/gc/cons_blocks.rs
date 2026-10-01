//! The cons block allocator: 64 KB-aligned blocks of ConsCell with packed mark bits, GNU alloc.c's cons_block shape.
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

/// GNU Emacs keeps conses in fixed-size aligned blocks and derives the owning
/// block/index directly from the cons pointer. Keep the same shape here so
/// mark/ownership checks stay O(1) instead of linearly scanning `cons_blocks`.
pub(super) const CONS_BLOCK_ALIGN: usize = CONS_BLOCK_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConsBlockCacheEntry {
    pub(super) block_base: usize,
    pub(super) block_index: usize,
}

impl ConsBlockCacheEntry {
    pub(super) fn new(block_base: usize, block_index: usize) -> Self {
        Self {
            block_base,
            block_index,
        }
    }
}

/// A GNU-shaped cons block with cells at the front of a fixed-size aligned
/// storage area, followed by the mark, old and unlogged bitmaps.
pub(super) struct ConsBlock {
    /// Aligned raw storage for cons cells plus `ConsBlockTrailer`.
    pub(super) storage: *mut u8,
    /// Index of the first never-allocated cell in this block.
    pub(super) next_index: u16,
}

impl ConsBlock {
    pub(super) fn layout() -> Layout {
        Layout::from_size_align(CONS_BLOCK_BYTES, CONS_BLOCK_ALIGN).expect("cons block layout")
    }

    pub(super) fn new() -> Self {
        let layout = Self::layout();
        let storage = unsafe { alloc::alloc_zeroed(layout) };
        if storage.is_null() {
            alloc::handle_alloc_error(layout);
        }
        Self {
            storage,
            next_index: 0,
        }
    }

    #[inline]
    pub(super) fn base_addr(&self) -> usize {
        self.storage as usize
    }

    #[inline]
    pub(super) fn cells_ptr(&self) -> *mut ConsCell {
        self.storage.cast()
    }

    #[inline]
    pub(super) fn trailer(&self) -> &ConsBlockTrailer {
        // SAFETY: the block owns this storage for the duration of the borrow.
        unsafe { ConsBlockTrailer::from_block_base(self.base_addr()) }
    }

    #[inline]
    pub(super) fn block_base_for_ptr(ptr: *const ConsCell) -> usize {
        (ptr as usize) & !(CONS_BLOCK_ALIGN - 1)
    }

    #[inline]
    pub(super) fn ptr_offset(ptr: *const ConsCell) -> usize {
        (ptr as usize).saturating_sub(Self::block_base_for_ptr(ptr))
    }

    #[inline]
    pub(super) fn ptr_is_cell_aligned(ptr: *const ConsCell) -> bool {
        let offset = Self::ptr_offset(ptr);
        offset < CONS_CELLS_BYTES && offset.is_multiple_of(size_of::<ConsCell>())
    }

    #[inline]
    pub(super) fn index_of_ptr(ptr: *const ConsCell) -> usize {
        Self::ptr_offset(ptr) / size_of::<ConsCell>()
    }

    #[inline]
    pub(super) fn mark_bit(index: usize) -> ConsMarkBit {
        ConsBlockTrailer::mark_bit(index)
    }

    /// View a mark-bitmap word as an atomic. The cons mark bits are accessed
    /// atomically (relaxed) so a future concurrent GC thread can set them while
    /// the mutator allocate-blacks / reads them without a data race; on x86 a
    /// relaxed atomic load/store is a plain mov, so this is free single-threaded.
    #[inline]
    pub(super) fn mark_word(&self, word_index: usize) -> &AtomicUsize {
        self.trailer().mark_word(word_index)
    }

    #[inline]
    pub(super) fn is_marked_ptr(&self, ptr: *const ConsCell) -> bool {
        self.trailer().is_marked(Self::index_of_ptr(ptr))
    }

    /// Mark the cell at `offset` bytes into this block's cells, reporting
    /// whether the bit was newly set.
    ///
    /// The caller has already decomposed the address, and the test and the set
    /// share one index and one mark word. Going through `is_marked_ptr` and
    /// then `mark_ptr` recomputed the block base, the offset, the cell index
    /// and the bit mask a second time -- on 26.4M marked conses per
    /// rust-lsp-typing capture.
    #[inline]
    pub(super) fn mark_cell_offset(&mut self, offset: usize) -> bool {
        self.trailer().mark_cell(offset / size_of::<ConsCell>())
    }

    /// Reserve up to `want` never-used cells from this block's bump cursor
    /// as one run (an allocation region): answers `(first cell index,
    /// count)`, `count == 0` when the block is full. The cursor only: the
    /// allocator writes car and cdr at hand-out, for cells from every source
    /// (GNU's `cons_block_index` bump is likewise just the cursor).
    #[inline]
    pub(super) fn reserve_tail(&mut self, want: usize) -> (usize, usize) {
        let first = self.next_index as usize;
        let n = want.min(CONS_BLOCK_SIZE - first);
        self.next_index = (first + n) as u16;
        (first, n)
    }

    /// Give the tail `[index, next_index)` back to the bump cursor (an
    /// allocation region's unused tail). Its mark bits must already be
    /// clear: bits at or above `next_index` are never set.
    pub(super) fn rewind_tail(&mut self, index: usize) {
        debug_assert!(index <= self.next_index as usize);
        self.next_index = index as u16;
    }

    /// Set (`set == true`) or clear the mark bits of cells
    /// `[start, start + n)` of the block whose storage begins at `base`,
    /// a word at a time with the same relaxed atomics the marker uses
    /// (`mark_cell_offset`): an allocation region pre-marked black at grant,
    /// or its unused tail unmarked at close. Needs no `&mut` block, so a
    /// region names its block by address alone.
    pub(super) fn mark_run_at(base: usize, start: usize, n: usize, set: bool) {
        // SAFETY: the allocation region names an owned block held live for
        // the duration of its extent. Its tail ranges stay inside the cells.
        unsafe { ConsBlockTrailer::from_block_base(base) }.mark_run(start, n, set);
    }

    /// Clear all mark bits used by this block. Runs stop-the-world (at
    /// `begin_collection`), but stores atomically so the representation stays
    /// consistent with the concurrent reads/writes elsewhere.
    pub(super) fn clear_marks(&mut self) {
        self.trailer()
            .clear_marks_world_stopped(self.next_index as usize);
    }

    /// Count currently-marked (live) cells via mark-bitmap popcount. Bits at or
    /// above `next_index` are never set, so popcounting the used words is exact.
    /// Cheap O(cells/64); used to recompute the live count after an incremental
    /// sweep without a second cell walk.
    pub(super) fn count_marked(&self) -> usize {
        self.trailer().count_marked(self.next_index as usize)
    }

    /// Sweep: thread reclaimed cells into the global intrusive free list and
    /// return the number of live cells in this block.
    pub(super) fn sweep(&mut self, free_list: &mut *mut ConsCell) -> usize {
        let mut live = 0;

        // Match GNU alloc.c: reclaimed conses are linked through the dead
        // cells themselves instead of rebuilding an external index vector.
        for i in (0..self.next_index as usize).rev() {
            let cell = unsafe { self.cells_ptr().add(i) };
            if self.trailer().is_marked(i) {
                live += 1;
            } else {
                unsafe {
                    (*cell).set_free_next(*free_list);
                }
                *free_list = cell;
            }
        }

        live
    }
}

impl Drop for ConsBlock {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.storage, Self::layout()) };
    }
}
