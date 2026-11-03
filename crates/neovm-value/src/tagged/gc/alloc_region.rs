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

use super::cons_allocator::ConsCarveAge;
use super::*;

const CONS_RECORDING_INACTIVE: usize = 2;
const CONS_RECORDING_MIXED: usize = 4;

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

/// The open regions' bookkeeping that only Rust reads. Keep the previous
/// footprint and cache a canonical recorded-block proof with the phase color.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct RegionBook {
    pub(super) cons: ConsRegionSource,
    /// Floats have no color: a slot is born at the current parity, which a
    /// region cannot outlive (it closes before every parity flip).
    pub(super) float: FloatRegionSource,
    /// Successfully recorded canonical block plus color in bit0 and a mixed
    /// old/young classification in bit2. Bit1 means inactive, even for base0.
    /// This is an allocation-epoch certificate, not a second young registry.
    /// Every collector entry closes it before changing ages/counts. Mixed
    /// blocks have an unaligned encoded base, so the store predicate below
    /// remains conservative without another hot-path branch.
    pub(super) cons_block_and_color: usize,
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(CONS_BLOCK_ALIGN == 1 << 16);
    assert!(std::mem::size_of::<RegionBook>() == 40);
    assert!(std::mem::align_of::<RegionBook>() == 8);
    assert!(std::mem::offset_of!(RegionBook, cons) == 0);
    assert!(std::mem::offset_of!(RegionBook, float) == 16);
    assert!(std::mem::offset_of!(RegionBook, cons_block_and_color) == 32);
};

impl RegionBook {
    pub(super) const fn new() -> Self {
        Self {
            cons: ConsRegionSource::Closed,
            float: FloatRegionSource::Closed,
            cons_block_and_color: CONS_RECORDING_INACTIVE,
        }
    }

    #[inline(always)]
    pub(super) fn no_old_cons_block_contains(&self, addr: usize) -> bool {
        (addr & !(CONS_BLOCK_ALIGN - 1)) == (self.cons_block_and_color & !1)
    }

    #[inline]
    pub(super) fn cons_color(&self) -> RegionColor {
        if self.cons_block_and_color & 1 == 0 {
            RegionColor::White
        } else {
            RegionColor::Black
        }
    }

    #[inline]
    pub(super) fn set_cons_block_and_color(
        &mut self,
        no_old_block: Option<usize>,
        color: RegionColor,
    ) {
        debug_assert!(no_old_block.is_none_or(|base| base & (CONS_BLOCK_ALIGN - 1) == 0));
        let color_bit = match color {
            RegionColor::White => 0,
            RegionColor::Black => 1,
        };
        self.cons_block_and_color = no_old_block.unwrap_or(CONS_RECORDING_INACTIVE) | color_bit;
    }

    #[inline]
    pub(super) fn clear_cons_block(&mut self) {
        self.cons_block_and_color = (self.cons_block_and_color & 1) | CONS_RECORDING_INACTIVE;
    }

    #[inline]
    fn recorded_cons_age(word: usize, base: usize) -> Option<ConsCarveAge> {
        if word & CONS_RECORDING_INACTIVE != 0 || word & !(CONS_BLOCK_ALIGN - 1) != base {
            return None;
        }
        Some(if word & CONS_RECORDING_MIXED == 0 {
            ConsCarveAge::NoOldOwners
        } else {
            ConsCarveAge::MayHaveOldOwners
        })
    }

    #[inline]
    fn set_cons_recording_and_color(
        &mut self,
        recorded: Option<(usize, ConsCarveAge)>,
        color: RegionColor,
    ) {
        self.set_cons_block_and_color(recorded.map(|(base, _)| base), color);
        if let Some((_, ConsCarveAge::MayHaveOldOwners)) = recorded {
            self.cons_block_and_color |= CONS_RECORDING_MIXED;
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
        // Only this non-safepoint refill may carry a certificate over its own
        // close. Collector/admission closes leave no certificate to copy.
        // A reserved run stays registered/retained throughout this extent;
        // other mutators cannot reset canonical logs/ages before stop-all.
        let previous_recording = self.region_book.cons_block_and_color;
        self.close_cons_region();
        let want = self.region_budget(size_of::<ConsCell>(), CONS_REGION_MAX_CELLS);
        let (first, n, source) = self.reserve_cons_run(want);
        debug_assert!(n >= 1 && n <= want);
        let recorded_block = if self.generational.enabled {
            // All three sources prove retained, registered storage before
            // returning a run. Record it before initializing/publishing cells,
            // unless this same epoch already recorded the identical block.
            let base = ConsBlockBase::of_cell(first);
            let identity = self.heap_identity();
            #[cfg(test)]
            let can_cache = !cons_allocator::carve_fault_active();
            #[cfg(not(test))]
            let can_cache = true;
            let recorded = match self._satb_layout_reserve.current_young_log_mut() {
                Ok(log) if log.id().heap_identity() != identity => {
                    Err(young_registry::YoungRegistryError::WrongHeap)
                }
                Ok(log) => {
                    // Even a hit must borrow the CURRENT resident, active log
                    // and validate its domain before publishing a grant. The
                    // canonical entry may belong to another mutator's log.
                    if can_cache
                        && let Some(age) =
                            RegionBook::recorded_cons_age(previous_recording, base.addr())
                    {
                        Ok(age)
                    } else {
                        self.cons_allocator.note_carve(base, log)
                    }
                }
                Err(error) => Err(error),
            };
            match recorded {
                // Faults must still exercise the real canonical path on the
                // next grant; SkipCarveRecording is not a successful record.
                Ok(age) => can_cache.then_some((base.addr(), age)),
                Err(error) => {
                    // No cell was initialized, marked, charged or handed out.
                    // SAFETY: reserve_cons_run gave us this entire exclusive,
                    // unmarked/young range in a still-owned registered block.
                    // Returning it to the private free list also handles a fresh
                    // or bump-tail grant without leaving untracked used cells.
                    unsafe { self.cons_allocator.return_unused(first, n) };
                    panic!("canonical young recording refused an unhanded cons grant: {error:?}");
                }
            }
        } else {
            None
        };
        let color = if self.allocates_black() {
            let base = ConsBlock::block_base_for_ptr(first);
            ConsBlock::mark_run_at(base, ConsBlock::index_of_ptr(first), n, true);
            self.note_black_cons_region_start(first);
            RegionColor::Black
        } else {
            RegionColor::White
        };
        self.charge_conses(n);
        self.region_book.cons = source;
        let stats = &mut self.region_stats;
        stats.cons_refills += 1;
        stats.cons_cells_granted += n as u64;
        stats.cons_short_runs += u64::from(n < 8);
        let first_addr = first as usize;
        self.region_book
            .set_cons_recording_and_color(recorded_block, color);
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
        // SAFETY: this heap retains every registered block and owns all
        // allocation regions. Sole access or the admitted-mode mutex reserves
        // an exclusive run for us.
        if let Some((head, n)) = unsafe { self.cons_allocator.reserve_run(want) } {
            self.region_stats.cons_from_free_list += 1;
            return (head, n, ConsRegionSource::FreeListRun);
        }
        if let Some(block) = self.cons_blocks.len().checked_sub(1)
            && self
                .cons_allocator
                .is_ready(ConsBlockBase::of(&self.cons_blocks[block]))
        {
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
        self.cons_allocator.register(&block);
        self.cons_blocks.push(block);
        let block_index = self.cons_blocks.len() - 1;
        self.cons_block_index_by_base
            .insert(block_base, block_index);
        if self.generational.enabled
            && let Some(slots) = self.card_slots_mut()
        {
            slots.insert_block(block_base);
        }
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
        let remaining = self.gc_threshold.saturating_sub(self.bytes_since_gc());
        if remaining == 0 {
            max
        } else {
            ((remaining - 1) / size).clamp(1, max)
        }
    }

    /// Charge a granted region of `n` conses to every consing counter.
    fn charge_conses(&mut self, n: usize) {
        self.add_memory_use_count(MemoryUseCountSlot::ConsCells, n as u64);
        self.current_mutator_gc_mut().allocated_count += n;
        self.cons_live_count += n;
        self.note_allocation_bytes(n * size_of::<ConsCell>());
    }

    /// Refund `m` never-handed-out conses of a closing region.
    fn refund_conses(&mut self, m: usize) {
        let index = MemoryUseCountSlot::ConsCells.index();
        self.current_mutator_gc_mut().memory_use_counts[index] =
            self.current_mutator_gc().memory_use_counts[index].wrapping_sub(m as u64);
        self.current_mutator_gc_mut().allocated_count -= m;
        self.cons_live_count -= m;
        self.current_mutator_gc_mut().bytes_since_gc -= m * size_of::<ConsCell>();
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
        if self.generational.enabled {
            self.note_black_float_region_start(first);
        }
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
        self.current_mutator_gc_mut().allocated_count += n;
        self.note_allocation_bytes(n * size_of::<FloatObj>());
    }

    /// Refund `m` never-handed-out floats of a closing region.
    fn refund_floats(&mut self, m: usize) {
        let index = MemoryUseCountSlot::Floats.index();
        self.current_mutator_gc_mut().memory_use_counts[index] =
            self.current_mutator_gc().memory_use_counts[index].wrapping_sub(m as u64);
        self.current_mutator_gc_mut().allocated_count -= m;
        self.current_mutator_gc_mut().bytes_since_gc -= m * size_of::<FloatObj>();
    }

    /// Close the float region: refund its unhanded slots and give them back
    /// to their page (`ObjectArena::give_back_run`).
    #[inline(never)]
    fn close_float_region(&mut self) {
        let book = self.region_book.float;
        let cur = self.jit.float_cur.get();
        let lim = self.jit.float_lim.get();
        if self.generational.enabled {
            self.close_black_float_region(cur);
        }
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
    /// linked back to that block's free list, in ascending order within the
    /// returned range, with every cell's car `DEAD` again. Other mutators may
    /// return separate runs in any order; reservation simply stops at a gap.
    #[inline(never)]
    fn close_cons_region(&mut self) {
        let source = self.region_book.cons;
        let cur = self.jit.cons_cur.get();
        let lim = self.jit.cons_lim.get();
        self.close_black_cons_region(cur);
        self.jit.cons_cur.set(0);
        self.jit.cons_lim.set(0);
        self.region_book.clear_cons_block();
        self.region_book.cons = ConsRegionSource::Closed;
        if source == ConsRegionSource::Closed {
            debug_assert!(cur == 0 && lim == 0, "a closed region has no cursor");
            return;
        }
        debug_assert_eq!(
            self.region_book.cons_color(),
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
        if self.region_book.cons_color() == RegionColor::Black {
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
        // Another mutator may have extended a bump tail before its owner
        // closed it. Such a range returns to this block's list too.
        // SAFETY: these are our unhanded, unmarked cells in a live block;
        // region close precedes sweep, and the mutex serializes list writes.
        unsafe { self.cons_allocator.return_unused(first, unused) };
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

#[cfg(test)]
#[path = "tests/cons_region_recording_cache_test.rs"]
mod recording_cache_tests;
