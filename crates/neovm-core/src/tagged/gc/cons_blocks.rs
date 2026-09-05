//! The cons block allocator: 64 KB-aligned blocks of ConsCell with packed mark bits, GNU alloc.c's cons_block shape.
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

/// GNU Emacs keeps conses in fixed-size aligned blocks and derives the owning
/// block/index directly from the cons pointer. Keep the same shape here so
/// mark/ownership checks stay O(1) instead of linearly scanning `cons_blocks`.
pub(super) const CONS_BLOCK_BYTES: usize = 64 * 1024;
pub(super) const CONS_BLOCK_ALIGN: usize = CONS_BLOCK_BYTES;
pub(super) const CONS_MARK_BITS_PER_WORD: usize = usize::BITS as usize;

pub(super) const fn cons_mark_words(cell_count: usize) -> usize {
    cell_count.div_ceil(CONS_MARK_BITS_PER_WORD)
}

pub(super) const fn cons_block_cell_count() -> usize {
    let cons_size = size_of::<ConsCell>();
    let mark_word_size = size_of::<usize>();
    let mut cells = CONS_BLOCK_BYTES / cons_size;
    while cells > 0 {
        let marks_bytes = cons_mark_words(cells) * mark_word_size;
        if cells * cons_size + marks_bytes <= CONS_BLOCK_BYTES {
            return cells;
        }
        cells -= 1;
    }
    0
}

pub(super) const CONS_BLOCK_SIZE: usize = cons_block_cell_count();
pub(super) const CONS_MARK_WORDS: usize = cons_mark_words(CONS_BLOCK_SIZE);
pub(super) const CONS_CELLS_BYTES: usize = CONS_BLOCK_SIZE * size_of::<ConsCell>();
pub(super) const CONS_MARKS_OFFSET: usize = CONS_CELLS_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConsMarkBit {
    pub(super) word_index: usize,
    pub(super) mask: usize,
}

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
/// storage area, followed by packed mark bits.
pub(super) struct ConsBlock {
    /// Aligned raw storage for cons cells plus mark bits.
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
    pub(super) fn mark_words_ptr(&self) -> *mut usize {
        unsafe { self.storage.add(CONS_MARKS_OFFSET).cast() }
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
        let word = index / CONS_MARK_BITS_PER_WORD;
        let bit = index % CONS_MARK_BITS_PER_WORD;
        ConsMarkBit {
            word_index: word,
            mask: 1usize << bit,
        }
    }

    /// View a mark-bitmap word as an atomic. The cons mark bits are accessed
    /// atomically (relaxed) so a future concurrent GC thread can set them while
    /// the mutator allocate-blacks / reads them without a data race; on x86 a
    /// relaxed atomic load/store is a plain mov, so this is free single-threaded.
    #[inline]
    pub(super) fn mark_word(&self, word_index: usize) -> &AtomicUsize {
        unsafe { &*(self.mark_words_ptr().add(word_index) as *const AtomicUsize) }
    }

    #[inline]
    pub(super) fn is_marked_ptr(&self, ptr: *const ConsCell) -> bool {
        let index = Self::index_of_ptr(ptr);
        let mark = Self::mark_bit(index);
        debug_assert!(mark.word_index < CONS_MARK_WORDS);
        (self.mark_word(mark.word_index).load(Ordering::Relaxed) & mark.mask) != 0
    }

    #[inline]
    pub(super) fn mark_ptr(&mut self, ptr: *const ConsCell) {
        let index = Self::index_of_ptr(ptr);
        let mark = Self::mark_bit(index);
        debug_assert!(mark.word_index < CONS_MARK_WORDS);
        self.mark_word(mark.word_index)
            .fetch_or(mark.mask, Ordering::Relaxed);
    }

    /// Allocate a fresh cons cell from this block's bump cursor.
    /// Returns None if the block has no never-used cells left.
    pub(super) fn alloc_bump(
        &mut self,
        car: TaggedValue,
        cdr: TaggedValue,
    ) -> Option<*mut ConsCell> {
        if self.next_index as usize >= CONS_BLOCK_SIZE {
            return None;
        }
        let idx = self.next_index;
        self.next_index += 1;
        let cell = unsafe { self.cells_ptr().add(idx as usize) };
        unsafe {
            (*cell).set_car(car);
            (*cell).set_cdr(cdr);
        }
        Some(cell)
    }

    /// Clear all mark bits used by this block. Runs stop-the-world (at
    /// `begin_collection`), but stores atomically so the representation stays
    /// consistent with the concurrent reads/writes elsewhere.
    pub(super) fn clear_marks(&mut self) {
        let used_words = cons_mark_words(self.next_index as usize);
        for w in 0..used_words {
            self.mark_word(w).store(0, Ordering::Relaxed);
        }
    }

    /// Count currently-marked (live) cells via mark-bitmap popcount. Bits at or
    /// above `next_index` are never set, so popcounting the used words is exact.
    /// Cheap O(cells/64); used to recompute the live count after an incremental
    /// sweep without a second cell walk.
    pub(super) fn count_marked(&self) -> usize {
        let used_words = cons_mark_words(self.next_index as usize);
        let mut live = 0usize;
        for w in 0..used_words {
            live += self.mark_word(w).load(Ordering::Relaxed).count_ones() as usize;
        }
        live
    }

    /// Sweep: thread reclaimed cells into the global intrusive free list and
    /// return the number of live cells in this block.
    pub(super) fn sweep(&mut self, free_list: &mut *mut ConsCell) -> usize {
        let mut live = 0;

        // Match GNU alloc.c: reclaimed conses are linked through the dead
        // cells themselves instead of rebuilding an external index vector.
        for i in (0..self.next_index as usize).rev() {
            let cell = unsafe { self.cells_ptr().add(i) };
            let mark = Self::mark_bit(i);
            let marked = (self.mark_word(mark.word_index).load(Ordering::Relaxed) & mark.mask) != 0;
            if marked {
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
