//! The single owner of the 64 KiB cons block layout (P3.1 C2.3).
//! Watch bits belong in a side table, never in this trailer.

use super::{ConsCell, size_of};
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

pub(crate) const CONS_BLOCK_BYTES: usize = 64 * 1024;
pub(super) const CONS_MARK_BITS_PER_WORD: usize = usize::BITS as usize;

pub(super) const fn cons_mark_words(cells: usize) -> usize {
    cells.div_ceil(CONS_MARK_BITS_PER_WORD)
}

/// Each mark word covers 64 cells, which own a mark bit, an old bit and an
/// age byte each; the age bytes are padded to whole words.
const fn cell_count() -> usize {
    let mut cells = CONS_BLOCK_BYTES / size_of::<ConsCell>();
    while cells * size_of::<ConsCell>()
        + cons_mark_words(cells)
            * (size_of::<AtomicUsize>() + size_of::<AtomicU64>() + CONS_MARK_BITS_PER_WORD)
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
pub(super) const CONS_OLD_OFFSET: usize =
    CONS_MARKS_OFFSET + CONS_MARK_WORDS * size_of::<AtomicUsize>();
pub(crate) const CONS_AGE_OFFSET: usize =
    CONS_OLD_OFFSET + CONS_MARK_WORDS * size_of::<AtomicU64>();
const CONS_AGE_BYTES: usize = CONS_MARK_WORDS * CONS_MARK_BITS_PER_WORD;
/// Age bytes written as one word, eight cells at a time.
const AGE_LANE_CELLS: usize = size_of::<u64>();
const AGE_LANES_PER_WORD: usize = CONS_MARK_BITS_PER_WORD / AGE_LANE_CELLS;

/// A cell's age byte, the per-cell mirror of its old bit. Compiled GEN1
/// stores test it with one byte load; the collector keeps working on whole
/// old-bitmap words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ConsAge {
    Young = 0,
    Old = 1,
}

/// `SPREAD_AGES[b]` has byte i set to `ConsAge::Old` exactly where bit i of
/// `b` is set: eight cells' old bits as eight age bytes.
const SPREAD_AGES: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut bits = 0;
    while bits < 256 {
        let mut cell = 0;
        while cell < AGE_LANE_CELLS {
            if bits >> cell & 1 != 0 {
                table[bits] |= (ConsAge::Old as u64) << (cell * 8);
            }
            cell += 1;
        }
        bits += 1;
    }
    table
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConsMarkBit {
    pub(super) word_index: usize,
    pub(super) mask: usize,
}

/// The old bitmap is the collector's view of age; the age bytes mirror it
/// cell by cell for the mutator's store test. Both change together and only
/// at a stop-all promotion with the marker joined, so the word-wide writes
/// of the mirror never race with a byte read.
#[repr(C)]
pub(super) struct ConsBlockTrailer {
    mark: [AtomicUsize; CONS_MARK_WORDS],
    old: [AtomicU64; CONS_MARK_WORDS],
    age: [AtomicU8; CONS_AGE_BYTES],
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
    fn old_atomic(&self, word_index: usize) -> &AtomicU64 {
        debug_assert!(word_index < CONS_MARK_WORDS);
        // SAFETY: the word index belongs to this live block's cells.
        unsafe { self.old.get_unchecked(word_index) }
    }

    #[inline]
    fn age(&self, index: usize) -> &AtomicU8 {
        debug_assert!(index < CONS_AGE_BYTES);
        // SAFETY: the index belongs to this live block's cells or the
        // padding of their last word.
        unsafe { self.age.get_unchecked(index) }
    }

    /// The age bytes of `lane` of mark word `word_index` as one word.
    #[inline]
    fn age_lane(&self, word_index: usize, lane: usize) -> &AtomicU64 {
        debug_assert!(word_index < CONS_MARK_WORDS && lane < AGE_LANES_PER_WORD);
        let first = word_index * CONS_MARK_BITS_PER_WORD + lane * AGE_LANE_CELLS;
        // SAFETY: the eight bytes lie inside `age`, word-aligned (asserted
        // below). The mirror is written only at a stop-all promotion, so a
        // word-wide access never races with a byte access.
        unsafe { AtomicU64::from_ptr(self.age.as_ptr().add(first).cast::<u64>().cast_mut()) }
    }

    #[inline]
    pub(super) fn old_word(&self, word_index: usize) -> u64 {
        self.old_atomic(word_index).load(Ordering::Relaxed)
    }

    /// The collector's test: one word covers 64 cells, so its scans and
    /// marking stay as dense in cache as before the mirror.
    #[inline]
    pub(super) fn is_old(&self, index: usize) -> bool {
        let bit = Self::mark_bit(index);
        self.old_word(bit.word_index) & bit.mask as u64 != 0
    }

    /// The mutator's test, as compiled stores make it: one byte load from
    /// the mirror, which always equals the old bit while a mutator runs.
    #[inline]
    pub(super) fn age_is_old(&self, index: usize) -> bool {
        debug_assert!(index < CONS_BLOCK_SIZE);
        self.age(index).load(Ordering::Relaxed) != ConsAge::Young as u8
    }

    /// Test setup only: production age changes are the two promotions,
    /// so no other writer can bypass the mirror. Writes both copies.
    #[cfg(test)]
    pub(super) fn set_old(&self, index: usize) {
        let bit = Self::mark_bit(index);
        self.old_atomic(bit.word_index)
            .fetch_or(bit.mask as u64, Ordering::Relaxed);
        self.age(index).store(ConsAge::Old as u8, Ordering::Relaxed);
    }

    /// Test setup only, as `set_old`. Writes both copies.
    #[cfg(test)]
    pub(super) fn clear_old(&self, index: usize) {
        let bit = Self::mark_bit(index);
        self.old_atomic(bit.word_index)
            .fetch_and(!(bit.mask as u64), Ordering::Relaxed);
        self.age(index)
            .store(ConsAge::Young as u8, Ordering::Relaxed);
    }

    /// Write mirror lane `lane` of mark word `word_index` from old bits OLD.
    /// Called only by the two stop-all promotions.
    #[inline]
    fn store_age_lane(&self, word_index: usize, lane: usize, old: u64) {
        let bits = (old >> (lane * AGE_LANE_CELLS)) as u8;
        self.age_lane(word_index, lane)
            .store(SPREAD_AGES[bits as usize], Ordering::Relaxed);
    }

    /// Whether the age bytes agree with the old bitmap (GEN1 verify).
    pub(super) fn ages_match_old_bits(&self) -> bool {
        (0..CONS_MARK_WORDS).all(|word| {
            let old = self.old_word(word);
            (0..AGE_LANES_PER_WORD).all(|lane| {
                self.age_lane(word, lane).load(Ordering::Relaxed)
                    == SPREAD_AGES[(old >> (lane * AGE_LANE_CELLS)) as u8 as usize]
            })
        })
    }

    /// Stop-all handshake required: the marker must be joined and every
    /// mutation extent and allocation region closed before promotion.
    /// Return only newly-old cells, so repeated promotions never charge the
    /// old-generation accounting for a cell twice.
    ///
    /// One of the two writers of cons age (with `promote_major_world_stopped`):
    /// both run only at stop-all, so the bitmap and its age-byte mirror change
    /// together while no mutator can read either. Lanes without a newly-old
    /// cell keep their bytes.
    #[cold]
    #[inline(never)]
    pub(super) fn promote_marked_world_stopped(&self, cells: usize) -> usize {
        let mut promoted = 0usize;
        for w in 0..cons_mark_words(cells) {
            let marked = self.mark_word(w).load(Ordering::Relaxed) as u64;
            let previous_old = self.old_word(w);
            let newly_old = marked & !previous_old;
            if newly_old == 0 {
                continue;
            }
            promoted += newly_old.count_ones() as usize;
            let old = previous_old | marked;
            self.old_atomic(w).store(old, Ordering::Relaxed);
            #[cfg(test)]
            if mirror_fault::is(mirror_fault::MirrorFault::SkipPromotionLanes) {
                continue;
            }
            for lane in 0..AGE_LANES_PER_WORD {
                if (newly_old >> (lane * AGE_LANE_CELLS)) as u8 != 0 {
                    self.store_age_lane(w, lane, old);
                }
            }
        }
        promoted
    }

    /// Stop-all and joined-marker boundary: major liveness replaces age.
    /// Unused region tails have already been unmarked.
    ///
    /// The other writer of cons age, at stop-all like
    /// `promote_marked_world_stopped`. Every lane whose old bits change is
    /// rewritten, so a cell that died here reads young in the mirror before
    /// the sweep can hand its slot to a new cons.
    #[cold]
    #[inline(never)]
    pub(super) fn promote_major_world_stopped(&self) -> usize {
        let mut promoted = 0;
        for w in 0..CONS_MARK_WORDS {
            let marked = self.mark_word(w).load(Ordering::Relaxed) as u64;
            let previous_old = self.old_word(w);
            promoted += (marked & !previous_old).count_ones() as usize;
            if marked == previous_old {
                continue;
            }
            self.old_atomic(w).store(marked, Ordering::Relaxed);
            let changed = marked ^ previous_old;
            #[cfg(test)]
            let changed = if mirror_fault::is(mirror_fault::MirrorFault::SkipMajorClear) {
                changed & marked
            } else {
                changed
            };
            for lane in 0..AGE_LANES_PER_WORD {
                if (changed >> (lane * AGE_LANE_CELLS)) as u8 != 0 {
                    self.store_age_lane(w, lane, marked);
                }
            }
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
}

/// Test-only faults that make a promotion skip mirror writes, so GEN1
/// verify can be shown to catch a dropped age write.
#[cfg(test)]
pub(super) mod mirror_fault {
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[repr(u8)]
    pub(crate) enum MirrorFault {
        None = 0,
        /// Minor promotion updates the bitmap but not the age bytes.
        SkipPromotionLanes = 1,
        /// Major promotion leaves dead cells old in the age bytes.
        SkipMajorClear = 2,
    }

    // Promotion runs on the stopping mutator's thread, as in these tests.
    thread_local! {
        static FAULT: Cell<MirrorFault> = const { Cell::new(MirrorFault::None) };
    }

    pub(crate) fn set(fault: MirrorFault) {
        FAULT.with(|current| current.set(fault));
    }

    pub(super) fn is(fault: MirrorFault) -> bool {
        FAULT.with(Cell::get) == fault
    }
}

const _: () = {
    assert!(usize::BITS == u64::BITS);
    assert!(CONS_BLOCK_SIZE == 3796);
    assert!(CONS_CELLS_BYTES + size_of::<ConsBlockTrailer>() <= CONS_BLOCK_BYTES);
    assert!(CONS_OLD_OFFSET == CONS_CELLS_BYTES + std::mem::offset_of!(ConsBlockTrailer, old));
    assert!(CONS_AGE_OFFSET == CONS_CELLS_BYTES + std::mem::offset_of!(ConsBlockTrailer, age));
    // Word-wide access to the age bytes needs word alignment.
    assert!(CONS_AGE_OFFSET.is_multiple_of(align_of::<AtomicU64>()));
    assert!(CONS_BLOCK_SIZE <= CONS_AGE_BYTES);
};

#[cfg(test)]
#[path = "tests/cons_trailer_test.rs"]
mod cons_trailer_tests;
