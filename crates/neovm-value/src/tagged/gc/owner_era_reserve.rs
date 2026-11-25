//! One owner's era reserve, granted at an all-owner stop.
//!
//! The reserve is whole cons blocks. Numeric allocation belongs solely to
//! the era numeric pending pool. The
//! owner carves the serial run size out of that reserve by writing only its
//! own cursors. The carve does not take the allocator mutex, does not call
//! `refill_cons_region`, and does not write `heap.jit`.
//!
//! Unused blocks are returned at the next stop, before the
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

use super::super::cons_allocator::{OwnerConsLease, OwnerConsRange};
use super::super::*;
use super::{CONS_REGION_MAX_CELLS, ConsRegionSource};
use crate::tagged::gc::mutator_registration::{AllStopped, RendezvousError};
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::Arc;

type ReservedCons = OwnerConsRange;

/// Cons blocks one owner may hold from a single stop.
///
/// The counted worker_compute pass grants 53 cons blocks. Sixty-four is the
/// bound above that pass. Past the cap the grant is a reserve miss.
pub const OWNER_RESERVE_CAP: usize = 64;

/// What the stop measured since the previous grant. Zero is the first
/// interval: the floor is one cons block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredInterval {
    pub conses: u64,
}

impl MeasuredInterval {
    pub const fn zero() -> Self {
        Self { conses: 0 }
    }
}

/// How many whole units a stop grants, and whether the cap clipped it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantOutcome {
    pub cons_blocks: usize,
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
    /// This reserve belongs to a different heap lifetime.
    WrongHeap,
    /// The capability does not name a currently valid all-owner stop.
    InvalidStop,
    /// The bounded book or heap obligation count cannot represent the grant.
    Capacity,
    /// Collector work or poisoned allocation metadata excludes a grant/return.
    CollectorBusy,
    /// A book cannot discharge more blocks than the heap still retains.
    InvalidReserve,
    /// This region does not contain this book's exact open grant.
    ForeignBook,
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

#[derive(Debug)]
struct OpenCons {
    slot: usize,
    start: usize,
    len: usize,
    taken: usize,
}

/// Exclusive cons blocks for one owner. Not a public address grant.
///
/// `OwnerEraReserve::new` is empty. Blocks enter only through
/// `grant_at_stop` in test-support builds. Fields are private, so another crate cannot
/// build a grant from raw addresses.
///
/// The ranges live in a lease the heap table also holds. Dropping a nonempty
/// book is loud in debug and does not discharge the lease: a stopped keeper
/// still returns it.
#[must_use = "return the cons reserve before the sweep, or the stopped keeper reclaims the lease"]
#[derive(Debug)]
pub struct OwnerEraReserve {
    heap: Option<HeapIdentity>,
    cons: Option<Arc<OwnerConsLease>>,
    open_cons: Option<OpenCons>,
    _thread: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(OwnerEraReserve: Send, Sync);

/// Owned transport of a closed cons book. Moving this value transfers the
/// whole reserve; it exposes neither a cursor nor a reference into the heap.
/// The active book stays confined to its worker. This closed form may travel
/// by value to the keeper while that worker publishes and parks.
#[must_use = "return the cons reserve before the sweep, or the stopped keeper reclaims the lease"]
#[derive(Debug)]
pub struct ClosedConsReserve {
    heap: Option<HeapIdentity>,
    cons: Option<Arc<OwnerConsLease>>,
}

static_assertions::assert_impl_all!(ClosedConsReserve: Send, Sync);
static_assertions::assert_not_impl_any!(ClosedConsReserve: Clone, Copy);

impl ClosedConsReserve {
    /// Resume exclusive cursor ownership after receiving the book by value.
    pub fn into_active(self) -> OwnerEraReserve {
        let mut carried = self;
        OwnerEraReserve {
            heap: carried.heap.take(),
            cons: carried.cons.take(),
            open_cons: None,
            _thread: PhantomData,
        }
    }
}

impl Drop for ClosedConsReserve {
    fn drop(&mut self) {
        panic_if_nonempty_book(self.cons.as_deref());
    }
}

impl OwnerEraReserve {
    pub const fn new() -> Self {
        Self {
            heap: None,
            cons: None,
            open_cons: None,
            _thread: PhantomData,
        }
    }

    /// Close the active ownership extent before handing the book to another
    /// thread. A refusal returns the original book without changing it.
    pub fn into_closed(self) -> Result<ClosedConsReserve, (ReserveError, Self)> {
        if self.open_cons.is_some() {
            return Err((ReserveError::RegionOpen, self));
        }
        let mut carried = self;
        Ok(ClosedConsReserve {
            heap: carried.heap.take(),
            cons: carried.cons.take(),
        })
    }

    pub fn cons_blocks_held(&self) -> usize {
        self.cons
            .as_ref()
            .map(|lease| lock_blocks(lease).len())
            .unwrap_or(0)
    }

    pub fn holds_reserve(&self) -> bool {
        self.cons_blocks_held() != 0
    }

    /// Block index of the first leased range. The session return empties the
    /// lease, so a post-state row records this before that return.
    #[cfg(any(test, feature = "test-support"))]
    pub(in crate::tagged::gc) fn first_leased_block_for_test(&self) -> usize {
        lock_blocks(self.cons.as_ref().expect("a lease is held"))[0].block_index
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
    #[cfg(any(test, feature = "test-support"))]
    pub fn grant_at_stop(
        &mut self,
        stopped: &AllStopped<'_>,
        heap: &mut TaggedHeap,
        region_closed: bool,
        interval: MeasuredInterval,
    ) -> Result<GrantOutcome, ReserveError> {
        self.grant_cons_at_stop(stopped, heap, region_closed, interval, None)
    }

    /// Same grant, recorded under `registration` so one retire can reclaim it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn grant_at_stop_for_registration(
        &mut self,
        stopped: &AllStopped<'_>,
        heap: &mut TaggedHeap,
        region_closed: bool,
        interval: MeasuredInterval,
        registration: std::num::NonZeroU64,
    ) -> Result<GrantOutcome, ReserveError> {
        self.grant_cons_at_stop(stopped, heap, region_closed, interval, Some(registration))
    }

    #[cfg(any(test, feature = "test-support"))]
    fn grant_cons_at_stop(
        &mut self,
        stopped: &AllStopped<'_>,
        heap: &mut TaggedHeap,
        region_closed: bool,
        interval: MeasuredInterval,
        registration: Option<std::num::NonZeroU64>,
    ) -> Result<GrantOutcome, ReserveError> {
        self.check_heap(heap)?;
        check_stop(stopped, heap)?;
        if !region_closed {
            return Err(ReserveError::RegionOpen);
        }
        if heap.alloc_regions_open() {
            return Err(ReserveError::SerialRegionOpen);
        }
        if self.holds_reserve() {
            return Err(ReserveError::ReserveHeld);
        }
        // Never add a lease while an existing collector could already read or
        // reclaim it. Other owners' stopped reserves may remain outstanding.
        if heap.concurrent_mark_running
            || heap.mark_in_progress
            || heap.sweep_in_progress
            || heap.generational.major_in_progress
            || !heap.gray_queue.is_empty()
            || heap.pending_obarray_scan.is_some()
            || heap.staged_mapped_cons_scan.is_some()
            || heap.staged_mapped_veclikes.is_some()
            || heap.cons_allocator.is_poisoned()
        {
            return Err(ReserveError::CollectorBusy);
        }
        let (cons_blocks, cons_miss) =
            capped(uncapped_units(interval.conses, CONS_BLOCK_SIZE as u64));
        heap.cons_allocator
            .owner_reserved_blocks()
            .checked_add(cons_blocks)
            .ok_or(ReserveError::Capacity)?;
        let lease = Arc::new(OwnerConsLease {
            owner_key: heap.cons_allocator.mint_owner_cons_key(),
            registration,
            open_run: std::sync::atomic::AtomicBool::new(false),
            blocks: std::sync::Mutex::new(Vec::new()),
        });
        lock_blocks(&lease)
            .try_reserve(cons_blocks)
            .map_err(|_| ReserveError::Capacity)?;
        if let Some(reg) = registration {
            stopped.note_owner_cons_lease_grant(reg, Arc::downgrade(&lease))?;
        }
        self.heap = Some(heap.heap_identity());
        self.cons = Some(Arc::clone(&lease));
        heap.cons_allocator
            .insert_owner_cons_lease(Arc::clone(&lease));
        for _ in 0..cons_blocks {
            let block = mint_cons_block(heap);
            // Capacity and counter overflow were checked before minting. Credit
            // each completed block before the infallible push publishes it.
            heap.cons_allocator.credit_owner_reserved_block();
            lock_blocks(&lease).push(block);
        }
        Ok(GrantOutcome {
            cons_blocks,
            reserve_miss: cons_miss,
        })
    }

    /// Return every uncarved block. Call this before the sweep.
    ///
    /// Carved cells that were handed out stay allocated. Cells never carved,
    /// and the unhanded tail of a closed run, are rewound when the block's
    /// `next_index` is still the grant end. A moved tail is free-listed
    /// instead, the same check as `close_cons_region`.
    pub fn return_unused_before_sweep(
        &mut self,
        stopped: &AllStopped<'_>,
        heap: &mut TaggedHeap,
    ) -> Result<(), ReserveError> {
        self.check_heap(heap)?;
        check_stop(stopped, heap)?;
        if self.open_cons.is_some() {
            return Err(ReserveError::RegionOpen);
        }
        if heap.cons_allocator.is_poisoned() {
            return Err(ReserveError::CollectorBusy);
        }
        let Some(lease) = self.cons.as_ref() else {
            return Ok(());
        };
        if lease.open_run.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ReserveError::RegionOpen);
        }
        let count = lock_blocks(lease).len();
        if count == 0 {
            // The keeper already returned this lease.
            return Ok(());
        }
        if heap.cons_allocator.owner_reserved_blocks() < count {
            return Err(ReserveError::InvalidReserve);
        }
        // An unexpected unwind loses the closed range book and keeps the heap
        // obligation, so neither a later sweep nor a repeated return can free
        // or return a partially discharged range twice.
        let cons = take_blocks(lease);
        let owner_key = lease.owner_key;
        #[cfg(any(test, feature = "test-support"))]
        let registration = lease.registration;
        for block in &cons {
            return_cons_block(heap, block);
        }
        heap.cons_allocator.return_owner_reserved_blocks(count);
        heap.cons_allocator.remove_owner_cons_lease(owner_key);
        #[cfg(any(test, feature = "test-support"))]
        if let Some(registration) = registration {
            stopped.note_owner_cons_lease_returned(registration, owner_key);
        }
        Ok(())
    }

    /// Carve the next cons run from this reserve into `region`.
    pub fn carve_cons(
        &mut self,
        heap: &mut TaggedHeap,
        region: &mut OwnerAllocRegion,
    ) -> Result<(), ReserveError> {
        self.check_heap(heap)?;
        if self.open_cons.is_some() || !region.is_closed() {
            return Err(ReserveError::RegionOpen);
        }
        let want = heap.region_budget(size_of::<ConsCell>(), CONS_REGION_MAX_CELLS);
        let Some((slot, addr, n, block_index, start)) = self.take_cons_run(want) else {
            return Err(ReserveError::ReserveMiss);
        };
        region.install_granted_cons(addr, n, ConsRegionSource::BlockTail { block: block_index });
        heap.charge_conses(n);
        if let Some(lease) = self.cons.as_ref() {
            lease
                .open_run
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.open_cons = Some(OpenCons {
            slot,
            start,
            len: n,
            taken: 0,
        });
        Ok(())
    }

    fn check_heap(&self, heap: &TaggedHeap) -> Result<(), ReserveError> {
        if self
            .heap
            .is_some_and(|expected| expected != heap.heap_identity())
        {
            return Err(ReserveError::WrongHeap);
        }
        Ok(())
    }

    /// Close the owner books, refund unhanded carved tails, and rewind the
    /// local carve cursors. Does not return the rest of the reserve; that
    /// is [`Self::return_unused_before_sweep`].
    pub fn close_for_stop(
        &mut self,
        heap: &mut TaggedHeap,
        region: &mut OwnerAllocRegion,
    ) -> Result<OwnerRegionRefund, ReserveError> {
        self.check_heap(heap)?;
        // Validate the source and complete carved range before closing either
        // cursor. A foreign or independently closed region must leave this
        // book's open record and the supplied region untouched.
        match self.open_cons.as_ref() {
            Some(open) => {
                let lease = self.cons.as_ref().ok_or(ReserveError::InvalidReserve)?;
                let blocks = lock_blocks(lease);
                let block = blocks.get(open.slot).ok_or(ReserveError::InvalidReserve)?;
                let first = block.origin + open.start * size_of::<ConsCell>();
                let block_index = block.block_index;
                let matched = region.matches_granted_cons(first, open.len, block_index);
                drop(blocks);
                if !matched {
                    return Err(ReserveError::ForeignBook);
                }
            }
            None if !region.is_closed() => return Err(ReserveError::ForeignBook),
            None => {}
        }
        let refund = region.close_for_stop();
        self.apply_cons_refund(heap, refund.cons);
        Ok(refund)
    }

    fn take_cons_run(&mut self, want: usize) -> Option<(usize, usize, usize, usize, usize)> {
        let lease = self.cons.as_ref()?;
        let mut blocks = lock_blocks(lease);
        let slot = blocks
            .iter()
            .position(|block| block.carve_at < block.limit)?;
        let block = &mut blocks[slot];
        let n = want.min(block.limit - block.carve_at);
        if n == 0 {
            return None;
        }
        let addr = block.origin + block.carve_at * size_of::<ConsCell>();
        let block_index = block.block_index;
        let start = block.carve_at;
        block.carve_at += n;
        Some((slot, addr, n, block_index, start))
    }

    fn apply_cons_refund(&mut self, heap: &mut TaggedHeap, refund: ConsTailRefund) {
        let Some(open) = self.open_cons.take() else {
            return;
        };
        let unused = match refund {
            ConsTailRefund::None => 0,
            ConsTailRefund::Rewind { cells, .. } => cells,
            ConsTailRefund::FreeList { .. } => {
                unreachable!("close preflight certifies this book's block-tail grant")
            }
        };
        let unused = unused.min(open.len);
        if unused > 0 {
            heap.refund_conses(unused);
        }
        let handed = open.len - unused;
        let Some(lease) = self.cons.as_ref() else {
            return;
        };
        lock_blocks(lease)[open.slot].carve_at = open.start + handed;
        lease
            .open_run
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

impl Drop for OwnerEraReserve {
    fn drop(&mut self) {
        panic_if_nonempty_book(self.cons.as_deref());
    }
}

fn lock_blocks(lease: &OwnerConsLease) -> std::sync::MutexGuard<'_, Vec<ReservedCons>> {
    lease
        .blocks
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn take_blocks(lease: &OwnerConsLease) -> Vec<ReservedCons> {
    std::mem::take(&mut *lock_blocks(lease))
}

/// Loud in debug when a nonempty book is dropped outside an unwind. The lease
/// stays in the heap table either way, so the keeper can still return it.
fn panic_if_nonempty_book(lease: Option<&OwnerConsLease>) {
    #[cfg(debug_assertions)]
    if lease.is_some_and(|lease| !lock_blocks(lease).is_empty()) && !std::thread::panicking() {
        panic!("dropped a nonempty owner cons reserve");
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = lease;
    }
}

#[cfg(any(test, feature = "test-support"))]
fn capped(wanted: usize) -> (usize, bool) {
    if wanted > OWNER_RESERVE_CAP {
        (OWNER_RESERVE_CAP, true)
    } else {
        (wanted, false)
    }
}

#[cfg(any(test, feature = "test-support"))]
fn mint_cons_block(heap: &mut TaggedHeap) -> ReservedCons {
    if let Some(block_index) = heap
        .cons_blocks
        .iter()
        .position(|block| block.next_index() == 0)
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

#[cfg(any(test, feature = "test-support"))]
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
    if block.base_addr() == reserved.base && block.next_index() == reserved.limit {
        block.rewind_tail(reserved.carve_at);
        return;
    }
    let first = (reserved.origin + reserved.carve_at * size_of::<ConsCell>()) as *mut ConsCell;
    // SAFETY: the uncarved range is this owner's exclusive grant in a live
    // block. Return runs before the sweep. The mutex serializes list writes
    // when the allocator is admitted; the sole owner holds the block.
    unsafe { heap.cons_allocator.return_unused(first, unused) };
}

fn check_stop(stopped: &AllStopped<'_>, heap: &TaggedHeap) -> Result<(), ReserveError> {
    stopped
        .check_world_roots(heap.heap_identity())
        .map_err(|error| match error {
            RendezvousError::Identity => ReserveError::WrongHeap,
            _ => ReserveError::InvalidStop,
        })
}

#[cfg(all(
    any(test, feature = "test-support"),
    not(feature = "gc-memory-telemetry")
))]
#[path = "owner_era_reserve/tests.rs"]
pub(in crate::tagged::gc) mod tests;

impl TaggedHeap {
    /// Cold lease predicate. The count lives outside the allocator mutex so
    /// Drop can retain backing storage without locking or relying on a book's
    /// destructor. A stopped return, including the keeper's lease-table walk,
    /// is what discharges whole blocks.
    pub(in crate::tagged::gc) fn has_owner_cons_reserves(&self) -> bool {
        self.cons_allocator.owner_reserved_blocks() != 0
    }

    /// Backstop for a direct collector entry. Those entries hold only
    /// `&mut TaggedHeap`, so they must not return a lease: the owner may
    /// still be handing the run out. The stop-session entry is
    /// `EraCore::collect_requested`. `gc_safe_point` does not use it: the
    /// safe point has no `AllStopped`, and `grant_at_stop` stays test-gated,
    /// so the serial heap's reserve count stays zero until a production
    /// grant exists.
    pub(in crate::tagged::gc) fn assert_no_owner_cons_reserves(&self) {
        assert!(
            !self.has_owner_cons_reserves(),
            "collection requires explicit return of every owner cons reserve"
        );
    }

    /// Return every leased range. Only the stopped session calls this.
    ///
    /// A poisoned allocator is permanent, so this leaves the count
    /// outstanding. A mid-return unwind can also leave a count after the
    /// lease table has been taken; the next stop walks an empty table and
    /// that count does not clear. The session reports `Publication` for
    /// either leftover. A forgotten book whose lease is still in the table
    /// is returned on the next stop while the allocator is healthy.
    /// This walk still returns every owner. One registration is reclaimed
    /// earlier by `reclaim_owner_cons_registration`, which `retire_rooted`
    /// queues and the keeper applies.
    pub(in crate::tagged::gc) fn return_leased_owner_cons_reserves(&mut self) {
        if !self.has_owner_cons_reserves() || self.cons_allocator.is_poisoned() {
            return;
        }
        let leases = self.cons_allocator.take_owner_cons_leases();
        let mut count = 0usize;
        for lease in &leases {
            let blocks = take_blocks(lease);
            count += blocks.len();
            for block in &blocks {
                return_cons_block(self, block);
            }
        }
        if count > 0 {
            self.cons_allocator.return_owner_reserved_blocks(count);
        }
    }

    /// Return one registration's closed lease and discharge its count.
    ///
    /// Other leases stay. A poisoned allocator changes nothing and returns
    /// false so the caller can keep the registration queued. `retire_rooted`
    /// refuses an open cons run with `RegistrationError::OpenRegion`, and
    /// `take_owner_cons_leases_for_registration` asserts that every matching
    /// lease is closed before taking or rewinding its blocks.
    #[cfg(any(test, feature = "test-support"))]
    pub(in crate::tagged::gc) fn reclaim_owner_cons_registration(
        &mut self,
        registration: std::num::NonZeroU64,
    ) -> bool {
        if self.cons_allocator.is_poisoned() {
            return false;
        }
        if !self.has_owner_cons_reserves() {
            return true;
        }
        let taken = self
            .cons_allocator
            .take_owner_cons_leases_for_registration(registration);
        let mut count = 0usize;
        for lease in &taken {
            let blocks = take_blocks(lease);
            count += blocks.len();
            for block in &blocks {
                return_cons_block(self, block);
            }
        }
        if count > 0 {
            self.cons_allocator.return_owner_reserved_blocks(count);
        }
        true
    }
}
