//! Allocation regions: conses and floats are handed out from bump regions —
//! a cursor and a limit per class in `JitHeapState`, shared by the Rust
//! allocator and compiled code — instead of one free-list pop or block bump
//! (and its bookkeeping) per object.
//!
//! **Where a region comes from.** A refill (cold) takes one contiguous run
//! in GNU `Fcons`/`make_float`'s source order: for conses the head of the
//! free list (and every cell after it that is the next address, up to the
//! budget), else the newest block's bump tail, else a fresh block; for
//! floats the first partial page's free-list run, else the newest page's
//! bump tail, else a new page (`ObjectArena::reserve_run`). A float
//! region's slots get their full header at the grant (born at the current
//! parity), so "alloc bit set implies a readable header" holds for every
//! reserved slot, handed out or not.
//!
//! **Allocate-black is a property of the region.** A region granted while
//! the heap allocates black (a deferred sweep or a concurrent mark is in
//! flight) has all its cells pre-marked, so a hand-out never tests the
//! phase. Every assignment to a flag that changes the phase —
//! `mark_parity`, `concurrent_mark_running`, `sweep_in_progress` — is
//! preceded by [`TaggedHeap::close_alloc_regions`], so a region never
//! outlives the phase it was granted in (CONCURRENT_GC.md invariant I1).
//!
//! **Counters are charged once per region.** Granting charges
//! `memory-use-counts`, `allocated_count`, `cons_live_count` and
//! `bytes_since_gc` for the whole run; closing refunds the unused tail. The
//! exact views (`memory_use_counts_snapshot`, `allocated_count()`,
//! `bytes_since_gc_exact`, `total_allocated_bytes`) subtract the open
//! region's unused cells, so Lisp reads exact counts at any moment;
//! `bytes_since_gc()` — the pacing gates — reads the charged value, never
//! below the exact one, and [`TaggedHeap::region_budget`] keeps a grant from
//! charging past the threshold, so a collection comes when it did before
//! (at most one region early, never late).
//!
//! **Collector code never sees an open region** (I2): every collector entry
//! and walker closes them first. An open free-list region whose reserved
//! cells were swept would hand them out twice.

use super::*;

/// The most cells one cons region takes (2 KiB).
pub(super) const CONS_REGION_MAX_CELLS: usize = 128;
/// The most slots one float region takes (2 KiB of slots, 1.5 KiB charged:
/// a float is charged `size_of::<FloatObj>()`, 24 bytes).
pub(super) const FLOAT_REGION_MAX_SLOTS: usize = 64;
/// A float slot's stride.
const FLOAT_SLOT: usize = <FloatObj as PagedObject>::SLOT_BYTES;

/// Where the open cons region's cells came from, which decides how its
/// unused tail goes back at close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConsRegionSource {
    /// No region is open.
    Closed,
    /// A run of address-adjacent cells from the head of the free list.
    FreeListRun,
    /// The bump tail of the block at this `cons_blocks` index.
    BlockTail { block: usize },
}

/// Where the open float region's slots came from: which page, and whether
/// its unhanded tail rewinds that page's bump cursor or goes back on its
/// free list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FloatRegionSource {
    Closed,
    Run { page: usize, source: ArenaRunSource },
}

/// The collector color the open region's cells were born with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegionColor {
    /// Granted while the heap allocates white: cells unmarked.
    White,
    /// Granted while a deferred sweep or a concurrent mark was in flight:
    /// every cell pre-marked, as `alloc_cons` used to mark each one.
    Black,
}

/// The open regions' bookkeeping that only Rust reads.
#[derive(Clone, Copy, Debug)]
pub(super) struct RegionBook {
    pub(super) cons: ConsRegionSource,
    pub(super) cons_color: RegionColor,
    /// Floats have no color: a slot is born at the current parity, which a
    /// region cannot outlive (it closes before every parity flip).
    pub(super) float: FloatRegionSource,
}

impl RegionBook {
    pub(super) const fn new() -> Self {
        Self {
            cons: ConsRegionSource::Closed,
            cons_color: RegionColor::White,
            float: FloatRegionSource::Closed,
        }
    }
}

/// Region refill statistics since the last collection finished, traced
/// under `RUST_LOG=neovm::gc::region=debug` and then reset: how often each
/// source refilled, how long the runs were, how many were short (fewer than
/// 8 cells: a fragmented free list makes nearly every allocation a refill),
/// and how many regions closed with an unused tail.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RegionStats {
    pub(crate) cons_refills: u64,
    pub(crate) cons_cells_granted: u64,
    pub(crate) cons_short_runs: u64,
    pub(crate) cons_from_free_list: u64,
    pub(crate) cons_from_block_tail: u64,
    pub(crate) cons_from_fresh_block: u64,
    pub(crate) cons_tail_closes: u64,
    pub(crate) float_refills: u64,
    pub(crate) float_slots_granted: u64,
    pub(crate) float_short_runs: u64,
    pub(crate) float_tail_closes: u64,
}

impl TaggedHeap {
    /// A cell for a new cons: the open region's next cell, else a refill.
    /// The only cons source; `alloc_cons` and `list_from_slice` write car
    /// and cdr. Nothing is charged or marked per cell (the region was).
    #[inline(always)]
    pub(super) fn take_cons_cell(&mut self) -> *mut ConsCell {
        let cur = self.jit.cons_cur.get();
        if cur < self.jit.cons_lim.get() {
            self.jit.cons_cur.set(cur + size_of::<ConsCell>());
            return cur as *mut ConsCell;
        }
        self.refill_cons_region()
    }

    /// The open region is exhausted (or none is open): close it, grant a new
    /// one — charged, pre-marked when the heap allocates black — and hand
    /// out its first cell.
    #[cold]
    #[inline(never)]
    fn refill_cons_region(&mut self) -> *mut ConsCell {
        self.close_cons_region();
        let want = self.region_budget(size_of::<ConsCell>(), CONS_REGION_MAX_CELLS);
        let (first, n, source) = self.reserve_cons_run(want);
        debug_assert!(n >= 1 && n <= want);
        let color = if self.allocates_black() {
            let base = ConsBlock::block_base_for_ptr(first);
            ConsBlock::mark_run_at(base, ConsBlock::index_of_ptr(first), n, true);
            RegionColor::Black
        } else {
            RegionColor::White
        };
        self.charge_conses(n);
        self.region_book.cons = source;
        self.region_book.cons_color = color;
        let stats = &mut self.region_stats;
        stats.cons_refills += 1;
        stats.cons_cells_granted += n as u64;
        stats.cons_short_runs += u64::from(n < 8);
        let first_addr = first as usize;
        self.jit.cons_cur.set(first_addr + size_of::<ConsCell>());
        self.jit
            .cons_lim
            .set(first_addr + n * size_of::<ConsCell>());
        first
    }

    /// One run of up to `want` (at least one) cells, in GNU `Fcons`'s
    /// source order: the free list's head and the address-adjacent cells
    /// after it, else the newest block's bump tail, else a fresh block.
    fn reserve_cons_run(&mut self, want: usize) -> (*mut ConsCell, usize, ConsRegionSource) {
        let head = self.cons_free_list;
        if !head.is_null() {
            // The sweep threads each block's dead cells high to low onto the
            // head, so the list ascends within a block; a run stops at the
            // first gap. It cannot cross into another block: the mark bitmap
            // follows a block's last cell.
            let mut n = 1;
            // SAFETY: free-list nodes are reclaimed cells of owned blocks.
            let mut next = unsafe { (*head).free_next() };
            while n < want && next as usize == head as usize + n * size_of::<ConsCell>() {
                n += 1;
                next = unsafe { (*next).free_next() };
            }
            self.cons_free_list = next;
            self.region_stats.cons_from_free_list += 1;
            return (head, n, ConsRegionSource::FreeListRun);
        }
        if let Some(block) = self.cons_blocks.len().checked_sub(1) {
            let (first, n) = self.cons_blocks[block].reserve_tail(want);
            if n > 0 {
                self.region_stats.cons_from_block_tail += 1;
                // SAFETY: `first` is below the block's cell count.
                let cell = unsafe { self.cons_blocks[block].cells_ptr().add(first) };
                return (cell, n, ConsRegionSource::BlockTail { block });
            }
        }
        self.region_stats.cons_from_fresh_block += 1;
        self.reserve_cons_run_in_fresh_block(want)
    }

    /// Every block is full and nothing was reclaimed: open a fresh block
    /// (GNU's `cons_block` rollover) and take the run from its start.
    fn reserve_cons_run_in_fresh_block(
        &mut self,
        want: usize,
    ) -> (*mut ConsCell, usize, ConsRegionSource) {
        let mut block = ConsBlock::new();
        let block_base = block.base_addr();
        let (first, n) = block.reserve_tail(want);
        debug_assert!(first == 0 && n >= 1, "a fresh block has room");
        let cell = block.cells_ptr();
        self.cons_blocks.push(block);
        let block_index = self.cons_blocks.len() - 1;
        self.cons_block_index_by_base
            .insert(block_base, block_index);
        if let Some(map) = self.chunk_map.as_deref() {
            map.set(block_base, ChunkEntry::new(ChunkClass::Cons, block_index));
        }
        (cell, n, ConsRegionSource::BlockTail { block: block_index })
    }

    /// How many objects of `size` bytes the next region may take, at most
    /// `max`: never enough to charge the consing counter past the collection
    /// threshold while the region is open, so the charged counter crosses it
    /// on the same allocation the exact one would. Already over (a
    /// collection pending or inhibited, or a mark running): `max`.
    pub(super) fn region_budget(&self, size: usize, max: usize) -> usize {
        let remaining = self.gc_threshold.saturating_sub(self.bytes_since_gc);
        if remaining == 0 {
            max
        } else {
            ((remaining - 1) / size).clamp(1, max)
        }
    }

    /// Charge a granted region of `n` conses to every consing counter.
    fn charge_conses(&mut self, n: usize) {
        self.add_memory_use_count(MemoryUseCountSlot::ConsCells, n as u64);
        self.allocated_count += n;
        self.cons_live_count += n;
        self.note_allocation_bytes(n * size_of::<ConsCell>());
    }

    /// Refund `m` never-handed-out conses of a closing region.
    fn refund_conses(&mut self, m: usize) {
        let index = MemoryUseCountSlot::ConsCells.index();
        self.memory_use_counts[index] = self.memory_use_counts[index].wrapping_sub(m as u64);
        self.allocated_count -= m;
        self.cons_live_count -= m;
        self.bytes_since_gc -= m * size_of::<ConsCell>();
    }

    /// Cells of the open cons region not handed out yet (0 when closed).
    #[inline]
    pub(super) fn open_cons_unused(&self) -> usize {
        (self.jit.cons_lim.get() - self.jit.cons_cur.get()) / size_of::<ConsCell>()
    }

    /// Slots of the open float region not handed out yet (0 when closed).
    #[inline]
    pub(super) fn open_float_unused(&self) -> usize {
        (self.jit.float_lim.get() - self.jit.float_cur.get()) / FLOAT_SLOT
    }

    /// Whether any allocation region is open.
    pub(crate) fn alloc_regions_open(&self) -> bool {
        self.region_book.cons != ConsRegionSource::Closed
            || self.region_book.float != FloatRegionSource::Closed
    }

    /// Close every open allocation region: refund, un-mark and give back its
    /// unused tail. Called before every change of collector phase and at the
    /// top of every collector walker; cheap when nothing is open.
    #[inline]
    pub(crate) fn close_alloc_regions(&mut self) {
        if self.region_book.cons != ConsRegionSource::Closed {
            self.close_cons_region();
        }
        if self.region_book.float != FloatRegionSource::Closed {
            self.close_float_region();
        }
    }

    /// A slot for a new float: the open region's next slot (its header
    /// already written), else a refill.
    #[inline(always)]
    pub(super) fn take_float_slot(&mut self) -> *mut FloatObj {
        let cur = self.jit.float_cur.get();
        if cur < self.jit.float_lim.get() {
            self.jit.float_cur.set(cur + FLOAT_SLOT);
            return cur as *mut FloatObj;
        }
        self.refill_float_region()
    }

    /// The open float region is exhausted (or none is open): close it,
    /// reserve a new run, write every slot's full header born at the current
    /// parity, charge the run, and hand out its first slot.
    #[cold]
    #[inline(never)]
    fn refill_float_region(&mut self) -> *mut FloatObj {
        self.close_float_region();
        let want = self.region_budget(size_of::<FloatObj>(), FLOAT_REGION_MAX_SLOTS);
        let run = self.float_arena.reserve_run(want);
        debug_assert!(run.count >= 1 && run.count <= want);
        let parity = self.mark_parity;
        let page = &self.float_arena.pages[run.page];
        for i in 0..run.count {
            let slot = page.slot_ptr(run.first + i);
            // SAFETY: a reserved slot of an owned page. FULL-HEADER WRITE:
            // never partially reuse prior slot bytes (see `alloc_float`).
            unsafe {
                std::ptr::write(
                    slot,
                    FloatObj {
                        header: GcHeader::new_marked(HeapObjectKind::Float, parity),
                        value: 0.0,
                    },
                );
            }
        }
        let first = page.slot_ptr(run.first);
        self.charge_floats(run.count);
        self.region_book.float = FloatRegionSource::Run {
            page: run.page,
            source: run.source,
        };
        let stats = &mut self.region_stats;
        stats.float_refills += 1;
        stats.float_slots_granted += run.count as u64;
        stats.float_short_runs += u64::from(run.count < 8);
        let first_addr = first as usize;
        self.jit.float_cur.set(first_addr + FLOAT_SLOT);
        self.jit.float_lim.set(first_addr + run.count * FLOAT_SLOT);
        first
    }

    /// Charge a granted region of `n` floats.
    fn charge_floats(&mut self, n: usize) {
        self.add_memory_use_count(MemoryUseCountSlot::Floats, n as u64);
        self.allocated_count += n;
        self.note_allocation_bytes(n * size_of::<FloatObj>());
    }

    /// Refund `m` never-handed-out floats of a closing region.
    fn refund_floats(&mut self, m: usize) {
        let index = MemoryUseCountSlot::Floats.index();
        self.memory_use_counts[index] = self.memory_use_counts[index].wrapping_sub(m as u64);
        self.allocated_count -= m;
        self.bytes_since_gc -= m * size_of::<FloatObj>();
    }

    /// Close the float region: refund its unhanded slots and give them back
    /// to their page (`ObjectArena::give_back_run`).
    #[inline(never)]
    fn close_float_region(&mut self) {
        let book = self.region_book.float;
        let cur = self.jit.float_cur.get();
        let lim = self.jit.float_lim.get();
        self.jit.float_cur.set(0);
        self.jit.float_lim.set(0);
        self.region_book.float = FloatRegionSource::Closed;
        let FloatRegionSource::Run { page, source } = book else {
            debug_assert!(cur == 0 && lim == 0, "a closed region has no cursor");
            return;
        };
        let unused = (lim - cur) / FLOAT_SLOT;
        if unused == 0 {
            return;
        }
        self.region_stats.float_tail_closes += 1;
        self.refund_floats(unused);
        let base = self.float_arena.pages[page].base_addr();
        debug_assert!(cur >= base && lim <= base + OBJECT_PAGE_BYTES);
        let first = (cur - base) / FLOAT_SLOT;
        self.float_arena.give_back_run(page, first, unused, source);
    }

    /// Close the cons region (see [`Self::close_alloc_regions`]). The unused
    /// cells `[cur, lim)` go back where they came from: a block tail still
    /// ending at `lim` rewinds its bump cursor (after the un-mark, keeping
    /// "bits at or above `next_index` are never set"); anything else is
    /// pushed back on the free list high to low, so the list stays ascending
    /// and every cell's car is `DEAD` again.
    #[inline(never)]
    fn close_cons_region(&mut self) {
        let source = self.region_book.cons;
        let cur = self.jit.cons_cur.get();
        let lim = self.jit.cons_lim.get();
        self.jit.cons_cur.set(0);
        self.jit.cons_lim.set(0);
        self.region_book.cons = ConsRegionSource::Closed;
        if source == ConsRegionSource::Closed {
            debug_assert!(cur == 0 && lim == 0, "a closed region has no cursor");
            return;
        }
        debug_assert_eq!(
            self.region_book.cons_color,
            if self.allocates_black() {
                RegionColor::Black
            } else {
                RegionColor::White
            },
            "a cons region outlived the collector phase it was granted in"
        );
        let unused = (lim - cur) / size_of::<ConsCell>();
        if unused == 0 {
            return;
        }
        self.region_stats.cons_tail_closes += 1;
        self.refund_conses(unused);
        let first = cur as *mut ConsCell;
        let base = ConsBlock::block_base_for_ptr(first);
        let start = ConsBlock::index_of_ptr(first);
        if self.region_book.cons_color == RegionColor::Black {
            ConsBlock::mark_run_at(base, start, unused, false);
        }
        if let ConsRegionSource::BlockTail { block } = source
            && let Some(owner) = self.cons_blocks.get_mut(block)
            && owner.base_addr() == base
            && owner.next_index as usize == start + unused
        {
            owner.rewind_tail(start);
            return;
        }
        debug_assert!(
            source == ConsRegionSource::FreeListRun,
            "a block-tail region whose block moved or grew while it was open"
        );
        for i in (0..unused).rev() {
            // SAFETY: the cell is an unhanded cell of the region's owned
            // block; nothing else can reach it.
            let cell = unsafe { first.add(i) };
            unsafe { (*cell).set_free_next(self.cons_free_list) };
            self.cons_free_list = cell;
        }
    }

    /// Trace and reset the refill statistics (once per finished collection).
    pub(super) fn trace_region_stats(&mut self) {
        let stats = std::mem::take(&mut self.region_stats);
        if stats.cons_refills == 0 && stats.float_refills == 0 {
            return;
        }
        tracing::debug!(
            target: "neovm::gc::region",
            cons_refills = stats.cons_refills,
            cons_mean_run = if stats.cons_refills == 0 {
                0.0
            } else {
                stats.cons_cells_granted as f64 / stats.cons_refills as f64
            },
            cons_short_run_share = if stats.cons_refills == 0 {
                0.0
            } else {
                stats.cons_short_runs as f64 / stats.cons_refills as f64
            },
            cons_from_free_list = stats.cons_from_free_list,
            cons_from_block_tail = stats.cons_from_block_tail,
            cons_from_fresh_block = stats.cons_from_fresh_block,
            cons_tail_closes = stats.cons_tail_closes,
            float_refills = stats.float_refills,
            float_mean_run = if stats.float_refills == 0 {
                0.0
            } else {
                stats.float_slots_granted as f64 / stats.float_refills as f64
            },
            float_short_run_share = if stats.float_refills == 0 {
                0.0
            } else {
                stats.float_short_runs as f64 / stats.float_refills as f64
            },
            float_tail_closes = stats.float_tail_closes,
            "allocation regions since the last collection"
        );
    }

    /// Test hook: flip the deferred-sweep flag the way `incremental_finish`
    /// and `finish_incremental_sweep` do, closing the regions first.
    #[cfg(test)]
    pub(crate) fn set_sweep_in_progress_for_test(&mut self, on: bool) {
        self.close_alloc_regions();
        self.sweep_in_progress = on;
    }

    /// Test hook: the open cons region, `(cursor, limit)` (zeros when closed).
    #[cfg(test)]
    pub(crate) fn cons_region_for_test(&self) -> (usize, usize) {
        (self.jit.cons_cur.get(), self.jit.cons_lim.get())
    }

    /// Test hook: the open float region, `(cursor, limit)` (zeros when
    /// closed).
    #[cfg(test)]
    pub(crate) fn float_region_for_test(&self) -> (usize, usize) {
        (self.jit.float_cur.get(), self.jit.float_lim.get())
    }

    /// Test hook: the live-cons count, exact while a region is open.
    #[cfg(test)]
    pub(crate) fn cons_live_count_exact(&self) -> usize {
        self.cons_live_count - self.open_cons_unused()
    }
}
