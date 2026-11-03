//! GEN1 card table (card-mark Stage A, tmp/v10x/reports/cardmark-stageA-2026-10-12.md).
//!
//! One masked byte per 512 bytes of owner address space. A store that may
//! create an old-to-young edge marks the card of its OWNER's start address;
//! the minor collection consults dirty cards. The index wraps at the mask, so
//! two owners 4 GiB apart share a byte: aliasing can only make a card look
//! dirty, never hide an edge. Clean is zero so the table starts as untouched,
//! lazily zero-filled pages.
//!
//! A second level of one byte per 64 KiB chunk (128 cards), right after the
//! cards in the same allocation, is marked with every card and names the
//! chunk's [`ChunkSlot`]. The minor scans the 64 KiB of summaries a word at
//! a time, then visits only the dirty slots and the cons blocks indexed
//! under them: its card work is a fixed table scan plus the dirty chunks,
//! never a walk of the heap's blocks, and the mutator's mark stays two
//! unconditional byte stores.
//!
//! Threading: mutators only ever store [`CARD_DIRTY`] into a byte (idempotent,
//! Relaxed, no read-modify-write); the collector reads and clears bytes only
//! with every mutator stopped.

use std::alloc::{self, Layout};
use std::mem::size_of;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU8, Ordering};

/// log2 of the card size: 512 bytes, 32 cons cells.
pub(crate) const CARD_SHIFT: u32 = 9;
pub(crate) const CARD_BYTES: usize = 1 << CARD_SHIFT;
/// Index bits: 2^23 one-byte cards, an aliasing period of 4 GiB.
pub(crate) const CARD_INDEX_BITS: u32 = 23;
pub(crate) const CARD_INDEX_MASK: usize = (1 << CARD_INDEX_BITS) - 1;
pub(crate) const CARD_TABLE_BYTES: usize = 1 << CARD_INDEX_BITS;
/// Cons blocks and arena pages are 64 KiB-aligned chunks of 128 cards.
pub(crate) const CHUNK_BYTES: usize = 64 * 1024;
pub(crate) const CHUNK_CARDS: usize = CHUNK_BYTES / CARD_BYTES;
const CHUNK_CARD_WORDS: usize = CHUNK_CARDS / size_of::<u64>();
/// log2 of the cards per chunk: a card index shifted down by this names its
/// chunk's summary byte.
pub(crate) const CARD_CHUNK_SHIFT: u32 = CHUNK_CARDS.trailing_zeros();
/// The chunk summary bytes start right after the cards.
pub(crate) const CHUNK_SUMMARY_OFFSET: usize = CARD_TABLE_BYTES;
const CHUNK_SUMMARY_BYTES: usize = CARD_TABLE_BYTES >> CARD_CHUNK_SHIFT;
const _: () = assert!(CHUNK_SUMMARY_BYTES == 1 << u16::BITS);
const _: () = assert!(CHUNK_SUMMARY_OFFSET % 64 == 0);
pub(crate) const CARD_CLEAN: u8 = 0;
pub(crate) const CARD_DIRTY: u8 = 1;

/// Byte offset, from a `*const TaggedHeap`, of the published card-table base
/// (one unaligned native-endian word in byte Cells after the exclusion words).
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) const HEAP_JIT_CARD_TABLE: usize =
    std::mem::offset_of!(super::TaggedHeap, card_table_base_0);

const _: () = {
    let offsets = [
        std::mem::offset_of!(super::TaggedHeap, card_table_base_0),
        std::mem::offset_of!(super::TaggedHeap, card_table_base_1),
        std::mem::offset_of!(super::TaggedHeap, card_table_base_2),
        std::mem::offset_of!(super::TaggedHeap, card_table_base_3),
        std::mem::offset_of!(super::TaggedHeap, card_table_base_4),
        std::mem::offset_of!(super::TaggedHeap, card_table_base_5),
        std::mem::offset_of!(super::TaggedHeap, card_table_base_6),
        std::mem::offset_of!(super::TaggedHeap, card_table_base_7),
    ];
    let mut byte = 0;
    while byte < offsets.len() {
        assert!(offsets[byte] == HEAP_JIT_CARD_TABLE + byte);
        byte += 1;
    }
    // The base follows the exclusion words, so no older offset moved.
    assert!(HEAP_JIT_CARD_TABLE == super::HEAP_JIT_EXCLUSION_LO + 40);
};

/// Mark OWNER_ADDR from a card-table base published outside the heap loan.
/// `base == 0` is a generational-off snapshot and does nothing. The two
/// stores match [`CardTable::mark`]: Relaxed, idempotent, no catalog lock.
#[inline(always)]
pub(super) fn mark_published(base: usize, owner_addr: usize) {
    if base == 0 {
        return;
    }
    let index = card_index(owner_addr);
    let summary = CHUNK_SUMMARY_OFFSET + (index >> CARD_CHUNK_SHIFT);
    // SAFETY: `base` is the card table allocated for this heap. Both indexes
    // are masked into that allocation, which outlives every admitted owner.
    unsafe {
        (*(base as *const AtomicU8).add(index)).store(CARD_DIRTY, Ordering::Relaxed);
        (*(base as *const AtomicU8).add(summary)).store(CARD_DIRTY, Ordering::Relaxed);
    }
}

/// The card of an owner address. Total: any address names exactly one card.
#[inline(always)]
pub(crate) const fn card_index(owner_addr: usize) -> usize {
    (owner_addr >> CARD_SHIFT) & CARD_INDEX_MASK
}

/// One chunk summary byte of the table. Chunks 4 GiB apart share a slot,
/// as their cards share bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ChunkSlot(u16);

impl ChunkSlot {
    /// The slot of the chunk holding OWNER_ADDR.
    #[inline(always)]
    pub(crate) const fn of(owner_addr: usize) -> Self {
        Self((card_index(owner_addr) >> CARD_CHUNK_SHIFT) as u16)
    }

    /// The index of the slot's first card.
    #[inline(always)]
    const fn first_card(self) -> usize {
        (self.0 as usize) << CARD_CHUNK_SHIFT
    }
}

/// Heap-owned zeroed byte table; its address is fixed for the heap's life.
pub(crate) struct CardTable {
    bytes: NonNull<AtomicU8>,
}

// SAFETY: the table is a fixed allocation of atomic bytes; every access goes
// through `AtomicU8`, and the allocation lives until the owning heap drops it.
unsafe impl Send for CardTable {}
unsafe impl Sync for CardTable {}

impl CardTable {
    fn layout() -> Layout {
        Layout::from_size_align(CARD_TABLE_BYTES + CHUNK_SUMMARY_BYTES, 64)
            .expect("card table layout")
    }

    pub(crate) fn new() -> Self {
        let layout = Self::layout();
        // SAFETY: non-zero size; zero is CARD_CLEAN for every byte.
        let raw = unsafe { alloc::alloc_zeroed(layout) };
        let bytes = NonNull::new(raw.cast::<AtomicU8>())
            .unwrap_or_else(|| alloc::handle_alloc_error(layout));
        Self { bytes }
    }

    /// The table's fixed address, which compiled cons stores index.
    pub(crate) fn base_addr(&self) -> usize {
        self.bytes.as_ptr() as usize
    }

    #[inline(always)]
    fn byte(&self, index: usize) -> &AtomicU8 {
        debug_assert!(index < CARD_TABLE_BYTES + CHUNK_SUMMARY_BYTES);
        // SAFETY: `index` is masked into the allocation, which outlives `self`.
        unsafe { &*self.bytes.as_ptr().add(index) }
    }

    #[inline(always)]
    fn summary(&self, slot: ChunkSlot) -> &AtomicU8 {
        self.byte(CHUNK_SUMMARY_OFFSET + slot.0 as usize)
    }

    /// Mark the card of OWNER_ADDR and its chunk's summary. Mutator side:
    /// two Relaxed byte stores.
    #[inline(always)]
    pub(crate) fn mark(&self, owner_addr: usize) {
        self.byte(card_index(owner_addr))
            .store(CARD_DIRTY, Ordering::Relaxed);
        self.summary(ChunkSlot::of(owner_addr))
            .store(CARD_DIRTY, Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn is_dirty(&self, owner_addr: usize) -> bool {
        self.byte(card_index(owner_addr)).load(Ordering::Relaxed) != CARD_CLEAN
    }

    /// The card words of SLOT. With every mutator stopped.
    #[inline]
    fn card_words(&self, slot: ChunkSlot) -> *mut u64 {
        // SAFETY: a slot's cards are CHUNK_CARDS consecutive in-table bytes
        // starting at a CHUNK_CARDS-aligned index, so the words are aligned
        // and in bounds.
        unsafe { self.bytes.as_ptr().add(slot.first_card()).cast::<u64>() }
    }

    /// The 128 card bytes of SLOT, or `None` when every one is clean: one
    /// chunk's cards are contiguous, read as sixteen words. Collector side,
    /// with every mutator stopped, so no store races the reads.
    #[inline]
    pub(crate) fn dirty_cards(&self, slot: ChunkSlot) -> Option<ChunkCards> {
        let words = self.card_words(slot);
        // SAFETY: see `card_words`.
        let word = |index: usize| unsafe { words.add(index).read() };
        if (0..CHUNK_CARD_WORDS).fold(0, |any, index| any | word(index)) == 0 {
            return None;
        }
        Some(ChunkCards(std::array::from_fn(word)))
    }

    /// Clean SLOT's summary and every one of its cards. Collector side,
    /// with every mutator stopped.
    #[inline]
    pub(crate) fn clear_slot(&self, slot: ChunkSlot) {
        self.summary(slot).store(CARD_CLEAN, Ordering::Relaxed);
        let words = self.card_words(slot);
        for index in 0..CHUNK_CARD_WORDS {
            // SAFETY: see `card_words`; no mutator runs.
            unsafe { words.add(index).write(0) };
        }
    }

    /// Append every slot whose summary is dirty to OUT, in slot order. The
    /// 64 KiB of summaries are read a cache line (eight words) at a time
    /// and clean lines skipped. With every mutator stopped.
    pub(crate) fn collect_dirty_slots(&self, out: &mut Vec<ChunkSlot>) {
        const LINE_WORDS: usize = 8;
        // SAFETY: the summaries are CHUNK_SUMMARY_BYTES in-table bytes at a
        // 64-byte-aligned offset of the 64-byte-aligned allocation, so the
        // words are aligned and in bounds; no mutator runs.
        let words = unsafe { self.bytes.as_ptr().add(CHUNK_SUMMARY_OFFSET).cast::<u64>() };
        let word = |index: usize| u64::from_le(unsafe { words.add(index).read() });
        for line in (0..CHUNK_SUMMARY_BYTES / size_of::<u64>()).step_by(LINE_WORDS) {
            if (line..line + LINE_WORDS).fold(0, |any, index| any | word(index)) == 0 {
                continue;
            }
            for index in line..line + LINE_WORDS {
                let mut bytes = word(index);
                while bytes != 0 {
                    let byte = bytes.trailing_zeros() as usize / 8;
                    bytes &= !(0xff << (byte * 8));
                    out.push(ChunkSlot((index * size_of::<u64>() + byte) as u16));
                }
            }
        }
    }

    /// Every slot whose summary is dirty, in slot order.
    #[cfg(test)]
    pub(crate) fn dirty_slots(&self) -> Vec<ChunkSlot> {
        let mut slots = Vec::new();
        self.collect_dirty_slots(&mut slots);
        slots
    }

    /// Clean one card, leaving its chunk summary dirty (tests: the collector
    /// cleans whole chunks).
    #[cfg(test)]
    pub(crate) fn clear(&self, owner_addr: usize) {
        self.byte(card_index(owner_addr))
            .store(CARD_CLEAN, Ordering::Relaxed);
    }
}

/// A copy of one chunk's card bytes with at least one dirty card.
#[derive(Clone, Copy)]
pub(crate) struct ChunkCards([u64; CHUNK_CARD_WORDS]);

impl ChunkCards {
    /// The byte offset from the chunk base of every dirty card, in address
    /// order; clean words are skipped eight cards at a time.
    #[inline]
    pub(crate) fn dirty_offsets(self) -> impl Iterator<Item = usize> {
        self.0
            .into_iter()
            .enumerate()
            .flat_map(|(word_index, word)| {
                // Byte k of the word is card 8 * word_index + k in memory order.
                let mut bytes = u64::from_le(word);
                std::iter::from_fn(move || {
                    if bytes == 0 {
                        return None;
                    }
                    let byte = bytes.trailing_zeros() as usize / 8;
                    bytes &= !(0xff << (byte * 8));
                    Some((word_index * size_of::<u64>() + byte) << CARD_SHIFT)
                })
            })
    }
}

/// The collector's index of chunk slots: which cons blocks share each one,
/// and the slots found dirty by the last scan.
#[derive(Default)]
pub(crate) struct CardSlots {
    blocks: FxHashMap<ChunkSlot, SmallVec<[usize; 1]>>,
    /// Sorted and duplicate-free.
    dirty: Vec<ChunkSlot>,
}

impl CardSlots {
    /// Cold catalog shell validation; no installed blocks or dirty work.
    #[cold]
    pub(in crate::tagged::gc) fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.dirty.is_empty()
    }

    /// Register the cons block at BASE under its slot.
    pub(crate) fn insert_block(&mut self, base: usize) {
        self.blocks
            .entry(ChunkSlot::of(base))
            .or_default()
            .push(base);
    }

    /// Re-register exactly BASES, after blocks were released.
    pub(crate) fn rebuild(&mut self, bases: impl IntoIterator<Item = usize>) {
        self.blocks.clear();
        for base in bases {
            self.insert_block(base);
        }
    }

    /// The bases of the cons blocks whose chunk uses SLOT.
    pub(crate) fn blocks(&self, slot: ChunkSlot) -> &[usize] {
        self.blocks.get(&slot).map_or(&[], |bases| bases.as_slice())
    }

    /// Replace `dirty` with the slots CARDS reports dirty now. With every
    /// mutator stopped.
    pub(crate) fn scan(&mut self, cards: &CardTable) {
        self.dirty.clear();
        cards.collect_dirty_slots(&mut self.dirty);
    }

    pub(crate) fn dirty(&self) -> &[ChunkSlot] {
        &self.dirty
    }

    /// Hand the dirty slots to the caller, which cleans them.
    pub(crate) fn take_dirty(&mut self) -> Vec<ChunkSlot> {
        std::mem::take(&mut self.dirty)
    }
}

impl Drop for CardTable {
    fn drop(&mut self) {
        // SAFETY: allocated in `new` with the same layout.
        unsafe { alloc::dealloc(self.bytes.as_ptr().cast(), Self::layout()) };
    }
}
