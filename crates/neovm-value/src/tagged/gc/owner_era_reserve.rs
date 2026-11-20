//! One owner's era reserve, granted at an all-owner stop.
//!
//! The reserve is whole cons blocks, float pages, and bignum pages. The
//! owner carves the serial run size out of that reserve by writing only its
//! own cursors. The carve does not take the allocator mutex, does not call
//! `refill_cons_region`, and does not write `heap.jit`.
//!
//! Unused blocks and pages are returned at the next stop, before the
//! collector sweeps, and are not charged. Pacing charges a carved run once
//! and the close subtracts the unhanded tail, the same rule as the serial
//! region. A reserve past the per-owner cap is a miss, not a larger grant.
//!
//! The miss is one witness-taking refill poll. That poll is the
//! allocation-region refill named in E4-DESIGN §5, next to the other polls.
//! pentry owns the poll body. This module reports the miss and does not
//! take the witness.
//!
//! ```compile_fail,E0599
//! use neovm_value::tagged::gc::OwnerEraReserve;
//! let _ = OwnerEraReserve::from_raw_addresses(0usize, 1usize);
//! ```
//!
//! ```compile_fail,E0451
//! use neovm_value::tagged::gc::OwnerEraReserve;
//! let _ = OwnerEraReserve { cons: Vec::new() };
//! ```

use super::super::*;
use super::{CONS_REGION_MAX_CELLS, ConsRegionSource, FLOAT_REGION_MAX_SLOTS, FloatRegionSource};
use std::marker::PhantomData;
use std::mem::size_of;

/// Blocks or pages one owner may hold from a single stop.
///
/// The counted worker_compute pass grants 53 cons blocks. Sixty-four is the
/// bound above that pass, and the same bound applies to float and bignum
/// pages. Past the cap the grant is a reserve miss.
pub const OWNER_RESERVE_CAP: usize = 64;

/// Bignum cursor run. The reserve unit is the whole page (1024 slots).
const BIGNUM_REGION_MAX_SLOTS: usize = 64;

/// What the stop measured since the previous grant. Zero is the first
/// interval: the floor is one block and one page of each class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredInterval {
    pub conses: u64,
    pub float_slots: u64,
    pub bignum_slots: u64,
}

impl MeasuredInterval {
    pub const fn zero() -> Self {
        Self {
            conses: 0,
            float_slots: 0,
            bignum_slots: 0,
        }
    }
}

/// How many whole units a stop grants, and whether the cap clipped it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantOutcome {
    pub cons_blocks: usize,
    pub float_pages: usize,
    pub bignum_pages: usize,
    /// The 2× interval asked for more than [`OWNER_RESERVE_CAP`].
    pub reserve_miss: bool,
}

/// Warmup misses are reported. The counted sample is the next pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefillDisclosure {
    pub first_pass_misses: u64,
    pub counted_pass_misses: u64,
}

/// Why a grant or a carve refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReserveError {
    /// The owner cursor book is still open. Close it before the stop grants.
    RegionOpen,
    /// A previous reserve is still held. Return it before the sweep.
    ReserveHeld,
    /// The serial heap book is open. The stop closes that book first.
    SerialRegionOpen,
    /// The next run is not in this reserve. One witness-taking refill poll.
    ///
    /// The poll is pentry's allocation-region refill (E4-DESIGN §5). This
    /// arm does not take the witness and does not allocate.
    ReserveMiss,
}

/// §8.4b: misses on the warmup pass are reported and are not the sample.
pub fn disclose_refill_passes(
    first_pass_misses: u64,
    counted_pass_misses: u64,
) -> RefillDisclosure {
    RefillDisclosure {
        first_pass_misses,
        counted_pass_misses,
    }
}

/// Whole-block misses for `allocations` cells when `blocks_held` blocks are
/// already reserved. One miss is one poll for the next block.
pub fn reserve_block_misses(allocations: u64, cells_per_block: u64, blocks_held: u64) -> u64 {
    if cells_per_block == 0 {
        return 0;
    }
    allocations
        .div_ceil(cells_per_block)
        .saturating_sub(blocks_held)
}

/// Units to grant for a measured interval, before the cap.
pub fn uncapped_units(measured: u64, unit: u64) -> usize {
    if unit == 0 {
        return 0;
    }
    let cells = measured.saturating_mul(2).max(unit);
    cells.div_ceil(unit) as usize
}

struct ReservedCons {
    block_index: usize,
    base: usize,
    /// Address of cell 0 of this block.
    origin: usize,
    /// Exclusive end index. The block's `next_index` at grant.
    limit: usize,
    /// Next cell index the owner may carve.
    carve_at: usize,
}

struct OpenCons {
    slot: usize,
    start: usize,
    len: usize,
    taken: usize,
}

struct ReservedPage {
    page: usize,
    origin: usize,
    limit: usize,
    carve_at: usize,
    source: ArenaRunSource,
}

struct OpenPage {
    slot: usize,
    start: usize,
    len: usize,
}

/// Exclusive blocks and pages for one owner. Not a public address grant.
///
/// `OwnerEraReserve::new` is empty. Blocks enter only through
/// [`Self::grant_at_stop`]. Fields are private, so another crate cannot
/// build a grant from raw addresses.
pub struct OwnerEraReserve {
    cons: Vec<ReservedCons>,
    floats: Vec<ReservedPage>,
    bignums: Vec<ReservedPage>,
    open_cons: Option<OpenCons>,
    open_float: Option<OpenPage>,
    open_bignum: Option<OpenPage>,
    bignum_cur: usize,
    bignum_lim: usize,
    _thread: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(OwnerEraReserve: Send, Sync);

impl OwnerEraReserve {
    pub const fn new() -> Self {
        Self {
            cons: Vec::new(),
            floats: Vec::new(),
            bignums: Vec::new(),
            open_cons: None,
            open_float: None,
            open_bignum: None,
            bignum_cur: 0,
            bignum_lim: 0,
            _thread: PhantomData,
        }
    }

    pub fn cons_blocks_held(&self) -> usize {
        self.cons.len()
    }

    pub fn float_pages_held(&self) -> usize {
        self.floats.len()
    }

    pub fn bignum_pages_held(&self) -> usize {
        self.bignums.len()
    }

    pub fn holds_reserve(&self) -> bool {
        !self.cons.is_empty() || !self.floats.is_empty() || !self.bignums.is_empty()
    }

    /// Bytes of the open cons run not yet handed out.
    ///
    /// Pacing (`bytes_since_gc`) keeps the whole carved run. This is the
    /// tail the exact view subtracts, the same split as the serial region.
    pub fn open_cons_tail_bytes(&self) -> usize {
        self.open_cons
            .as_ref()
            .map(|open| open.len.saturating_sub(open.taken) * size_of::<ConsCell>())
            .unwrap_or(0)
    }

    /// The owner handed out `n` cons cells of the open run.
    pub fn note_cons_taken(&mut self, n: usize) {
        if let Some(open) = self.open_cons.as_mut() {
            open.taken = open.taken.saturating_add(n).min(open.len);
        }
    }

    /// Grant at the stop, after publication has closed the owner book and
    /// the previous reserve has been returned.
    pub fn grant_at_stop(
        &mut self,
        heap: &mut TaggedHeap,
        region_closed: bool,
        interval: MeasuredInterval,
    ) -> Result<GrantOutcome, ReserveError> {
        if !region_closed {
            return Err(ReserveError::RegionOpen);
        }
        if heap.alloc_regions_open() {
            return Err(ReserveError::SerialRegionOpen);
        }
        if self.holds_reserve() {
            return Err(ReserveError::ReserveHeld);
        }
        let (cons_blocks, cons_miss) =
            capped(uncapped_units(interval.conses, CONS_BLOCK_SIZE as u64));
        let float_slots = ObjectPage::<FloatObj>::SLOTS as u64;
        let bignum_slots = ObjectPage::<BignumObj>::SLOTS as u64;
        let (float_pages, float_miss) = capped(uncapped_units(interval.float_slots, float_slots));
        let (bignum_pages, bignum_miss) =
            capped(uncapped_units(interval.bignum_slots, bignum_slots));
        for _ in 0..cons_blocks {
            self.cons.push(mint_cons_block(heap));
        }
        for _ in 0..float_pages {
            self.floats.push(mint_page(heap, PageClass::Float));
        }
        for _ in 0..bignum_pages {
            self.bignums.push(mint_page(heap, PageClass::Bignum));
        }
        Ok(GrantOutcome {
            cons_blocks,
            float_pages,
            bignum_pages,
            reserve_miss: cons_miss || float_miss || bignum_miss,
        })
    }

    /// Return every uncarved block and page. Call this before the sweep.
    ///
    /// Carved cells that were handed out stay allocated. Cells never carved,
    /// and the unhanded tail of a closed run, are rewound when the block's
    /// `next_index` is still the grant end. A moved tail is free-listed
    /// instead, the same check as `close_cons_region`.
    pub fn return_unused_before_sweep(
        &mut self,
        heap: &mut TaggedHeap,
    ) -> Result<(), ReserveError> {
        if self.open_cons.is_some() || self.open_float.is_some() || self.open_bignum.is_some() {
            return Err(ReserveError::RegionOpen);
        }
        let cons = std::mem::take(&mut self.cons);
        for block in cons {
            return_cons_block(heap, &block);
        }
        let floats = std::mem::take(&mut self.floats);
        for page in floats {
            return_page(&mut heap.float_arena, &page);
        }
        let bignums = std::mem::take(&mut self.bignums);
        for page in bignums {
            return_page(&mut heap.bignum_arena, &page);
        }
        self.bignum_cur = 0;
        self.bignum_lim = 0;
        Ok(())
    }

    /// Carve the next cons run from this reserve into `region`.
    pub fn carve_cons(
        &mut self,
        heap: &mut TaggedHeap,
        region: &mut OwnerAllocRegion,
    ) -> Result<(), ReserveError> {
        if !region_cons_closed(region) {
            return Err(ReserveError::RegionOpen);
        }
        let want = heap.region_budget(size_of::<ConsCell>(), CONS_REGION_MAX_CELLS);
        let Some((slot, addr, n, block_index)) = self.take_cons_run(want) else {
            return Err(ReserveError::ReserveMiss);
        };
        let start = self.cons[slot].carve_at - n;
        region.install_granted_cons(addr, n, ConsRegionSource::BlockTail { block: block_index });
        heap.charge_conses(n);
        self.open_cons = Some(OpenCons {
            slot,
            start,
            len: n,
            taken: 0,
        });
        Ok(())
    }

    /// Carve the next float run from this reserve into `region`.
    pub fn carve_float(
        &mut self,
        heap: &mut TaggedHeap,
        region: &mut OwnerAllocRegion,
    ) -> Result<(), ReserveError> {
        if !region_float_closed(region) {
            return Err(ReserveError::RegionOpen);
        }
        let want = heap.region_budget(size_of::<FloatObj>(), FLOAT_REGION_MAX_SLOTS);
        let Some((slot, addr, n)) = self.take_page_run(PageClass::Float, want, FLOAT_SLOT_BYTES)
        else {
            return Err(ReserveError::ReserveMiss);
        };
        let page = self.floats[slot].page;
        let source = self.floats[slot].source;
        let start = self.floats[slot].carve_at - n;
        write_float_headers(heap, addr, n);
        region.install_granted_float(addr, n, FloatRegionSource::Run { page, source });
        heap.charge_floats(n);
        self.open_float = Some(OpenPage {
            slot,
            start,
            len: n,
        });
        Ok(())
    }

    /// Carve the next bignum run into this reserve's own cursor.
    ///
    /// Does not call `alloc_bignum_inline`. A page miss is [`ReserveError::ReserveMiss`].
    pub fn carve_bignum(&mut self, heap: &mut TaggedHeap) -> Result<(), ReserveError> {
        if self.bignum_cur < self.bignum_lim {
            return Err(ReserveError::RegionOpen);
        }
        let want = heap.region_budget(size_of::<BignumObj>(), BIGNUM_REGION_MAX_SLOTS);
        let stride = bignum_stride();
        let Some((slot, addr, n)) = self.take_page_run(PageClass::Bignum, want, stride) else {
            return Err(ReserveError::ReserveMiss);
        };
        let start = self.bignums[slot].carve_at - n;
        self.bignum_cur = addr;
        self.bignum_lim = addr + n * stride;
        heap.charge_bignum_slots(n);
        self.open_bignum = Some(OpenPage {
            slot,
            start,
            len: n,
        });
        Ok(())
    }

    /// The next bignum slot in the carved run, or `None` when it is exhausted.
    pub fn take_bignum_slot(&mut self) -> Option<usize> {
        if self.bignum_cur < self.bignum_lim {
            let here = self.bignum_cur;
            self.bignum_cur = here + bignum_stride();
            Some(here)
        } else {
            None
        }
    }

    /// Close the owner books, refund unhanded carved tails, and rewind the
    /// local carve cursors. Does not return the rest of the reserve; that
    /// is [`Self::return_unused_before_sweep`].
    pub fn close_for_stop(
        &mut self,
        heap: &mut TaggedHeap,
        region: &mut OwnerAllocRegion,
    ) -> OwnerRegionRefund {
        let refund = region.close_for_stop();
        self.apply_cons_refund(heap, refund.cons);
        self.apply_float_refund(heap, refund.float);
        self.apply_bignum_refund(heap);
        refund
    }

    fn take_cons_run(&mut self, want: usize) -> Option<(usize, usize, usize, usize)> {
        let slot = self
            .cons
            .iter()
            .position(|block| block.carve_at < block.limit)?;
        let block = &mut self.cons[slot];
        let n = want.min(block.limit - block.carve_at);
        if n == 0 {
            return None;
        }
        let addr = block.origin + block.carve_at * size_of::<ConsCell>();
        let block_index = block.block_index;
        block.carve_at += n;
        Some((slot, addr, n, block_index))
    }

    fn take_page_run(
        &mut self,
        class: PageClass,
        want: usize,
        stride: usize,
    ) -> Option<(usize, usize, usize)> {
        let pages = match class {
            PageClass::Float => &mut self.floats,
            PageClass::Bignum => &mut self.bignums,
        };
        let slot = pages.iter().position(|page| page.carve_at < page.limit)?;
        let page = &mut pages[slot];
        let n = want.min(page.limit - page.carve_at);
        if n == 0 {
            return None;
        }
        let addr = page.origin + page.carve_at * stride;
        page.carve_at += n;
        Some((slot, addr, n))
    }

    fn apply_cons_refund(&mut self, heap: &mut TaggedHeap, refund: ConsTailRefund) {
        let Some(open) = self.open_cons.take() else {
            return;
        };
        let unused = match refund {
            ConsTailRefund::None => 0,
            ConsTailRefund::Rewind { cells, block, .. } => {
                debug_assert_eq!(block, self.cons[open.slot].block_index);
                cells
            }
            ConsTailRefund::FreeList { cells, .. } => cells,
        };
        let unused = unused.min(open.len);
        if unused > 0 {
            heap.refund_conses(unused);
        }
        let handed = open.len - unused;
        self.cons[open.slot].carve_at = open.start + handed;
    }

    fn apply_float_refund(&mut self, heap: &mut TaggedHeap, refund: FloatTailRefund) {
        let Some(open) = self.open_float.take() else {
            return;
        };
        let unused = match refund {
            FloatTailRefund::None => 0,
            FloatTailRefund::PageTail { slots, .. } | FloatTailRefund::PageFree { slots, .. } => {
                slots
            }
        };
        let unused = unused.min(open.len);
        if unused > 0 {
            heap.refund_floats(unused);
        }
        self.floats[open.slot].carve_at = open.start + (open.len - unused);
    }

    fn apply_bignum_refund(&mut self, heap: &mut TaggedHeap) {
        let Some(open) = self.open_bignum.take() else {
            return;
        };
        let unused = if self.bignum_lim > self.bignum_cur {
            (self.bignum_lim - self.bignum_cur) / bignum_stride()
        } else {
            0
        };
        let unused = unused.min(open.len);
        if unused > 0 {
            heap.refund_bignum_slots(unused);
        }
        self.bignums[open.slot].carve_at = open.start + (open.len - unused);
        self.bignum_cur = 0;
        self.bignum_lim = 0;
    }
}

fn capped(wanted: usize) -> (usize, bool) {
    if wanted > OWNER_RESERVE_CAP {
        (OWNER_RESERVE_CAP, true)
    } else {
        (wanted, false)
    }
}

fn region_cons_closed(region: &OwnerAllocRegion) -> bool {
    region.cons_book_closed()
}

fn region_float_closed(region: &OwnerAllocRegion) -> bool {
    region.float_book_closed()
}

fn bignum_stride() -> usize {
    <BignumObj as PagedObject>::SLOT_BYTES
}

enum PageClass {
    Float,
    Bignum,
}

fn mint_cons_block(heap: &mut TaggedHeap) -> ReservedCons {
    if let Some(block_index) = heap
        .cons_blocks
        .iter()
        .position(|block| block.next_index == 0)
    {
        let (first, n) = heap.cons_blocks[block_index].reserve_tail(CONS_BLOCK_SIZE);
        debug_assert_eq!((first, n), (0, CONS_BLOCK_SIZE));
        let base = heap.cons_blocks[block_index].base_addr();
        let origin = heap.cons_blocks[block_index].cells_ptr() as usize;
        note_cons_block(heap, origin);
        return ReservedCons {
            block_index,
            base,
            origin,
            limit: n,
            carve_at: 0,
        };
    }
    let (cell, n, source) = heap.reserve_cons_run_in_fresh_block(CONS_BLOCK_SIZE);
    let ConsRegionSource::BlockTail { block: block_index } = source else {
        unreachable!("a fresh cons block is a block tail");
    };
    debug_assert_eq!(n, CONS_BLOCK_SIZE);
    let origin = cell as usize;
    let base = heap.cons_blocks[block_index].base_addr();
    note_cons_block(heap, origin);
    ReservedCons {
        block_index,
        base,
        origin,
        limit: n,
        carve_at: 0,
    }
}

fn note_cons_block(heap: &mut TaggedHeap, origin: usize) {
    if !heap.generational.enabled {
        return;
    }
    let identity = heap.heap_identity();
    let base = ConsBlockBase::of_cell(origin as *mut ConsCell);
    let recorded = match heap._satb_layout_reserve.current_young_log_mut() {
        Ok(log) if log.id().heap_identity() != identity => {
            Err(young_registry::YoungRegistryError::WrongHeap)
        }
        Ok(log) => heap.cons_allocator.note_carve(base, log).map(|_| ()),
        Err(error) => Err(error),
    };
    if let Err(error) = recorded {
        if let Some(block) = heap.cons_blocks.iter_mut().find(|block| {
            block.base_addr() == ConsBlock::block_base_for_ptr(origin as *const ConsCell)
        }) {
            block.rewind_tail(0);
        }
        panic!("canonical young recording refused an owner cons block: {error:?}");
    }
}

fn return_cons_block(heap: &mut TaggedHeap, reserved: &ReservedCons) {
    let unused = reserved.limit.saturating_sub(reserved.carve_at);
    if unused == 0 {
        return;
    }
    let block = &mut heap.cons_blocks[reserved.block_index];
    if block.base_addr() == reserved.base && block.next_index as usize == reserved.limit {
        block.rewind_tail(reserved.carve_at);
        return;
    }
    let first = (reserved.origin + reserved.carve_at * size_of::<ConsCell>()) as *mut ConsCell;
    // SAFETY: the uncarved range is this owner's exclusive grant in a live
    // block. Return runs before the sweep. The mutex serializes list writes
    // when the allocator is admitted; the sole owner holds the block.
    unsafe { heap.cons_allocator.return_unused(first, unused) };
}

fn mint_page(heap: &mut TaggedHeap, class: PageClass) -> ReservedPage {
    match class {
        PageClass::Float => mint_page_in(&mut heap.float_arena),
        PageClass::Bignum => mint_page_in(&mut heap.bignum_arena),
    }
}

fn mint_page_in<T: PagedObject>(arena: &mut ObjectArena<T>) -> ReservedPage {
    let slots = ObjectPage::<T>::SLOTS;
    if let Some(page_index) = arena
        .pages
        .iter()
        .position(|page| page.next_index == 0 && !page.retired)
    {
        let page = &mut arena.pages[page_index];
        page.next_index = slots;
        page.set_allocated_range(0, slots);
        let origin = page.slot_ptr(0) as usize;
        return ReservedPage {
            page: page_index,
            origin,
            limit: slots,
            carve_at: 0,
            source: ArenaRunSource::PageTail,
        };
    }
    let run = arena.reserve_run(slots);
    debug_assert_eq!(run.count, slots, "an owner page grant is a whole page");
    let origin = arena.pages[run.page].slot_ptr(run.first) as usize;
    ReservedPage {
        page: run.page,
        origin,
        limit: run.first + run.count,
        carve_at: run.first,
        source: run.source,
    }
}

fn return_page<T: PagedObject>(arena: &mut ObjectArena<T>, reserved: &ReservedPage) {
    let unused = reserved.limit.saturating_sub(reserved.carve_at);
    if unused == 0 {
        return;
    }
    arena.give_back_run(reserved.page, reserved.carve_at, unused, reserved.source);
}

fn write_float_headers(heap: &TaggedHeap, first: usize, slots: usize) {
    let parity = heap.mark_parity;
    for i in 0..slots {
        let slot = (first + i * FLOAT_SLOT_BYTES) as *mut FloatObj;
        // SAFETY: the slot is in a page this reserve just carved, and the
        // owner has not handed it out. The header is born at the current parity.
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
}

impl TaggedHeap {
    fn charge_bignum_slots(&mut self, n: usize) {
        self.current_mutator_gc_mut().allocated_count += n;
        self.note_allocation_bytes(n * size_of::<BignumObj>());
    }

    fn refund_bignum_slots(&mut self, n: usize) {
        self.current_mutator_gc_mut().allocated_count -= n;
        self.current_mutator_gc_mut().bytes_since_gc -= n * size_of::<BignumObj>();
    }

    #[cfg(test)]
    fn cons_block_next_index_for_test(&self, index: usize) -> u16 {
        self.cons_blocks[index].next_index
    }

    #[cfg(test)]
    fn cons_block_len_for_test(&self) -> usize {
        self.cons_blocks.len()
    }

    #[cfg(test)]
    fn young_block_len_for_test(&mut self) -> usize {
        self._satb_layout_reserve
            .current_young_log_mut()
            .map(|log| log.blocks().len())
            .unwrap_or(0)
    }

    #[cfg(test)]
    fn float_page_len_for_test(&self) -> usize {
        self.float_arena.pages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heap() -> TestHeap {
        TestHeap::new()
    }

    #[test]
    fn two_idle_stops_hold_no_reserve() {
        let mut heap = heap();
        heap.close_alloc_regions();
        let mut owner = OwnerEraReserve::new();
        let blocks_before = heap.cons_block_len_for_test();
        owner
            .grant_at_stop(&mut heap, true, MeasuredInterval::zero())
            .expect("floor grant");
        assert_eq!(owner.cons_blocks_held(), 1);
        assert_eq!(owner.float_pages_held(), 1);
        assert_eq!(owner.bignum_pages_held(), 1);
        assert_eq!(heap.bytes_since_gc(), 0);
        assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
        assert_eq!(heap.jit_float_window_for_test(), (0, 0));
        owner.return_unused_before_sweep(&mut heap).expect("return");
        assert!(!owner.holds_reserve());
        assert_eq!(heap.bytes_since_gc(), 0);
        owner
            .grant_at_stop(&mut heap, true, MeasuredInterval::zero())
            .expect("second floor grant");
        assert_eq!(owner.cons_blocks_held(), 1);
        assert_eq!(
            heap.cons_block_len_for_test(),
            blocks_before + 1,
            "the second idle grant reuses the returned block"
        );
        assert_eq!(heap.float_page_len_for_test(), 1);
        owner
            .return_unused_before_sweep(&mut heap)
            .expect("second return");
        assert!(!owner.holds_reserve());
        assert_eq!(heap.cons_block_next_index_for_test(0), 0);
    }

    #[test]
    fn pacing_charges_carved_cells_and_subtracts_the_open_tail() {
        let mut heap = heap();
        heap.close_alloc_regions();
        let mut owner = OwnerEraReserve::new();
        let mut region = OwnerAllocRegion::new();
        let young = heap.young_block_len_for_test();
        owner
            .grant_at_stop(&mut heap, true, MeasuredInterval::zero())
            .unwrap();
        assert_eq!(heap.bytes_since_gc(), 0, "a reserved block is not charged");
        assert_eq!(heap.young_block_len_for_test(), young + 1);
        owner.carve_cons(&mut heap, &mut region).unwrap();
        assert_eq!(heap.young_block_len_for_test(), young + 1);
        let cell = size_of::<ConsCell>();
        assert_eq!(heap.bytes_since_gc(), CONS_REGION_MAX_CELLS * cell);
        let exact_before_take = heap.bytes_since_gc() - owner.open_cons_tail_bytes();
        assert_eq!(exact_before_take, 0);
        for _ in 0..100 {
            region.take_cons_cell().expect("cell");
        }
        owner.note_cons_taken(100);
        let exact = heap.bytes_since_gc() - owner.open_cons_tail_bytes();
        assert_eq!(exact, 100 * cell);
        owner.close_for_stop(&mut heap, &mut region);
        assert_eq!(heap.bytes_since_gc(), 100 * cell);
        assert!(region.is_closed());
        owner.return_unused_before_sweep(&mut heap).unwrap();
        assert!(!owner.holds_reserve());
        assert_eq!(heap.cons_block_next_index_for_test(0), 100);
        assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
    }

    #[test]
    fn the_cap_is_a_reserve_miss() {
        let mut heap = heap();
        heap.close_alloc_regions();
        let mut owner = OwnerEraReserve::new();
        let wanted = uncapped_units(100_000, CONS_BLOCK_SIZE as u64);
        assert_eq!(wanted, 53);
        assert!(wanted <= OWNER_RESERVE_CAP);
        assert_eq!(
            reserve_block_misses(1_000_000, CONS_BLOCK_SIZE as u64, 1),
            263
        );
        assert_eq!(reserve_block_misses(100_000, CONS_BLOCK_SIZE as u64, 53), 0);
        let disclosure = disclose_refill_passes(26, 0);
        assert_eq!(disclosure.first_pass_misses, 26);
        assert_eq!(disclosure.counted_pass_misses, 0);
        assert_ne!(
            disclosure.counted_pass_misses,
            disclosure.first_pass_misses + disclosure.counted_pass_misses
        );
        let interval = MeasuredInterval {
            conses: (OWNER_RESERVE_CAP as u64 + 1) * CONS_BLOCK_SIZE as u64,
            float_slots: 0,
            bignum_slots: 0,
        };
        let outcome = owner.grant_at_stop(&mut heap, false, interval);
        assert_eq!(outcome, Err(ReserveError::RegionOpen));
        assert_eq!(heap.cons_block_len_for_test(), 0);
        let outcome = owner
            .grant_at_stop(&mut heap, true, interval)
            .expect("capped grant");
        assert!(outcome.reserve_miss);
        assert_eq!(outcome.cons_blocks, OWNER_RESERVE_CAP);
        assert_eq!(owner.cons_blocks_held(), OWNER_RESERVE_CAP);
        owner.return_unused_before_sweep(&mut heap).unwrap();
        assert!(!owner.holds_reserve());
    }

    #[test]
    fn two_owners_carve_disjoint_runs_without_moving_the_heap_window() {
        let mut heap = heap();
        heap.close_alloc_regions();
        let mut a = OwnerEraReserve::new();
        let mut b = OwnerEraReserve::new();
        let mut region_a = OwnerAllocRegion::new();
        let mut region_b = OwnerAllocRegion::new();
        a.grant_at_stop(&mut heap, true, MeasuredInterval::zero())
            .unwrap();
        b.grant_at_stop(&mut heap, true, MeasuredInterval::zero())
            .unwrap();
        a.carve_cons(&mut heap, &mut region_a).unwrap();
        b.carve_cons(&mut heap, &mut region_b).unwrap();
        let cell_a = region_a.take_cons_cell().unwrap();
        let cell_b = region_b.take_cons_cell().unwrap();
        assert!(cell_a.abs_diff(cell_b) >= CONS_BLOCK_BYTES);
        assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
        assert_eq!(heap.jit_float_window_for_test(), (0, 0));
        let mut exhausted = a;
        let mut region = region_a;
        // Drain the one block, then the next carve is a miss and allocates nothing.
        let blocks = heap.cons_block_len_for_test();
        loop {
            while region.take_cons_cell().is_some() {}
            exhausted.close_for_stop(&mut heap, &mut region);
            if exhausted.carve_cons(&mut heap, &mut region).is_err() {
                break;
            }
        }
        assert_eq!(heap.cons_block_len_for_test(), blocks);
        assert_eq!(
            exhausted.carve_cons(&mut heap, &mut region),
            Err(ReserveError::ReserveMiss)
        );
    }

    #[test]
    fn a_moved_block_tail_is_not_rewound() {
        let mut heap = heap();
        heap.close_alloc_regions();
        let mut owner = OwnerEraReserve::new();
        owner
            .grant_at_stop(&mut heap, true, MeasuredInterval::zero())
            .unwrap();
        // Another grant extended the tail past this reserve's limit.
        heap.cons_blocks[0].next_index = CONS_BLOCK_SIZE as u16;
        // Pretend the owner carved nothing, but the block tail moved by one
        // past the recorded limit so the rewind check fails closed.
        owner.cons[0].limit = CONS_BLOCK_SIZE - 1;
        owner.return_unused_before_sweep(&mut heap).unwrap();
        assert_eq!(
            heap.cons_block_next_index_for_test(0),
            CONS_BLOCK_SIZE as u16
        );
    }
}
