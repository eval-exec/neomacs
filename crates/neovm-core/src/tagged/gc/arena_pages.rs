//! Size-class object arena pages: 64 KB-aligned pages of fixed-stride slots for floats, strings, vectors, bytecode, lambdas, macros, records and symbols-with-position, with per-page allocation bitmaps, free lists, and the PagedObject trait.
//!
//! Moved out of `gc.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

//
// Floats (v1), strings and vectors (stage 3), and bytecode (task 03/3a) live
// in size-class arena PAGES:
// fixed 64KB-aligned pages of per-class fixed-stride slots replace the
// per-object `Box` + intrusive-list storage. Page objects keep their
// `GcHeader` (parity/tenured semantics untouched) but are NEVER in
// `non_cons_object_addrs` and NEVER linked onto `all_objects`/
// `tenured_objects`: the intrusive lists sweep with `free_gc_object`, whose
// `Box::from_raw` would corrupt the heap on a page pointer. The OWNERSHIP
// ORACLE for a page object is the page-span test (`ObjectArena::owns`):
// page-base registry hit + stride alignment + ALLOC-BIT-SET. The page sweep
// (`ObjectArena::sweep_range`) is their only reclaimer, wired into both sweep
// entry points (eager `finalize_collection` and the cooperative
// `incremental_sweep_slice`).
//
// GENERATIONAL NOTE — page objects tenure via the promotion PAGE WALK
// (`promote_and_blacken` flips `header.tenured` on every allocated slot at
// the one-time first partition cycle; the intrusive-list splice never sees
// them). The per-object `header.tenured` remains the SOLE mark-path
// authority; no page-level flag is consulted by any mark path. Pages that
// are FULL of tenured slots at promotion are RETIRED: never swept, never
// allocated into, freed only at heap teardown — but they STAY in the
// ownership oracle, because `value_is_tenured` (and through it the
// remembered-set write barrier) gates on ownership: a retired-page tenured
// object that answered "not owned" would miss its first post-retirement
// tenured→young edge and its child would be swept while live (UAF). Pages
// left with a mix of tenured + free/young slots stay in rotation as MIXED
// pages: every later sweep re-skips their tenured slots (a bounded cost —
// the one-time loadup survivor set is the only tenured population).

pub(super) const OBJECT_PAGE_BYTES: usize = 64 * 1024;
/// Pages are ALIGNED to their size so any slot pointer derives its page base
/// with `addr & !(OBJECT_PAGE_BYTES - 1)` (the cons-block trick). The explicit
/// `Layout` alignment in `ObjectPage::layout` is what makes that mask valid.
pub(super) const OBJECT_PAGE_ALIGN: usize = OBJECT_PAGE_BYTES;
/// Bitmap capacity for the smallest stride (32B floats → 2048 slots → 32
/// words). Classes with larger strides use a prefix of the array; the unused
/// tail words stay zero forever.
pub(super) const OBJECT_PAGE_MAX_ALLOC_WORDS: usize =
    (OBJECT_PAGE_BYTES / 32).div_ceil(usize::BITS as usize);
/// Sentinel: "no slot" (free-list terminator) / "no page" (partial-chain
/// terminator and empty-chain head).
pub(super) const PAGE_NONE: usize = usize::MAX;

/// Per-class parameters for the size-class arena pages. Implemented by the
/// paged object types (`FloatObj`, `StringObj`, `VectorObj`).
///
/// CONTRACT for implementors: `Self` is `#[repr(C)]` with a `GcHeader` at
/// offset 0, and fits its slot with room for the trailing free-list link
/// word (`size_of::<Self>() + 8 <= SLOT_BYTES`) — const-checked in
/// `ObjectPage::<Self>::LAYOUT_OK`, evaluated at every page creation.
pub(super) trait PagedObject: Sized {
    /// Slot stride in bytes; a page holds `OBJECT_PAGE_BYTES / SLOT_BYTES`
    /// slots. The trailing 8 bytes of a slot hold the page-local free-list
    /// link while the slot is FREE, so the link never aliases object bytes —
    /// an adversarially scribbled dead header cannot corrupt the free list,
    /// and pushing a slot to the free list does not touch its (stale) header.
    const SLOT_BYTES: usize;
    /// The `GcHeader.kind` every allocated slot of this class must carry
    /// (debug-asserted by the sweep and verifiers).
    const KIND: HeapObjectKind;
    /// Class name for diagnostics.
    const CLASS: &'static str;
    /// TEST-ONLY live page counter (teardown-leak / double-free probe for the
    /// Drop tests): `ObjectPage::new` increments, `ObjectPage::drop` decrements.
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize;
}

// Slot strides. Float: `FloatObj` is exactly 24 bytes (GcHeader 16 + f64 8) →
// 32B slots (24..32 = free link). String: `StringObj` is 56 bytes (GcHeader
// 16 + LispString 40) → 64B slots with the link in bytes 56..64 — ZERO slack,
// const-proven below. Vector: `VectorObj` is 48 bytes (VecLikeHeader 24 +
// LispValueVec 24 — the 24 relies on the Owned(Vec)/Mapped niche packing; the
// const assert below is the compile-time proof) → shares the 64B class, link
// in bytes 56..64.
pub(super) const _: () = assert!(size_of::<FloatObj>() == 24, "FloatObj must stay 24 bytes");
pub(super) const _: () = assert!(
    size_of::<StringObj>() <= 56,
    "StringObj must fit a 64-byte slot with its trailing free-list link \
     (bytes 56..64 — zero slack)",
);
pub(super) const _: () = assert!(
    size_of::<VectorObj>() <= 48,
    "VectorObj must stay <= 48 bytes (VecLikeHeader 24 + niche-packed \
     LispValueVec 24); if this fails the niche packing broke — give Vector \
     its own larger stride instead of silently overlapping the link word",
);
// ByteCode (task 03/3a): `ByteCodeObj` is VecLikeHeader 24 + ByteCodeFunction
// (~336B of vecs/options/params + the jit Runtime) → 384B slots with the
// free-list link in bytes 376..384. 384 is NOT a power of two: a page holds
// floor(64KB / 384) = 170 slots and the trailing 256 bytes are a permanently
// unused tail (never bump-reached, no alloc bit — `ObjectArena::owns` bounds
// the slot index explicitly so a stride-aligned tail address answers
// NOT-owned). If this assert fails the ByteCodeFunction grew — BUMP THE
// STRIDE (and say so in the commit); never squeeze the link into live bytes.
pub(super) const _: () = assert!(
    size_of::<ByteCodeObj>() <= 376,
    "ByteCodeObj must fit a 384-byte slot with its trailing free-list link \
     (bytes 376..384); bump the bytecode stride if the struct grew",
);
// Lambda/Macro (task 03/3b): each is VecLikeHeader 24 + LispValueVec 24 +
// OnceLock<LambdaParams> (~64) = 112B → a 128B power-of-two class (512
// slots/page, link in bytes 120..128). LambdaObj and MacroObj are
// byte-identical in layout (same fields) but DISTINCT Rust types, so each
// gets its OWN class arena at the shared 128B stride — exactly as
// string/vector share the 64B stride in separate arenas (per-class
// registries never merge; a page hit is never a cross-class collision). If
// either assert fails the struct grew — BUMP THE STRIDE; never squeeze the
// link into live bytes.
pub(super) const _: () = assert!(
    size_of::<LambdaObj>() <= 120,
    "LambdaObj must fit a 128-byte slot with its trailing free-list link \
     (bytes 120..128); bump the lambda/macro stride if the struct grew",
);
pub(super) const _: () = assert!(
    size_of::<MacroObj>() <= 120,
    "MacroObj must fit a 128-byte slot with its trailing free-list link \
     (bytes 120..128); bump the lambda/macro stride if the struct grew",
);
// Record (task 03/3b): `RecordObj` is VecLikeHeader 24 + LispValueVec 24 =
// 48B → the 64B class (1024 slots/page, link in bytes 56..64), shared with
// string/vector in its OWN arena. `RecordObj` backs BOTH the `Record` and
// `WindowConfiguration` type tags (`alloc_record` / `alloc_window_configuration`
// — same Rust type, distinct tag), so both funnel to `record_arena`. If this
// assert fails the struct grew — bump the stride; never squeeze the link.
pub(super) const _: () = assert!(
    size_of::<RecordObj>() <= 56,
    "RecordObj must fit a 64-byte slot with its trailing free-list link \
     (bytes 56..64); bump the record stride if the struct grew",
);
// SymbolWithPos (task 03/3b): `SymbolWithPosObj` is VecLikeHeader 24 + two
// fixed TaggedValue fields (sym, pos) = 40B → the 64B class (1024 slots/page,
// link in bytes 56..64), OWN arena. Both fields are `Copy` immediates
// (TaggedValue), so the struct is POD-like (`needs_drop` == false, like
// FloatObj): the generic sweep/teardown `drop_in_place` walk compiles out —
// no payload to free. 64B (vs a tighter 48B) keeps the class power-of-two /
// page-dividing (no page tail) and leaves comfortable headroom for a
// low-volume type; if this assert fails the struct grew — bump the stride.
pub(super) const _: () = assert!(
    size_of::<SymbolWithPosObj>() <= 56,
    "SymbolWithPosObj must fit a 64-byte slot with its trailing free-list \
     link (bytes 56..64); bump the symbol-with-pos stride if the struct grew",
);

#[cfg(test)]
pub(crate) static LIVE_FLOAT_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_STRING_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_VECTOR_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_BYTECODE_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_LAMBDA_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_MACRO_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_RECORD_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_SYMBOL_WITH_POS_PAGES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static LIVE_MARKER_PAGES: AtomicUsize = AtomicUsize::new(0);
// Marker: `MarkerObj` is VecLikeHeader 24 + LispMarker (buffer, marker_id,
// bytepos, charpos, chain link, two flags) ≈ 88B → the 128B class (512
// slots/page, link in bytes 120..128). Markers are the highest-churn editor
// object (`save-excursion` makes and frees one per call — thousands per
// org-mode operation); GNU keeps them in `marker_block`s for the same
// reason. POD: `LispMarker` holds no Values, so the sweep's drop_in_place
// compiles out. If this assert fails the struct grew — bump the stride.
const _: () = assert!(
    size_of::<MarkerObj>() <= 120,
    "MarkerObj must fit a 128-byte slot with its trailing free-list link \
     (bytes 120..128); bump the marker stride if the struct grew",
);

impl PagedObject for MarkerObj {
    // 128B class, own arena: POD-like (no Values; the intrusive buffer-chain
    // link is a raw pointer that `unchain_dead_markers` detaches before any
    // sweep frees the slot).
    const SLOT_BYTES: usize = 128;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "marker";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_MARKER_PAGES
    }
}
impl PagedObject for FloatObj {
    const SLOT_BYTES: usize = 32;
    const KIND: HeapObjectKind = HeapObjectKind::Float;
    const CLASS: &'static str = "float";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_FLOAT_PAGES
    }
}

impl PagedObject for StringObj {
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::String;
    const CLASS: &'static str = "string";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_STRING_PAGES
    }
}

impl PagedObject for VectorObj {
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "vector";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_VECTOR_PAGES
    }
}

impl PagedObject for ByteCodeObj {
    // First non-power-of-two stride: 170 slots/page + a 256B unused tail
    // (see the ByteCodeObj const assert above).
    const SLOT_BYTES: usize = 384;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "bytecode";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_BYTECODE_PAGES
    }
}

impl PagedObject for LambdaObj {
    // Shared 128B lambda/macro class: 512 slots/page, link in bytes 120..128.
    const SLOT_BYTES: usize = 128;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "lambda";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_LAMBDA_PAGES
    }
}

impl PagedObject for MacroObj {
    // Shares the 128B lambda class stride in its OWN arena (see LambdaObj).
    const SLOT_BYTES: usize = 128;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "macro";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_MACRO_PAGES
    }
}

impl PagedObject for RecordObj {
    // 64B class (shared stride, own arena): 1024 slots/page, link 56..64.
    // Backs both the Record and WindowConfiguration type tags.
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "record";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_RECORD_PAGES
    }
}

impl PagedObject for SymbolWithPosObj {
    // 64B class, own arena: POD-like (no payload, `needs_drop` == false).
    const SLOT_BYTES: usize = 64;
    const KIND: HeapObjectKind = HeapObjectKind::VecLike;
    const CLASS: &'static str = "symbol-with-pos";
    #[cfg(test)]
    fn live_page_counter() -> &'static AtomicUsize {
        &LIVE_SYMBOL_WITH_POS_PAGES
    }
}

/// Slot count of a float page (test scenarios size their populations off it).
#[cfg(test)]
pub(super) const FLOAT_PAGE_SLOTS: usize =
    OBJECT_PAGE_BYTES / <FloatObj as PagedObject>::SLOT_BYTES;
/// Slot count of a bytecode page (170: the 384B stride does not divide 64KB;
/// the 256B page tail is never allocated).
#[cfg(test)]
pub(super) const BYTECODE_PAGE_SLOTS: usize =
    OBJECT_PAGE_BYTES / <ByteCodeObj as PagedObject>::SLOT_BYTES;
/// Slot count of a lambda/macro page (512: the 128B stride divides 64KB
/// exactly, no tail). LambdaObj and MacroObj share the stride so this const
/// applies to both arenas.
#[cfg(test)]
pub(super) const LAMBDA_PAGE_SLOTS: usize =
    OBJECT_PAGE_BYTES / <LambdaObj as PagedObject>::SLOT_BYTES;
/// Slot count of a record page (1024: 64B stride divides 64KB exactly).
#[cfg(test)]
pub(super) const RECORD_PAGE_SLOTS: usize =
    OBJECT_PAGE_BYTES / <RecordObj as PagedObject>::SLOT_BYTES;
/// Slot count of a symbol-with-pos page (1024: 64B stride).
#[cfg(test)]
pub(super) const SYMBOL_WITH_POS_PAGE_SLOTS: usize =
    OBJECT_PAGE_BYTES / <SymbolWithPosObj as PagedObject>::SLOT_BYTES;
/// Slot count of a marker page (512: 128B stride).
#[cfg(test)]
pub(super) const MARKER_PAGE_SLOTS: usize =
    OBJECT_PAGE_BYTES / <MarkerObj as PagedObject>::SLOT_BYTES;

/// One 64KB-aligned arena page of fixed-stride `T` slots.
///
/// The ALLOCATION BITMAP (`alloc_bits`) is the sole authority on which slots
/// hold live objects: a clear bit means the slot bytes are GARBAGE (a
/// never-bumped slot is uninitialized; a freed slot's header is stale and may
/// have been scribbled by reuse machinery), so every reader — sweep, verifier,
/// teardown, and the page-span ownership oracle — must test the bit BEFORE
/// any header access (ALLOCATED-BIT-FIRST; the INVERSE of the intrusive-list
/// sweep, whose list membership itself implies a valid header). The alloc-bit
/// test in `ObjectArena::owns` is also what makes the page-span oracle exact:
/// a freed-but-unswept... rather, a freed slot answers NOT-owned the instant
/// its bit clears, replacing float-v1's explicit addr-set evict-before-free.
pub(super) struct ObjectPage<T: PagedObject> {
    /// 64KB of raw slot storage, aligned to 64KB (`OBJECT_PAGE_ALIGN`).
    pub(super) storage: *mut u8,
    /// Bump cursor: index of the first never-allocated slot.
    pub(super) next_index: usize,
    /// Per-slot allocation bitmap (bit set ⇔ slot holds a live `T`). Sized
    /// for the smallest stride; this class uses the first `ALLOC_WORDS`.
    pub(super) alloc_bits: [usize; OBJECT_PAGE_MAX_ALLOC_WORDS],
    /// Occupancy: number of set bits in `alloc_bits`.
    pub(super) allocated: usize,
    /// Page-local free list: head slot index, linked through each free slot's
    /// trailing link word (`FREE_LINK_OFFSET`). `PAGE_NONE` = empty.
    pub(super) free_head: usize,
    /// Class free list ("pages with free slots") chain: index of the next
    /// such page in the arena, `PAGE_NONE` at the tail.
    pub(super) next_partial: usize,
    /// Whether this page is currently linked on the arena's partial chain.
    pub(super) on_partial: bool,
    /// RETIRED (promotion, stage-3 commit 4): the page was full of tenured
    /// slots at the one-time promotion. Never swept, never allocated into
    /// (it has no free slots and never gains any), freed at heap teardown —
    /// but it STAYS in the page-base registry so the ownership oracle keeps
    /// answering "owned" for its slots (see the C1 note in the module doc).
    pub(super) retired: bool,
    pub(super) _class: std::marker::PhantomData<*mut T>,
}

impl<T: PagedObject> ObjectPage<T> {
    /// Slots per page for this class.
    pub(super) const SLOTS: usize = OBJECT_PAGE_BYTES / T::SLOT_BYTES;
    /// Bitmap words this class actually uses (prefix of `alloc_bits`).
    pub(super) const ALLOC_WORDS: usize = Self::SLOTS.div_ceil(usize::BITS as usize);
    /// Offset of the free-list link word inside a FREE slot (past the object).
    pub(super) const FREE_LINK_OFFSET: usize = T::SLOT_BYTES - size_of::<usize>();
    /// Layout proofs the slot scheme rests on, per class. Referenced in
    /// `new()` so the asserts are evaluated at compile time for every
    /// instantiated class.
    pub(super) const LAYOUT_OK: () = {
        // The stride need NOT be a power of two or divide the page exactly
        // (bytecode's 384B stride is the first such class): `SLOTS` floors
        // the division and the sub-stride page tail is simply never
        // bump-reached. Everything stride-derived (`slot_ptr` multiply,
        // `owns`'s modulo + explicit `< SLOTS` bound, the bitmap prefix) is
        // exact for any stride that satisfies the asserts below.
        assert!(Self::SLOTS >= 1);
        assert!(Self::SLOTS * T::SLOT_BYTES <= OBJECT_PAGE_BYTES);
        assert!(Self::ALLOC_WORDS <= OBJECT_PAGE_MAX_ALLOC_WORDS);
        // The trailing link word never aliases object bytes.
        assert!(Self::FREE_LINK_OFFSET >= size_of::<T>());
        assert!(Self::FREE_LINK_OFFSET + size_of::<usize>() <= T::SLOT_BYTES);
        assert!(T::SLOT_BYTES.is_multiple_of(std::mem::align_of::<T>()));
        assert!(Self::FREE_LINK_OFFSET.is_multiple_of(std::mem::align_of::<usize>()));
    };

    pub(super) fn layout() -> Layout {
        Layout::from_size_align(OBJECT_PAGE_BYTES, OBJECT_PAGE_ALIGN).expect("object page layout")
    }

    pub(super) fn new() -> Self {
        // Force the per-class layout proofs (compile-time).
        #[allow(clippy::let_unit_value)]
        let () = Self::LAYOUT_OK;
        let storage = unsafe { alloc::alloc(Self::layout()) };
        if storage.is_null() {
            alloc::handle_alloc_error(Self::layout());
        }
        #[cfg(test)]
        T::live_page_counter().fetch_add(1, Ordering::Relaxed);
        Self {
            storage,
            next_index: 0,
            alloc_bits: [0; OBJECT_PAGE_MAX_ALLOC_WORDS],
            allocated: 0,
            free_head: PAGE_NONE,
            next_partial: PAGE_NONE,
            on_partial: false,
            retired: false,
            _class: std::marker::PhantomData,
        }
    }

    #[inline]
    pub(super) fn base_addr(&self) -> usize {
        self.storage as usize
    }

    #[inline]
    pub(super) fn slot_ptr(&self, index: usize) -> *mut T {
        debug_assert!(index < Self::SLOTS);
        unsafe { self.storage.add(index * T::SLOT_BYTES).cast() }
    }

    /// Page base for any pointer into a page — valid ONLY because pages are
    /// size-aligned (see `OBJECT_PAGE_ALIGN`).
    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn page_base_for_ptr(ptr: *const T) -> usize {
        (ptr as usize) & !(OBJECT_PAGE_ALIGN - 1)
    }

    #[inline]
    pub(super) fn is_allocated(&self, index: usize) -> bool {
        let word = index / usize::BITS as usize;
        let mask = 1usize << (index % usize::BITS as usize);
        (self.alloc_bits[word] & mask) != 0
    }

    /// Set slot `index`'s alloc bit (it must be clear) and bump occupancy.
    #[inline]
    pub(super) fn set_allocated(&mut self, index: usize) {
        debug_assert!(!self.is_allocated(index), "arena slot double-allocated");
        self.alloc_bits[index / usize::BITS as usize] |= 1usize << (index % usize::BITS as usize);
        self.allocated += 1;
    }

    /// The free-list link word of slot `index` (meaningful only while free).
    #[inline]
    pub(super) fn free_link_ptr(&self, index: usize) -> *mut usize {
        debug_assert!(index < Self::SLOTS);
        unsafe {
            self.storage
                .add(index * T::SLOT_BYTES + Self::FREE_LINK_OFFSET)
                .cast()
        }
    }

    /// Pop one slot off the page-local free list. The caller must set the
    /// alloc bit and FULL-HEADER-WRITE the slot before publishing it.
    #[inline]
    pub(super) fn pop_free(&mut self) -> Option<usize> {
        if self.free_head == PAGE_NONE {
            return None;
        }
        let index = self.free_head;
        debug_assert!(
            !self.is_allocated(index),
            "free-listed arena slot has its alloc bit set",
        );
        self.free_head = unsafe { self.free_link_ptr(index).read() };
        Some(index)
    }

    /// FREE one slot: clear its alloc bit and thread it onto the page-local
    /// free list. For payload-bearing classes (strings own byte storage +
    /// interval tables; vectors own their element `Vec`) the caller MUST
    /// `drop_in_place` the slot BEFORE calling this — a bit-clear-only free
    /// leaks every payload, and once the bit is clear the slot bytes are
    /// garbage that no reader (this fn included) may interpret. Only the
    /// trailing link word is written here — the stale object bytes are left
    /// in place and must never be read again (allocated-bit-first).
    #[inline]
    pub(super) fn free_slot(&mut self, index: usize) {
        debug_assert!(self.is_allocated(index), "arena slot double-freed");
        debug_assert!(!self.retired, "freed a slot in a retired page");
        self.alloc_bits[index / usize::BITS as usize] &=
            !(1usize << (index % usize::BITS as usize));
        self.allocated -= 1;
        unsafe { self.free_link_ptr(index).write(self.free_head) };
        self.free_head = index;
    }

    /// Bump-allocate the next never-used slot index, if any. The caller must
    /// set the alloc bit and FULL-HEADER-WRITE the slot.
    #[inline]
    pub(super) fn bump(&mut self) -> Option<usize> {
        if self.next_index >= Self::SLOTS {
            return None;
        }
        let index = self.next_index;
        self.next_index += 1;
        Some(index)
    }
}

impl<T: PagedObject> Drop for ObjectPage<T> {
    fn drop(&mut self) {
        // TEARDOWN OWNS THE PAYLOADS: walk the allocated slots (bit-first —
        // clear-bit slot bytes are garbage) and `drop_in_place` each live
        // object, so strings free their byte storage + interval tables and
        // vectors free their element `Vec`. Float-v1's dealloc-only Drop does
        // NOT generalize; `needs_drop` keeps the float walk compiled out.
        // Reached either when a completed sweep removes an empty young page,
        // or when the owning `TaggedHeap` drops. Both paths run on the mutator
        // after any concurrent marker has joined, so no GC thread can still be
        // reading these slots. Retired pages are freed only at heap teardown.
        if std::mem::needs_drop::<T>() {
            for word_index in 0..Self::ALLOC_WORDS {
                let mut bits = self.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    unsafe { std::ptr::drop_in_place(self.slot_ptr(index)) };
                }
            }
        }
        #[cfg(test)]
        T::live_page_counter().fetch_sub(1, Ordering::Relaxed);
        unsafe { alloc::dealloc(self.storage, Self::layout()) };
    }
}

/// One size class of the object arena: its pages, the page-base registry
/// (page base → `pages` index; mirrors `cons_block_index_by_base` but is a
/// DISTINCT per-class registry — the mark paths dispatch tag-first to the
/// class registry, and the collision analysis for `owns` depends on each
/// registry holding only its own class's pages — never merge them, with each
/// other or with the cons registry), and the class free list.
pub(super) struct ObjectArena<T: PagedObject> {
    /// Every retained page of this class, retired pages included. Completely
    /// empty young pages are removed after a full sweep; all remaining pages
    /// are freed by this vector's drop at heap teardown (`ObjectPage: Drop`).
    pub(super) pages: Vec<ObjectPage<T>>,
    /// Page-base → `pages` index: O(1) page lookup from any slot pointer
    /// (pages are size-aligned, so `ObjectPage::page_base_for_ptr` masks the
    /// base out of the pointer). Retired pages STAY registered (C1).
    pub(super) page_index_by_base: FxHashMap<usize, usize>,
    /// Class free list: index of the first page with free slots
    /// (`PAGE_NONE` = none), chained through `ObjectPage::next_partial`.
    /// Alloc order: partial-page free-slot pop → last-page bump → new page.
    pub(super) partial_head: usize,
}

impl<T: PagedObject> ObjectArena<T> {
    pub(super) fn new() -> Self {
        Self {
            pages: Vec::new(),
            page_index_by_base: FxHashMap::default(),
            partial_head: PAGE_NONE,
        }
    }

    /// THE PAGE-SPAN OWNERSHIP ORACLE for this class: `ptr` is an owned, LIVE
    /// object of class `T` iff its masked page base is a registered page of
    /// this arena AND the offset is slot-aligned AND the slot's ALLOC BIT IS
    /// SET. The alloc-bit test is load-bearing: bump-cursor/registry bounds
    /// alone would answer "owned" for a freed slot, and every owner of an
    /// owns()→header-read sequence (`is_heap_young`, `value_is_tenured`,
    /// `is_value_marked`, `mark_value`'s owned arms) would then read garbage
    /// bytes. A freed slot answers NOT-owned the instant its bit clears —
    /// this replaces float-v1's explicit addr-set evict-before-free. Retired
    /// pages answer normally (their bits are all set, permanently) — C1.
    ///
    /// Mutator-thread only (like every `&self` heap read); the GC thread's
    /// ownership test is the start-handshake page-base SNAPSHOT, never this
    /// live registry/bitmap.
    #[inline]
    pub(super) fn owns(&self, ptr: *const u8) -> bool {
        let addr = ptr as usize;
        let base = addr & !(OBJECT_PAGE_ALIGN - 1);
        let Some(&index) = self.page_index_by_base.get(&base) else {
            return false;
        };
        let offset = addr - base;
        if !offset.is_multiple_of(T::SLOT_BYTES) {
            return false;
        }
        let slot = offset / T::SLOT_BYTES;
        // Non-power-of-two strides (bytecode's 384B) leave a sub-stride page
        // TAIL whose first byte is stride-aligned; bound the index so a tail
        // address answers NOT-owned by construction (its bit also can never
        // be set — bump/free never mint indices >= SLOTS — but the oracle
        // must not lean on "never set" for exactness).
        slot < ObjectPage::<T>::SLOTS && self.pages[index].is_allocated(slot)
    }

    /// Grab one raw slot: class free-list pop → current-page bump → new page.
    /// Sets the slot's alloc bit; the caller MUST immediately
    /// full-header-write the slot (its bytes are garbage until then, and the
    /// sweep may legally visit it as soon as the mutator next yields —
    /// allocated-bit ⇒ readable header is the sweep's contract).
    pub(super) fn alloc_slot(&mut self) -> *mut T {
        // 1. Class free list: pop from the first page with freed slots.
        //    Retired pages are never on this chain (they never gain free
        //    slots: full at retirement and never swept).
        if self.partial_head != PAGE_NONE {
            let page_index = self.partial_head;
            let page = &mut self.pages[page_index];
            let index = page.pop_free().expect("partial page must have free slots");
            page.set_allocated(index);
            if page.free_head == PAGE_NONE {
                // Drained: unlink from the partial chain (head pop — O(1)).
                self.partial_head = page.next_partial;
                page.next_partial = PAGE_NONE;
                page.on_partial = false;
            }
            return page.slot_ptr(index);
        }
        // 2. Current-page bump: only the NEWEST page can have never-used
        //    slots (older pages were bump-exhausted before it was created).
        //    A retired last page is bump-exhausted by construction (retired
        //    ⇒ full), so `bump` correctly falls through to a fresh page.
        if let Some(page) = self.pages.last_mut()
            && let Some(index) = page.bump()
        {
            page.set_allocated(index);
            return page.slot_ptr(index);
        }
        // 3. Fresh 64KB-aligned page.
        let mut page = ObjectPage::<T>::new();
        let index = page.bump().expect("fresh arena page must have space");
        page.set_allocated(index);
        let ptr = page.slot_ptr(index);
        let base = page.base_addr();
        self.pages.push(page);
        let prev = self.page_index_by_base.insert(base, self.pages.len() - 1);
        debug_assert!(prev.is_none(), "arena page base registered twice");
        ptr
    }

    /// Sweep pages `[start, end)` of this class — the page objects' only
    /// reclaimer, wired into BOTH sweep entry points: the eager
    /// `finalize_collection` (whole range in one call) and the cooperative
    /// `incremental_sweep_slice` (page-at-a-time behind a per-class cursor).
    /// Runs on the mutator thread only; the GC thread never sweeps.
    ///
    /// Visit order is ALLOCATED-BIT-FIRST: a clear bit means the slot bytes
    /// are garbage (never-bumped = uninitialized; freed = stale, possibly
    /// scribbled), so ANY header read through it is UB — the inverse of the
    /// intrusive-list sweep, whose list membership implies a valid header.
    /// RETIRED pages are skipped whole (never swept; their slots are all
    /// tenured — permanently live). For allocated slots the order is:
    ///
    /// 1. TENURED-SKIP BEFORE THE PARITY TEST: a tenured slot's mark bit
    ///    froze at promotion; interpreting it against the current parity
    ///    would free a live tenured object on every alternate-parity cycle
    ///    (the float-v1 template's bare `is_marked_at` is exactly that bug
    ///    once page objects can tenure). Tenured slots are skipped (MIXED
    ///    pages carry them forever — bounded by the one-time loadup survivor
    ///    set) and, like tenured LIST objects — which the young-list sweep
    ///    never counts — contribute nothing to the recomputed live bytes,
    ///    keeping `live_bytes` (the adaptive pacer term) on the same
    ///    definition it had before the migration.
    /// 2. Marked-at-parity slots are survivors: their VARIABLE byte size
    ///    (`object_bytes_from_header` — fixed struct + payload storage) is
    ///    summed into the returned live bytes, which both recompute sites
    ///    feed into `live_bytes`.
    /// 3. Dead slots: `on_free(addr)` (registry eviction hook — the vector
    ///    class evicts `vector_object_addrs` here), then
    ///    `drop_in_place::<T>` (strings own byte storage + interval tables,
    ///    vectors own their element `Vec` — a bit-clear-only free leaks them
    ///    all; NEVER `Box::from_raw` on page memory), then the alloc bit
    ///    clears and the slot threads onto the page-local free list.
    ///
    /// Pages that gained free slots join the arena's partial chain, so the
    /// class free list can hand their slots out again — including to a
    /// mutator running BETWEEN cooperative slices. That mid-sweep reuse is
    /// why each visit RE-READS the live bitmap word instead of a sweep-start
    /// snapshot. A slot reallocated mid-sweep re-enters allocated +
    /// born-at-parity ⇒ reads as marked ⇒ survivor.
    ///
    /// Returns `(survivor bytes, slots freed)`.
    pub(super) fn sweep_range(
        &mut self,
        start: usize,
        end: usize,
        parity: bool,
        mut on_free: impl FnMut(usize),
    ) -> (usize, usize) {
        let mut live_bytes = 0usize;
        let mut freed = 0usize;
        for page_index in start..end.min(self.pages.len()) {
            let page = &mut self.pages[page_index];
            if page.retired {
                continue; // never swept; slots permanently tenured-live
            }
            let mut freed_any = false;
            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                // RE-READ the current bitmap word (see the doc above).
                let mut bits = page.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    let slot = page.slot_ptr(index);
                    // Alloc bit set ⇒ the slot holds a fully written live
                    // object — reading its header is sound.
                    let header = unsafe { &*(slot as *const GcHeader) };
                    debug_assert!(
                        header.kind == T::KIND,
                        "wrong-kind header in a {} arena slot",
                        T::CLASS,
                    );
                    // (1) TENURED-SKIP before any parity interpretation.
                    if header.tenured {
                        continue;
                    }
                    if header.is_marked_at(parity) {
                        // (2) Survivor: variable-size byte accounting.
                        live_bytes = live_bytes.saturating_add(
                            TaggedHeap::object_bytes_from_header(slot as *const GcHeader),
                        );
                    } else {
                        // (3) Dead: evict from any class registry, drop the
                        // payload IN PLACE, then clear the bit (the oracle
                        // answers NOT-owned from here on).
                        on_free(slot as usize);
                        unsafe { std::ptr::drop_in_place(slot) };
                        page.free_slot(index);
                        freed += 1;
                        freed_any = true;
                    }
                }
            }
            if freed_any && !page.on_partial {
                page.on_partial = true;
                page.next_partial = self.partial_head;
                self.partial_head = page_index;
            }
        }
        (live_bytes, freed)
    }

    /// Release completely empty young pages after the whole class has been
    /// swept, then rebuild every index-bearing arena structure. This must not
    /// run between cooperative sweep slices: removing a `pages` element shifts
    /// later indices and would invalidate both the sweep cursor and partial
    /// chain. Slot storage has its own stable allocation, so compacting the
    /// `Vec<ObjectPage<T>>` does not move any surviving Lisp object.
    pub(super) fn release_empty_pages(&mut self) -> usize {
        let old_len = self.pages.len();
        if !self
            .pages
            .iter()
            .any(|page| page.allocated == 0 && !page.retired)
        {
            return 0;
        }

        self.pages
            .retain(|page| page.allocated != 0 || page.retired);
        self.pages.shrink_to_fit();

        self.page_index_by_base =
            FxHashMap::with_capacity_and_hasher(self.pages.len(), Default::default());
        self.partial_head = PAGE_NONE;
        for (page_index, page) in self.pages.iter_mut().enumerate() {
            let previous = self.page_index_by_base.insert(page.base_addr(), page_index);
            debug_assert!(previous.is_none(), "arena page base registered twice");

            page.next_partial = PAGE_NONE;
            page.on_partial = false;
            if page.free_head != PAGE_NONE {
                page.on_partial = true;
                page.next_partial = self.partial_head;
                self.partial_head = page_index;
            }
        }

        old_len - self.pages.len()
    }

    /// Collect raw pointers to every ALLOCATED slot (allocated-bit-first;
    /// retired pages INCLUDED — their slots are live tenured objects).
    /// Snapshot semantics: callers walk the returned vector while calling
    /// arbitrary `&self`/`&mut self` heap methods (verifiers, promotion).
    pub(super) fn collect_allocated_slots(&self) -> Vec<*mut T> {
        let mut out = Vec::new();
        for page in &self.pages {
            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                let mut bits = page.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    out.push(page.slot_ptr(word_index * usize::BITS as usize + bit));
                }
            }
        }
        out
    }

    /// Exact page/slot occupancy plus directly-owned payload capacity for
    /// diagnostics. The allocation bitmap is authoritative, just as it is for
    /// sweep and ownership checks; unallocated slot bytes are never read.
    pub(super) fn layout_stats(
        &self,
        payload_layout: impl Fn(&T) -> PayloadLayout,
    ) -> ArenaLayoutStats {
        let mut stats = ArenaLayoutStats {
            class: T::CLASS,
            pages: self.pages.len(),
            page_bytes: self.pages.len().saturating_mul(OBJECT_PAGE_BYTES),
            slot_bytes: T::SLOT_BYTES,
            slots_per_page: ObjectPage::<T>::SLOTS,
            ..ArenaLayoutStats::default()
        };

        for page in &self.pages {
            stats.bumped_slots = stats.bumped_slots.saturating_add(page.next_index);
            stats.allocated_slots = stats.allocated_slots.saturating_add(page.allocated);
            stats.reclaimed_slots = stats
                .reclaimed_slots
                .saturating_add(page.next_index.saturating_sub(page.allocated));
            stats.never_used_slots = stats
                .never_used_slots
                .saturating_add(ObjectPage::<T>::SLOTS.saturating_sub(page.next_index));
            stats.retired_pages += usize::from(page.retired);
            if page.allocated == 0 {
                stats.empty_pages += 1;
            } else if page.allocated == ObjectPage::<T>::SLOTS {
                stats.full_pages += 1;
            } else {
                stats.partial_pages += 1;
            }

            for word_index in 0..ObjectPage::<T>::ALLOC_WORDS {
                let mut bits = page.alloc_bits[word_index];
                while bits != 0 {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let index = word_index * usize::BITS as usize + bit;
                    let object = unsafe { &*page.slot_ptr(index) };
                    let header = unsafe { &*(object as *const T as *const GcHeader) };
                    if header.tenured {
                        stats.tenured_slots += 1;
                    } else {
                        stats.young_slots += 1;
                    }
                    let payload = payload_layout(object);
                    stats.payload_logical_bytes = stats
                        .payload_logical_bytes
                        .saturating_add(payload.logical_bytes);
                    stats.payload_capacity_bytes = stats
                        .payload_capacity_bytes
                        .saturating_add(payload.capacity_bytes);
                    stats.owned_payloads += usize::from(payload.owned);
                    stats.mapped_payloads += usize::from(payload.mapped);
                }
            }
        }

        stats.occupied_slot_bytes = stats.allocated_slots.saturating_mul(T::SLOT_BYTES);
        stats.object_struct_bytes = stats.allocated_slots.saturating_mul(size_of::<T>());
        debug_assert_eq!(
            stats.allocated_slots,
            stats.tenured_slots + stats.young_slots,
            "arena layout accounting lost an allocated slot",
        );
        stats
    }
}

pub(super) struct MappedConsRange {
    pub(super) start: *mut ConsCell,
    pub(super) len: usize,
    pub(super) mark_bits: Vec<usize>,
}

impl MappedConsRange {
    pub(super) fn new(start: *mut ConsCell, len: usize) -> Self {
        Self {
            start,
            len,
            mark_bits: vec![0; cons_mark_words(len)],
        }
    }

    #[inline]
    pub(super) fn contains_ptr(&self, ptr: *const ConsCell) -> bool {
        if ptr.is_null() || self.len == 0 {
            return false;
        }
        let start = self.start as usize;
        let end = start + self.len * size_of::<ConsCell>();
        let ptr = ptr as usize;
        start <= ptr && ptr < end && (ptr - start).is_multiple_of(size_of::<ConsCell>())
    }

    #[inline]
    pub(super) fn index_of_ptr(&self, ptr: *const ConsCell) -> usize {
        (ptr as usize - self.start as usize) / size_of::<ConsCell>()
    }

    #[inline]
    pub(super) fn is_marked_ptr(&self, ptr: *const ConsCell) -> bool {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        (self.mark_bits[mark.word_index] & mark.mask) != 0
    }

    #[inline]
    pub(super) fn mark_ptr(&mut self, ptr: *const ConsCell) {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        self.mark_bits[mark.word_index] |= mark.mask;
    }

    pub(super) fn clear_marks(&mut self) {
        self.mark_bits.fill(0);
    }

    /// Mark every cell in the range live (dump-partition: born black). Sets
    /// exactly `len` bits so `live_count` stays exact.
    pub(super) fn mark_all(&mut self) {
        self.mark_bits.fill(!0);
        let rem = self.len % CONS_MARK_BITS_PER_WORD;
        if rem != 0
            && let Some(last) = self.mark_bits.last_mut()
        {
            *last = (1usize << rem) - 1;
        }
    }

    pub(super) fn live_count(&self) -> usize {
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

pub(super) struct MappedFloatRange {
    pub(super) start: *mut FloatObj,
    pub(super) len: usize,
    pub(super) mark_bits: Vec<usize>,
}

impl MappedFloatRange {
    pub(super) fn new(start: *mut FloatObj, len: usize) -> Self {
        Self {
            start,
            len,
            mark_bits: vec![0; cons_mark_words(len)],
        }
    }

    #[inline]
    pub(super) fn contains_ptr(&self, ptr: *const FloatObj) -> bool {
        if ptr.is_null() || self.len == 0 {
            return false;
        }
        let start = self.start as usize;
        let end = start + self.len * size_of::<FloatObj>();
        let ptr = ptr as usize;
        start <= ptr && ptr < end && (ptr - start).is_multiple_of(size_of::<FloatObj>())
    }

    #[inline]
    pub(super) fn index_of_ptr(&self, ptr: *const FloatObj) -> usize {
        (ptr as usize - self.start as usize) / size_of::<FloatObj>()
    }

    #[inline]
    pub(super) fn is_marked_ptr(&self, ptr: *const FloatObj) -> bool {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        (self.mark_bits[mark.word_index] & mark.mask) != 0
    }

    #[inline]
    pub(super) fn mark_ptr(&mut self, ptr: *const FloatObj) {
        let index = self.index_of_ptr(ptr);
        let mark = ConsBlock::mark_bit(index);
        self.mark_bits[mark.word_index] |= mark.mask;
    }

    pub(super) fn clear_marks(&mut self) {
        self.mark_bits.fill(0);
    }

    /// Mark every cell in the range live (dump-partition: born black). Sets
    /// exactly `len` bits so `live_count` stays exact.
    pub(super) fn mark_all(&mut self) {
        self.mark_bits.fill(!0);
        let rem = self.len % CONS_MARK_BITS_PER_WORD;
        if rem != 0
            && let Some(last) = self.mark_bits.last_mut()
        {
            *last = (1usize << rem) - 1;
        }
    }

    pub(super) fn live_count(&self) -> usize {
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

pub(super) struct MappedVecLikeObject {
    pub(super) header: *mut VecLikeHeader,
    pub(super) byte_len: usize,
    pub(super) marked: bool,
}

impl MappedVecLikeObject {
    pub(super) fn new(header: *mut VecLikeHeader, byte_len: usize) -> Self {
        Self {
            header,
            byte_len,
            marked: false,
        }
    }
}

pub(super) struct MappedStringObject {
    pub(super) ptr: *mut StringObj,
    pub(super) byte_len: usize,
    pub(super) marked: bool,
}

impl MappedStringObject {
    pub(super) fn new(ptr: *mut StringObj, byte_len: usize) -> Self {
        Self {
            ptr,
            byte_len,
            marked: false,
        }
    }
}

/// Per-kind breakdown of the values the GC thread parked in `deferred` for the
/// STW termination drain, taken as `join_concurrent_mark` folds the buffer into
/// gray. Sizes the concurrent-tracing extension: which kind a further
/// concurrent tier should take on first (strings are mark-only + intervals;
/// records/closures need atomic slots + snapshot/clone-on-write; weak/growable
/// hash tables stay deferred regardless). Counts are ENTRIES, not unique
/// objects — the GC thread parks a value once per discovered edge, and the
/// termination's `mark_value` dedups. Diagnostics only.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DrainKinds {
    pub string: usize,
    /// Vectors trace concurrently (Stage 2 Tier B scans their BACKINGS), and
    /// since task 01 the GC thread also CLAIMS owned page vectors' header
    /// marks — so this bucket counts only the still-parked residue
    /// (mapped/Box-residual vectors, and snapshot-missed edge cases).
    pub vector: usize,
    pub record: usize,
    /// Lambda + Macro (interpreted closures).
    pub closure: usize,
    /// Since task 01's bytecode arm the GC thread CLAIMS owned page
    /// bytecode (children gray-pushed at claim time) — this bucket counts
    /// only the still-parked residue (mapped/dump-span bytecode and
    /// mid-cycle-page snapshot misses).
    pub bytecode: usize,
    pub hash_table: usize,
    /// CharTable + SubCharTable.
    pub char_table: usize,
    pub float: usize,
    /// Non-owned conses (new-block or mapped) the GC thread could not mark via
    /// its start-of-cycle block snapshot.
    pub cons: usize,
    /// Built-in functions — a large near-constant population (~1.7k registered
    /// at startup), split out so it does not mask the true `other` residue.
    pub subr: usize,
    /// Every remaining veclike (marker/buffer/overlay/bignum/...).
    pub other: usize,
}

impl DrainKinds {
    /// Classify one parked value into its bucket — the same tag dispatch
    /// `mark_value` uses (cons/string/float, then the veclike `type_tag`).
    ///
    /// # Safety
    /// `val` must be a live heap value: `join_concurrent_mark` runs before the
    /// termination drain and sweep, and nothing is freed during a concurrent
    /// mark (allocate-black; sweeps never overlap marking), so every parked
    /// entry's header is still valid.
    pub(super) unsafe fn note(&mut self, val: TaggedValue) {
        if val.is_cons() {
            self.cons += 1;
        } else if val.is_string() {
            self.string += 1;
        } else if val.is_float() {
            self.float += 1;
        } else if val.is_veclike() {
            let ptr = val.as_veclike_ptr().unwrap();
            match unsafe { (*ptr).type_tag } {
                VecLikeType::Vector => self.vector += 1,
                VecLikeType::Record => self.record += 1,
                VecLikeType::Lambda | VecLikeType::Macro => self.closure += 1,
                VecLikeType::ByteCode => self.bytecode += 1,
                VecLikeType::HashTable => self.hash_table += 1,
                VecLikeType::CharTable | VecLikeType::SubCharTable => self.char_table += 1,
                VecLikeType::Subr => self.subr += 1,
                _ => self.other += 1,
            }
        } else {
            self.other += 1; // unreachable: only heap objects are parked
        }
    }

    /// Fold `cycle`'s per-kind counts into this lifetime per-kind maximum.
    pub(super) fn merge_max(&mut self, cycle: &DrainKinds) {
        self.string = self.string.max(cycle.string);
        self.vector = self.vector.max(cycle.vector);
        self.record = self.record.max(cycle.record);
        self.closure = self.closure.max(cycle.closure);
        self.bytecode = self.bytecode.max(cycle.bytecode);
        self.hash_table = self.hash_table.max(cycle.hash_table);
        self.char_table = self.char_table.max(cycle.char_table);
        self.float = self.float.max(cycle.float);
        self.cons = self.cons.max(cycle.cons);
        self.subr = self.subr.max(cycle.subr);
        self.other = self.other.max(cycle.other);
    }

    /// Sum of all buckets — equals the deferred-entry count it was built from.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn total(&self) -> usize {
        self.string
            + self.vector
            + self.record
            + self.closure
            + self.bytecode
            + self.hash_table
            + self.char_table
            + self.float
            + self.cons
            + self.subr
            + self.other
    }
}

impl std::fmt::Display for DrainKinds {
    /// Compact trace-line segment: `str=N vec=N rec=N clo=N bc=N ht=N ct=N f=N
    /// cons=N sub=N other=N`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "str={} vec={} rec={} clo={} bc={} ht={} ct={} f={} cons={} sub={} other={}",
            self.string,
            self.vector,
            self.record,
            self.closure,
            self.bytecode,
            self.hash_table,
            self.char_table,
            self.float,
            self.cons,
            self.subr,
            self.other,
        )
    }
}

/// A point-in-time accounting of one fixed-stride object arena. This is
/// diagnostics-only and intentionally separates the always-resident 64 KiB
/// page backing from payload allocations owned by live objects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArenaLayoutStats {
    pub class: &'static str,
    pub pages: usize,
    pub page_bytes: usize,
    pub slot_bytes: usize,
    pub slots_per_page: usize,
    pub allocated_slots: usize,
    pub tenured_slots: usize,
    pub young_slots: usize,
    pub bumped_slots: usize,
    pub reclaimed_slots: usize,
    pub never_used_slots: usize,
    pub empty_pages: usize,
    pub partial_pages: usize,
    pub full_pages: usize,
    pub retired_pages: usize,
    pub occupied_slot_bytes: usize,
    pub object_struct_bytes: usize,
    pub payload_logical_bytes: usize,
    pub payload_capacity_bytes: usize,
    pub owned_payloads: usize,
    pub mapped_payloads: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PayloadLayout {
    pub(super) logical_bytes: usize,
    pub(super) capacity_bytes: usize,
    pub(super) owned: bool,
    pub(super) mapped: bool,
}

impl PayloadLayout {
    pub(super) fn add(self, other: Self) -> Self {
        Self {
            logical_bytes: self.logical_bytes.saturating_add(other.logical_bytes),
            capacity_bytes: self.capacity_bytes.saturating_add(other.capacity_bytes),
            owned: self.owned || other.owned,
            mapped: self.mapped || other.mapped,
        }
    }
}

/// Exact ordinary-cons block occupancy. Mapped pdump conses are reported in
/// [`MappedLayoutStats`] because they do not consume allocator-backed blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConsLayoutStats {
    pub pages: usize,
    pub page_bytes: usize,
    pub capacity_slots: usize,
    pub bumped_slots: usize,
    pub live_slots: usize,
    pub reclaimed_slots: usize,
    pub never_used_slots: usize,
    pub empty_pages: usize,
    pub partial_pages: usize,
    pub full_pages: usize,
    pub occupied_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MappedLayoutStats {
    pub conses: usize,
    pub floats: usize,
    pub strings: usize,
    pub veclikes: usize,
    pub object_image_bytes: usize,
    pub copied_string_payloads: usize,
    pub copied_string_capacity_bytes: usize,
    pub copied_veclike_payloads: usize,
    pub copied_veclike_capacity_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BoxedKindLayoutStats {
    pub class: &'static str,
    pub objects: usize,
    pub tenured_objects: usize,
    /// Object struct plus directly-owned backing capacities known to the GC.
    /// Nested allocations inside structural hash keys are not included.
    pub known_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HeapLayoutStats {
    pub allocated_objects: usize,
    pub managed_live_bytes: usize,
    pub page_backing_bytes: usize,
    pub known_payload_capacity_bytes: usize,
    pub cons: ConsLayoutStats,
    pub arenas: Vec<ArenaLayoutStats>,
    pub mapped: MappedLayoutStats,
    pub boxed: Vec<BoxedKindLayoutStats>,
}

/// Snapshot of the deferred-sweep cost accounting plus the concurrent-mark
/// termination drain probe. Diagnostics only: per-cycle fields hold the most
/// recently completed (or in-flight) deferred sweep; lifetime fields aggregate
/// across the heap's life, with the eager STW sweep feeding `lifetime_sweep_us`
/// too so the two sweep paths are comparable.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SweepStats {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub sweep_us: u64,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub slice_count: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub cons_blocks_swept: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub noncons_freed: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_sweep_us: u64,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_slices: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_cons_blocks_swept: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub lifetime_noncons_freed: usize,
    /// Values `join_concurrent_mark` folded into the termination gray queue:
    /// the GC thread's parked non-cons buffer and the residual SATB log.
    pub last_termination_deferred: usize,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub max_termination_deferred: usize,
    pub last_termination_satb: usize,
    /// Per-kind breakdown of `last_termination_deferred`, plus the lifetime
    /// per-kind maximum (each bucket's own max across cycles). Populated in
    /// crate tests and under `NEOVM_GC_TRACE=1`; zero otherwise — the
    /// classification's header reads are not free STW time.
    pub last_termination_kinds: DrainKinds,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub max_termination_kinds: DrainKinds,
    /// CONCURRENT STRING MARKING: owned interval-free strings the GC thread
    /// claimed concurrently last cycle — string marks that LEFT the STW drain
    /// (the `kinds.string` bucket keeps counting the strings still parked:
    /// interval-bearing + mapped/dump-span ones). Always populated.
    pub last_concurrent_str_claimed: usize,
    /// CONCURRENT FLOAT CLAIMS (task 01): owned young page floats the GC
    /// thread claimed concurrently last cycle (the `kinds.float` bucket keeps
    /// counting the still-parked ones: snapshot-missed/mapped/Box floats).
    pub last_concurrent_float_claimed: usize,
    /// SUBR RECOGNIZE-AND-DROP (task 01): defer-path drops of leaked-static
    /// subrs last cycle (drop EVENTS — one per discovered edge, not unique
    /// subrs; the `kinds.subr` bucket keeps counting mapped subrs, which
    /// still park).
    pub last_concurrent_subr_dropped: usize,
    /// CONCURRENT VECTOR-HEADER CLAIMS (task 01): owned young page vectors
    /// whose header the GC thread claimed last cycle (the `kinds.vector`
    /// bucket keeps counting the still-parked ones: mapped/Box-residual).
    pub last_concurrent_vec_claimed: usize,
    /// CONCURRENT BYTECODE CLAIMS (task 01): owned young page bytecode the
    /// GC thread claimed (children gray-pushed) last cycle (the
    /// `kinds.bytecode` bucket keeps counting the still-parked residue:
    /// mapped/dump-span and mid-cycle-page bytecode).
    pub last_concurrent_bc_claimed: usize,
    /// Cost of the `join_concurrent_mark` fold itself (taking the SATB +
    /// deferred buffers, classifying, pushing to gray) — the cheap half of the
    /// termination; the mark fixpoint that follows is the trace line's `drain`.
    pub last_termination_fold_us: u64,
    /// Lifetime count of concurrent-mark terminations (`join_concurrent_mark`
    /// calls), so a probe polling between eval chunks can detect a new cycle.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub termination_count: usize,
    /// Mark cost of the most recent cycle at `incremental_finish`. For a
    /// concurrent cycle this is exactly the STW termination drain: the counter
    /// resets at `concurrent_begin` and the termination's
    /// `incremental_drain_all` is the only accumulation.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub mark_us: u64,
}

/// One root group's cost inside a `seed_all_context_roots` call:
/// `(name, wall µs enumerating+seeding the group, values the group visited)`.
/// The count is the enumeration volume (every value fed to the seeding sink,
/// BEFORE the non-heap-object / mapped-root filters), which is what the walk's
/// O() actually scales with.
pub(crate) type RootGroup = (&'static str, u64, usize);

/// Per-group decomposition of one `seed_all_context_roots` call (one root
/// handshake's context-root seeding). Built fresh each call by the evaluator.
#[derive(Clone, Debug, Default)]
pub(crate) struct RootSeedBreakdown {
    /// Whole-call wall time (all groups + thread-local + marker heads).
    pub total_us: u64,
    /// Ordered per-group `(name, µs, values visited)` records.
    pub groups: Vec<RootGroup>,
}

impl RootSeedBreakdown {
    /// Compact `name=USus/COUNT` rendering of the nonzero groups, for the
    /// `NEOVM_GC_TRACE` handshake lines.
    pub(crate) fn format_nonzero(&self) -> String {
        let mut out = String::new();
        for &(name, us, count) in &self.groups {
            if us == 0 && count == 0 {
                continue;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&format!("{name}={us}us/{count}"));
        }
        out
    }

    /// Count for a named group (0 if absent) — test/probe convenience.
    #[cfg(test)]
    pub(crate) fn group_count(&self, name: &str) -> usize {
        self.groups
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|&(_, _, c)| c)
            .unwrap_or(0)
    }
}

/// STW pause instrumentation for the concurrent collector's TWO handshakes —
/// the START handshake (`start_concurrent_mark`: clear marks, obarray
/// snapshot, root seeding, cons/vector snapshots, job assembly) and the
/// TERMINATION handshake (`terminate_concurrent_mark`: join+fold, root
/// re-seeding, residual drain, weak/finalizer/marker post-passes) — each
/// decomposed per phase and per root GROUP, plus once-per-handshake O() size
/// probes. Sibling of [`SweepStats`]; diagnostics only (no behavior).
/// Heap-side phases are recorded where they run in this file; the evaluator
/// records the context-root breakdown and the context-side probes via
/// `handshake_stats_mut`.
#[derive(Clone, Debug, Default)]
pub(crate) struct HandshakeStats {
    // --- START handshake (once per concurrent cycle) ---
    /// Lifetime count of concurrent start handshakes (`concurrent_begin`).
    pub start_count: usize,
    /// Whole start-handshake pause (µs) and its lifetime max.
    pub last_start_total_us: u64,
    pub max_start_total_us: u64,
    /// `begin_collection` mark-bit clearing (young non-cons + cons blocks).
    pub last_start_clear_us: u64,
    /// The clear's three-way split (task #7 stage 2a diagnostics rider):
    /// cons-block bitmap memset / young non-cons `all_objects` walk (the only
    /// component an epoch/parity mark-bit design would remove) / mapped
    /// (pdump) mark-state resets.
    pub last_start_clear_cons_us: u64,
    pub last_start_clear_noncons_us: u64,
    pub last_start_clear_mapped_us: u64,
    /// `seed_internal_runtime_roots` at start (registries + doomed queue).
    pub last_start_runtime_us: u64,
    pub last_start_runtime_roots: usize,
    /// `seed_mapped_remembered` at start (dump remembered-set re-scan).
    pub last_start_remembered_us: u64,
    pub last_start_remembered_roots: usize,
    /// `obarray.scan_snapshot()` capture.
    pub last_start_obsnap_us: u64,
    /// `seed_all_context_roots` at start, per group.
    pub last_start_roots: RootSeedBreakdown,
    /// `launch_concurrent_mark` phases: cons-block base snapshot, vector
    /// backing snapshot, and the residual job assembly + thread send.
    pub last_start_conssnap_us: u64,
    pub last_start_vecsnap_us: u64,
    /// CONCURRENT FLOAT CLAIMS (task 01): float-arena page-base snapshot
    /// capture — O(pages) only, mirrors `last_start_vecsnap_us`.
    pub last_start_floatsnap_us: u64,
    /// CONCURRENT VECTOR-HEADER CLAIMS (task 01): vector-arena page-BASE
    /// snapshot capture (distinct from the Tier-B backing `vecsnap`).
    pub last_start_vecbasesnap_us: u64,
    /// CONCURRENT BYTECODE CLAIMS (task 01): bytecode-arena page-base
    /// snapshot capture — O(pages) only, mirrors `last_start_vecbasesnap_us`.
    pub last_start_bcsnap_us: u64,
    pub last_start_jobasm_us: u64,

    // --- TERMINATION handshake (once per concurrent cycle) ---
    /// Lifetime count of termination reseeds
    /// (`reseed_runtime_and_remembered_roots`).
    pub term_count: usize,
    /// The whole pre-drain roots lump (join → reseed → ctx roots → new
    /// symbols), as printed by the existing `roots=` trace field, + max.
    pub last_term_roots_total_us: u64,
    pub max_term_roots_total_us: u64,
    /// `join_concurrent_mark` total (stop signal + thread exit wait + the
    /// SATB/deferred fold; the fold alone is `SweepStats::
    /// last_termination_fold_us`).
    pub last_term_join_us: u64,
    /// `seed_internal_runtime_roots` at termination.
    pub last_term_runtime_us: u64,
    pub last_term_runtime_roots: usize,
    /// `seed_mapped_remembered` at termination.
    pub last_term_remembered_us: u64,
    pub last_term_remembered_roots: usize,
    /// `seed_all_context_roots` at termination, per group.
    pub last_term_ctxroots: RootSeedBreakdown,
    /// Stage 1b residual: `trace_new_symbol_cells` over mid-cycle interns.
    pub last_term_newsyms_us: u64,
    pub last_term_newsyms_roots: usize,
    /// `incremental_finish` post-drain passes: doomed-finalizer scan, weak
    /// hash-table sweep, dead-marker unchaining.
    pub last_term_finalizer_us: u64,
    pub last_term_weak_us: u64,
    pub last_term_unchain_us: u64,

    // --- O() size probes (refreshed at each handshake) ---
    /// JIT COMPILED cache: total cached entries / total reloc slots walked.
    pub probe_jit_compiled_entries: usize,
    pub probe_jit_reloc_slots: usize,
    /// `mapped_remembered.len()` — the dump remembered set (never cleared).
    pub probe_mapped_remembered: usize,
    /// Bytecode operand-stack buffer depth (`bc_buf`).
    pub probe_bc_buf_depth: usize,
    /// Binding-stack depth (`specpdl`).
    pub probe_specpdl_depth: usize,
    /// Obarray logical slots + chunk count (start snapshot / current).
    pub probe_obarray_slots: usize,
    pub probe_obarray_chunks: usize,
    /// Vector-backing snapshot length (Tier B, captured at start).
    pub probe_vector_snapshot_len: usize,
    /// Owned cons blocks snapshotted at start.
    pub probe_cons_blocks: usize,
    /// Live buffers (= marker chain-head slots installed).
    pub probe_buffer_count: usize,
}

impl HandshakeStats {
    /// Compact probe rendering for the `NEOVM_GC_TRACE` handshake lines.
    pub(crate) fn format_probes(&self) -> String {
        format!(
            "jit={}/{} rem={} bc={} spec={} obslots={} obchunks={} vecs={} consblk={} bufs={}",
            self.probe_jit_compiled_entries,
            self.probe_jit_reloc_slots,
            self.probe_mapped_remembered,
            self.probe_bc_buf_depth,
            self.probe_specpdl_depth,
            self.probe_obarray_slots,
            self.probe_obarray_chunks,
            self.probe_vector_snapshot_len,
            self.probe_cons_blocks,
            self.probe_buffer_count,
        )
    }
}
