//! The single owner of the 64 KiB cons block layout (P3.1 C2.3).
//! Watch bits belong in a side table, never in this trailer.

use super::{ConsCell, size_of};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub(crate) const CONS_BLOCK_BYTES: usize = 64 * 1024;
pub(super) const CONS_MARK_BITS_PER_WORD: usize = usize::BITS as usize;

pub(super) const fn cons_mark_words(cells: usize) -> usize {
    cells.div_ceil(CONS_MARK_BITS_PER_WORD)
}

const fn cell_count() -> usize {
    let mut cells = CONS_BLOCK_BYTES / size_of::<ConsCell>();
    while cells * size_of::<ConsCell>()
        + cons_mark_words(cells) * (size_of::<AtomicUsize>() + 2 * size_of::<AtomicU64>())
        > CONS_BLOCK_BYTES
    {
        cells -= 1;
    }
    cells
}

pub(crate) const CONS_BLOCK_SIZE: usize = cell_count();
pub(crate) const CONS_MARK_WORDS: usize = cons_mark_words(CONS_BLOCK_SIZE);
pub(super) const CONS_CELLS_BYTES: usize = CONS_BLOCK_SIZE * size_of::<ConsCell>();
pub(crate) const CONS_MARKS_OFFSET: usize = CONS_CELLS_BYTES;
pub(crate) const CONS_OLD_OFFSET: usize =
    CONS_MARKS_OFFSET + CONS_MARK_WORDS * size_of::<AtomicUsize>();
pub(crate) const CONS_UNLOGGED_OFFSET: usize =
    CONS_OLD_OFFSET + CONS_MARK_WORDS * size_of::<AtomicU64>();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConsMarkBit {
    pub(super) word_index: usize,
    pub(super) mask: usize,
}

#[repr(C)]
pub(super) struct ConsBlockTrailer {
    mark: [AtomicUsize; CONS_MARK_WORDS],
    old: [AtomicU64; CONS_MARK_WORDS],
    unlogged: [AtomicU64; CONS_MARK_WORDS],
}

// The generation methods land with the layout, ahead of their C2.4/C2.5 users.
#[allow(dead_code)]
impl ConsBlockTrailer {
    /// Obtain a live block's trailer without aliasing any of its cons cells.
    ///
    /// # Safety
    /// `base` must name an allocated, aligned owned cons block, and its
    /// storage must outlive the returned reference. Dump conses do not have
    /// this trailer. All access to these words uses their atomic types.
    #[inline]
    pub(super) unsafe fn from_block_base<'a>(base: usize) -> &'a Self {
        unsafe { &*((base + CONS_MARKS_OFFSET) as *const Self) }
    }

    #[inline]
    pub(super) fn mark_bit(index: usize) -> ConsMarkBit {
        ConsMarkBit {
            word_index: index / CONS_MARK_BITS_PER_WORD,
            mask: 1usize << (index % CONS_MARK_BITS_PER_WORD),
        }
    }

    /// Atomic representation and unchecked indexing preserve the existing
    /// mark hot path: a relaxed load remains a plain load on x86-64.
    #[inline]
    pub(super) fn mark_word(&self, word_index: usize) -> &AtomicUsize {
        debug_assert!(word_index < CONS_MARK_WORDS);
        // SAFETY: every caller derives a word of this live block's cells.
        // Match the original raw-word accessor without adding an array
        // bounds assumption to the legacy mark/sweep loops.
        unsafe { &*self.mark.as_ptr().add(word_index) }
    }

    #[inline]
    pub(super) fn is_marked(&self, index: usize) -> bool {
        let bit = Self::mark_bit(index);
        (self.mark_word(bit.word_index).load(Ordering::Relaxed) & bit.mask) != 0
    }

    /// Mutator marking keeps its existing load-before-RMW fast path.
    #[inline]
    pub(super) fn mark_cell(&self, index: usize) -> bool {
        let bit = Self::mark_bit(index);
        let word = self.mark_word(bit.word_index);
        if word.load(Ordering::Relaxed) & bit.mask != 0 {
            return false;
        }
        word.fetch_or(bit.mask, Ordering::Relaxed);
        true
    }

    /// Concurrent claiming must decide ownership from the RMW's old value.
    #[inline]
    pub(super) fn try_mark(&self, index: usize) -> bool {
        let bit = Self::mark_bit(index);
        (self
            .mark_word(bit.word_index)
            .fetch_or(bit.mask, Ordering::Relaxed)
            & bit.mask)
            == 0
    }

    pub(super) fn mark_run(&self, start: usize, n: usize, set: bool) {
        debug_assert!(start + n <= CONS_BLOCK_SIZE);
        let end = start + n;
        let mut i = start;
        while i < end {
            let bit = i % CONS_MARK_BITS_PER_WORD;
            let span = (CONS_MARK_BITS_PER_WORD - bit).min(end - i);
            let mask = if span == CONS_MARK_BITS_PER_WORD {
                usize::MAX
            } else {
                ((1usize << span) - 1) << bit
            };
            let word = self.mark_word(i / CONS_MARK_BITS_PER_WORD);
            if set {
                word.fetch_or(mask, Ordering::Relaxed);
            } else {
                word.fetch_and(!mask, Ordering::Relaxed);
            }
            i += span;
        }
    }

    /// The mutator may reset marks only after the stop-all handshake has
    /// joined the marker and closed allocation regions. Atomic stores retain
    /// the representation used by all other mark accesses.
    pub(super) fn clear_marks_world_stopped(&self, cells: usize) {
        for w in 0..cons_mark_words(cells) {
            self.mark_word(w).store(0, Ordering::Relaxed);
        }
    }

    pub(super) fn count_marked(&self, cells: usize) -> usize {
        (0..cons_mark_words(cells))
            .map(|w| self.mark_word(w).load(Ordering::Relaxed).count_ones() as usize)
            .sum()
    }

    /// An old cell is live throughout a minor even when it was not traced;
    /// newly allocated sweep-window cells survive through their mark bit.
    #[inline]
    pub(super) fn is_live_generational(&self, index: usize) -> bool {
        let bit = Self::mark_bit(index);
        let live = self.old_word(bit.word_index)
            | self.mark_word(bit.word_index).load(Ordering::Relaxed) as u64;
        live & bit.mask as u64 != 0
    }

    /// Count the union, rather than adding old and marked populations: a
    /// just-promoted cell still has both bits until the next mark begins.
    pub(super) fn count_live_generational(&self, cells: usize) -> usize {
        (0..cons_mark_words(cells))
            .map(|w| {
                (self.old_word(w) | self.mark_word(w).load(Ordering::Relaxed) as u64).count_ones()
                    as usize
            })
            .sum()
    }

    #[inline]
    fn generation_bit(index: usize) -> (usize, u64) {
        debug_assert!(index < CONS_BLOCK_SIZE);
        (
            index / u64::BITS as usize,
            1u64 << (index % u64::BITS as usize),
        )
    }

    #[inline]
    fn old_atomic(&self, word_index: usize) -> &AtomicU64 {
        debug_assert!(word_index < CONS_MARK_WORDS);
        // SAFETY: the word index belongs to this live block's cells.
        unsafe { self.old.get_unchecked(word_index) }
    }

    #[inline]
    fn unlogged_atomic(&self, word_index: usize) -> &AtomicU64 {
        debug_assert!(word_index < CONS_MARK_WORDS);
        // SAFETY: the word index belongs to this live block's cells.
        unsafe { self.unlogged.get_unchecked(word_index) }
    }

    #[inline]
    pub(super) fn old_word(&self, word_index: usize) -> u64 {
        self.old_atomic(word_index).load(Ordering::Relaxed)
    }

    #[inline]
    pub(super) fn unlogged_word(&self, word_index: usize) -> u64 {
        self.unlogged_atomic(word_index).load(Ordering::Relaxed)
    }

    #[inline]
    pub(super) fn is_old(&self, index: usize) -> bool {
        let (word, mask) = Self::generation_bit(index);
        self.old_word(word) & mask != 0
    }

    #[inline]
    pub(super) fn is_unlogged(&self, index: usize) -> bool {
        let (word, mask) = Self::generation_bit(index);
        self.unlogged_word(word) & mask != 0
    }

    #[inline]
    pub(super) fn set_old(&self, index: usize) {
        let (word, mask) = Self::generation_bit(index);
        self.old_atomic(word).fetch_or(mask, Ordering::Relaxed);
    }

    #[inline]
    pub(super) fn clear_old(&self, index: usize) {
        let (word, mask) = Self::generation_bit(index);
        self.old_atomic(word).fetch_and(!mask, Ordering::Relaxed);
    }

    #[inline]
    pub(super) fn set_unlogged(&self, index: usize) {
        let (word, mask) = Self::generation_bit(index);
        self.unlogged_atomic(word).fetch_or(mask, Ordering::Relaxed);
    }

    /// Claim one owner's logging responsibility without losing adjacent bits
    /// to a concurrent update of the same bitmap word.
    #[inline]
    pub(super) fn try_claim_unlogged(&self, index: usize) -> bool {
        let (word, mask) = Self::generation_bit(index);
        self.unlogged_atomic(word)
            .fetch_and(!mask, Ordering::Relaxed)
            & mask
            != 0
    }

    /// Stop-all handshake required: the marker must be joined and every
    /// mutation extent and allocation region closed before promotion/reset.
    /// There are no concurrent bit updates to merge while these stores run.
    /// Return only newly-old cells, so repeated promotions never charge the
    /// old-generation accounting for a cell twice.
    #[cold]
    #[inline(never)]
    pub(super) fn promote_marked_world_stopped(&self, cells: usize) -> usize {
        let mut promoted = 0usize;
        for w in 0..cons_mark_words(cells) {
            let previous_old = self.old_word(w);
            let marked = self.mark_word(w).load(Ordering::Relaxed) as u64;
            promoted += (marked & !previous_old).count_ones() as usize;
            let old = previous_old | marked;
            self.old_atomic(w).store(old, Ordering::Relaxed);
            self.unlogged_atomic(w).store(old, Ordering::Relaxed);
        }
        promoted
    }

    /// Stop-all and joined-marker boundary: major liveness replaces age.
    /// Unused region tails have already been unmarked. Whole-word stores
    /// are safe because no mutator can claim or update adjacent bits here.
    #[cold]
    #[inline(never)]
    pub(super) fn promote_major_world_stopped(&self) -> usize {
        let mut promoted = 0;
        for w in 0..CONS_MARK_WORDS {
            let marked = self.mark_word(w).load(Ordering::Relaxed) as u64;
            promoted += (marked & !self.old_word(w)).count_ones() as usize;
            self.old_atomic(w).store(marked, Ordering::Relaxed);
            self.unlogged_atomic(w).store(marked, Ordering::Relaxed);
        }
        promoted
    }

    pub(super) fn count_old(&self, cells: usize) -> usize {
        (0..cons_mark_words(cells))
            .map(|w| self.old_word(w).count_ones() as usize)
            .sum()
    }

    /// Diagnostic live-word view; all generation arithmetic stays here.
    pub(super) fn live_word(&self, word: usize, minor: bool) -> usize {
        let marked = self.mark_word(word).load(Ordering::Relaxed);
        if minor {
            marked | self.old_word(word) as usize
        } else {
            marked
        }
    }

    /// Stop-all handshake required, as for promotion: no mutator or marker
    /// may claim or set an unlogged bit while this reset runs.
    #[cold]
    #[inline(never)]
    pub(super) fn reset_unlogged_world_stopped(&self, cells: usize) {
        for w in 0..cons_mark_words(cells) {
            self.unlogged_atomic(w)
                .store(self.old_word(w), Ordering::Relaxed);
        }
    }
}

const _: () = {
    assert!(usize::BITS == u64::BITS);
    assert!(CONS_BLOCK_SIZE == 4001);
    assert!(CONS_CELLS_BYTES + size_of::<ConsBlockTrailer>() <= CONS_BLOCK_BYTES);
    assert!(CONS_OLD_OFFSET == CONS_CELLS_BYTES + std::mem::offset_of!(ConsBlockTrailer, old));
    assert!(
        CONS_UNLOGGED_OFFSET == CONS_CELLS_BYTES + std::mem::offset_of!(ConsBlockTrailer, unlogged)
    );
};

#[cfg(test)]
#[path = "tests/cons_trailer_test.rs"]
mod cons_trailer_tests;
