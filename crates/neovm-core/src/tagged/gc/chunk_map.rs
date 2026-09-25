//! The chunk map: an O(1) page directory for every 64 KiB-aligned heap
//! granule (design P3.1 §3.7, the one page directory of P3.0 §3.5).
//!
//! Cons blocks and object-arena pages are 64 KiB, 64 KiB-aligned
//! allocations, so the granule `addr >> 16` names at most one of them. The
//! map is a two-level radix over the 47-bit user address space: a level-1
//! array of leaf pointers indexed by VA bits 46..32, and 256 KiB leaves of
//! [`ChunkEntry`] words indexed by bits 31..16. A lookup is two dependent
//! loads. Leaves are allocated on demand (zeroed, so faulted in lazily) and
//! freed only with the map; a process's heap pages typically occupy one or
//! two 4 GiB regions, so one or two leaves.
//!
//! **Entries.** A [`ChunkEntry`] packs the granule's [`ChunkClass`] (5
//! bits) and the index of its block or page in the owning collection (27
//! bits): `cons_blocks[i]`, or `pages[i]` of the class's arena.
//! `ChunkClass::None` means "no heap block or page": a boxed object,
//! the mapped image, a static, or not the heap at all.
//!
//! **Writers** (mutator only; `Release` stores): a new cons block or arena
//! page, and the release passes at the end of a sweep, which clear the
//! released granules and rewrite the moved indices. Blocks and pages are
//! only ever appended while a mark runs, and released only after the sweep,
//! which a new mark never overlaps, so during a mark every entry of a block
//! or page that existed at its start handshake is stable, and one created
//! since has an index at or above that class's count then — which is how
//! the GC thread tells snapshot pages from mid-cycle ones
//! ([`PageSnapshot::ChunkMap`]).
//!
//! **Readers.** The mutator's ownership oracles (`owns_*_object`,
//! `mark_cons_slow`, `is_value_marked`) and the GC thread's claim
//! classification (`Acquire` loads). The map is shared through an `Arc`;
//! the job holds a clone, and the heap outlives any job (its drop joins a
//! running mark first).
//!
//! Behind `NEOVM_GC_CHUNK_MAP=1` (`knobs.rs`); off, the heap has no map and
//! the per-class `FxHashMap` registries answer, as before. The registries
//! are maintained either way.

use super::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::sync::atomic::{AtomicPtr, AtomicU32};

/// What a 64 KiB granule holds.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
pub(crate) enum ChunkClass {
    /// No heap block or page.
    None = 0,
    Cons = 1,
    Float = 2,
    String = 3,
    Vector = 4,
    ByteCode = 5,
    Lambda = 6,
    Macro = 7,
    Record = 8,
    SymbolWithPos = 9,
    Marker = 10,
    Bignum = 11,
}

/// Number of [`ChunkClass`] values (the size of per-class tables).
pub(crate) const CHUNK_CLASS_COUNT: usize = ChunkClass::Bignum as usize + 1;

/// A granule's class and block/page index, packed in one word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChunkEntry(u32);

impl ChunkEntry {
    const CLASS_BITS: u32 = 5;
    const CLASS_MASK: u32 = (1 << Self::CLASS_BITS) - 1;
    /// The largest block or page index an entry can hold.
    pub(crate) const MAX_INDEX: usize = (u32::MAX >> Self::CLASS_BITS) as usize;
    /// No heap block or page.
    pub(crate) const NONE: Self = Self(0);

    pub(crate) fn new(class: ChunkClass, index: usize) -> Self {
        assert!(
            index <= Self::MAX_INDEX,
            "chunk map index {index} out of range"
        );
        Self(((index as u32) << Self::CLASS_BITS) | u32::from(u8::from(class)))
    }

    /// Whether this granule holds a block or page of `class`.
    #[inline(always)]
    pub(crate) fn is(self, class: ChunkClass) -> bool {
        self.0 & Self::CLASS_MASK == u32::from(class as u8)
    }

    /// The granule's class.
    #[inline(always)]
    pub(crate) fn class(self) -> ChunkClass {
        // Every stored entry was built from a `ChunkClass`.
        ChunkClass::try_from((self.0 & Self::CLASS_MASK) as u8).unwrap_or(ChunkClass::None)
    }

    /// The block's index in `cons_blocks`, or the page's in its arena.
    #[inline(always)]
    pub(crate) fn index(self) -> usize {
        (self.0 >> Self::CLASS_BITS) as usize
    }
}

const GRANULE_SHIFT: usize = 16;
const LEAF_BITS: usize = 16;
const LEAF_LEN: usize = 1 << LEAF_BITS;
const L1_SHIFT: usize = GRANULE_SHIFT + LEAF_BITS;
const L1_BITS: usize = 15;
const L1_LEN: usize = 1 << L1_BITS;

const _: () = assert!(1 << GRANULE_SHIFT == OBJECT_PAGE_ALIGN);
const _: () = assert!(1 << GRANULE_SHIFT == CONS_BLOCK_ALIGN);

/// One level-2 table: the entries of a 4 GiB region's granules.
#[repr(transparent)]
struct ChunkLeaf([AtomicU32; LEAF_LEN]);

/// The two-level page directory (see the module doc).
pub(crate) struct ChunkMap {
    /// `L1_LEN` leaf pointers, null until a granule of that region is set.
    l1: Box<[AtomicPtr<ChunkLeaf>]>,
}

// SAFETY: all shared state is atomics; leaves are owned by the map and freed
// only by its drop.
unsafe impl Send for ChunkMap {}
unsafe impl Sync for ChunkMap {}

impl ChunkMap {
    pub(crate) fn new() -> Self {
        let layout = Layout::array::<AtomicPtr<ChunkLeaf>>(L1_LEN).expect("chunk map level 1");
        // SAFETY: an all-zero `AtomicPtr` is a null pointer, so zeroed memory
        // is `L1_LEN` initialized null entries, owned by the box.
        let l1 = unsafe {
            let ptr = alloc::alloc_zeroed(layout) as *mut AtomicPtr<ChunkLeaf>;
            if ptr.is_null() {
                alloc::handle_alloc_error(layout);
            }
            Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, L1_LEN))
        };
        Self { l1 }
    }

    /// The entry of the granule holding `addr`.
    #[inline(always)]
    pub(crate) fn get(&self, addr: usize) -> ChunkEntry {
        let hi = addr >> L1_SHIFT;
        if hi >= L1_LEN {
            return ChunkEntry::NONE;
        }
        // SAFETY: `hi < L1_LEN`, the level-1 length.
        let leaf = unsafe { self.l1.get_unchecked(hi) }.load(Ordering::Acquire);
        if leaf.is_null() {
            return ChunkEntry::NONE;
        }
        // SAFETY: a published leaf lives as long as the map; the index is
        // masked to its length.
        let word = unsafe {
            (*leaf)
                .0
                .get_unchecked((addr >> GRANULE_SHIFT) & (LEAF_LEN - 1))
        };
        ChunkEntry(word.load(Ordering::Acquire))
    }

    /// Set the entry of the granule at `base` (64 KiB-aligned). Mutator
    /// only: there is exactly one writer.
    pub(crate) fn set(&self, base: usize, entry: ChunkEntry) {
        debug_assert_eq!(base & ((1 << GRANULE_SHIFT) - 1), 0, "unaligned granule");
        let hi = base >> L1_SHIFT;
        assert!(
            hi < L1_LEN,
            "heap granule {base:#x} is above the chunk map's 47-bit range"
        );
        let slot = &self.l1[hi];
        let mut leaf = slot.load(Ordering::Acquire);
        if leaf.is_null() {
            if entry == ChunkEntry::NONE {
                return;
            }
            let layout = Layout::new::<ChunkLeaf>();
            // SAFETY: an all-zero `AtomicU32` array is `LEAF_LEN` NONE
            // entries. Published with `Release` below, so a reader that
            // loads the pointer sees the zeroes.
            leaf = unsafe { alloc::alloc_zeroed(layout) as *mut ChunkLeaf };
            if leaf.is_null() {
                alloc::handle_alloc_error(layout);
            }
            slot.store(leaf, Ordering::Release);
        }
        // SAFETY: a published leaf; the index is masked to its length.
        unsafe { &(*leaf).0[(base >> GRANULE_SHIFT) & (LEAF_LEN - 1)] }
            .store(entry.0, Ordering::Release);
    }
}

impl Drop for ChunkMap {
    fn drop(&mut self) {
        for slot in self.l1.iter() {
            let leaf = slot.load(Ordering::Acquire);
            if !leaf.is_null() {
                // SAFETY: allocated in `set` with this layout, freed once.
                unsafe { alloc::dealloc(leaf as *mut u8, Layout::new::<ChunkLeaf>()) };
            }
        }
    }
}

/// The GC thread's ownership snapshot for one concurrent mark: which cons
/// blocks and which string, float, vector and byte-code pages existed at
/// the world-stopped start handshake. A value in one of them is an owned
/// object of that class the marker may mark or claim; anything else (a
/// block or page created since, the image, a boxed object) defers to the
/// termination.
pub(super) enum PageSnapshot {
    /// Per-class base-address sets captured at the handshake
    /// (`NEOVM_GC_CHUNK_MAP` off). O(blocks + pages) to build, one hash
    /// probe per test.
    BaseSets {
        cons: FxHashSet<usize>,
        string: FxHashSet<usize>,
        float: FxHashSet<usize>,
        vector: FxHashSet<usize>,
        bytecode: FxHashSet<usize>,
    },
    /// The live chunk map, plus each class's block or page count at the
    /// handshake: a granule is in the snapshot iff its class matches and its
    /// index is below that count (see the module doc for why).
    ChunkMap {
        map: std::sync::Arc<ChunkMap>,
        start_count: [usize; CHUNK_CLASS_COUNT],
    },
}

impl PageSnapshot {
    /// Is `addr` inside a snapshot block or page of `class`?
    #[inline(always)]
    pub(super) fn contains(&self, class: ChunkClass, addr: usize) -> bool {
        match self {
            PageSnapshot::BaseSets {
                cons,
                string,
                float,
                vector,
                bytecode,
            } => {
                let base = addr & !(OBJECT_PAGE_ALIGN - 1);
                match class {
                    ChunkClass::Cons => cons.contains(&base),
                    ChunkClass::String => string.contains(&base),
                    ChunkClass::Float => float.contains(&base),
                    ChunkClass::Vector => vector.contains(&base),
                    ChunkClass::ByteCode => bytecode.contains(&base),
                    _ => false,
                }
            }
            PageSnapshot::ChunkMap { map, start_count } => {
                let entry = map.get(addr);
                entry.is(class) && entry.index() < start_count[class as usize]
            }
        }
    }
}
