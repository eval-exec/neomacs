//! Exact local observation identities and the collector's conservative gate.
//!
//! This lives inside the existing mutator journal State, never in a Context
//! or a new TLS identity cache. Certificates and its ledger remain !Send;
//! only the executing mutator changes them. Shared permanent mark metadata
//! and the reclamation epoch are atomic. No old owner header is dereferenced.

use super::super::gc::{
    BarrierWindow, CompiledObservationGate, collection_observation_epoch,
    collection_observed_metadata, reclaimed_collection_observation_count,
};
use super::super::header::ConsCell;
use super::super::value::{TAG_CONS, TAG_MASK, TAG_VECLIKE};
use super::NativeWindowCollectionOwner;
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;
use std::mem::size_of;

const EMPTY_BOUNDS: (usize, usize) = (usize::MAX, 0);
// Prune once the marks cleared since the last validation reach this fraction
// of the ledger. Each O(ledger) scan is then paid for by at least
// len / PRUNE_DEBT_RATIO real clears, and at most that many entries are dead.
const PRUNE_DEBT_RATIO: u64 = 4;

pub(super) struct ObservedEnvelope {
    // Complete lifetime publication ledger, including non-inline subtypes.
    // Between prunes it may also hold identities whose marks were cleared;
    // membership checks consult exact metadata before trusting an entry.
    owners: FxHashSet<usize>,
    // Cached native-store eligibility from live publication. A Buffer/Window/
    // Frame/Bytecode/String owner is still marked/replay-qualified in `owners`,
    // but cannot widen a native write gate or bound its empty intervals.
    addresses: SortedAddressLedger,
    // Vector/Record eligibility is also cached at live publication. Native
    // slot stores use these bounds independently of observed Cons addresses.
    slot_addresses: SortedAddressLedger,
    epoch: Option<u64>,
    // Global cleared-mark count acquired before the last full validation.
    validated_reclaimed: u64,
    dump: BarrierWindow,
    full: (usize, usize),
    below: (usize, usize),
    above: (usize, usize),
    slot_below: (usize, usize),
    slot_above: (usize, usize),
    // Protocol bounds for a proven empty interval, never an owner hint or a
    // Lisp-state cache. Only this mutator's observed identities constrain it.
    excluded: Option<BarrierWindow>,
    // The last cons refusal within that proven gap. This mutator-local raw
    // address is neither rooted nor dereferenced. A disjoint replacement can
    // retain it as the exact exemption without changing native protocol data.
    excluded_cons: usize,
    // An exact protocol exemption survives replacement of the generic gap.
    // It is never rooted or dereferenced. Observation of this address closes
    // it before publishing a mark or taking the read's revision snapshot.
    single_cons: usize,
    #[cfg(any(test, feature = "test-support"))]
    geometry_rebuilds: usize,
    #[cfg(any(test, feature = "test-support"))]
    validated_identities: usize,
}

/// Frozen size of the envelope as it was when the thread-local `StateSlot`
/// reserve was pinned (two ordered address sets). The reserve is sized from
/// this layout, never from the live envelope: the envelope itself lives in
/// the heap-side `State`, so growing it must not grow the per-thread TLS block
/// and shift every `%fs` offset in the binary.
#[allow(dead_code)]
pub(super) struct LegacyEnvelopeLayout {
    owners: FxHashSet<usize>,
    addresses: BTreeSet<usize>,
    slot_addresses: BTreeSet<usize>,
    epoch: Option<u64>,
    dump: BarrierWindow,
    full: (usize, usize),
    below: (usize, usize),
    above: (usize, usize),
    slot_below: (usize, usize),
    slot_above: (usize, usize),
    excluded: Option<BarrierWindow>,
    excluded_cons: usize,
    single_cons: usize,
    #[cfg(any(test, feature = "test-support"))]
    geometry_rebuilds: usize,
}

impl ObservedEnvelope {
    /// Heap reinstallation can change the native dump/owner protocol between
    /// uses of a learned negative proof. Forget only those optional rejects;
    /// the World's positive ledger and capture dependencies remain intact.
    pub(super) fn discard_exclusions(&mut self) {
        self.excluded = None;
        self.excluded_cons = 0;
        self.single_cons = 0;
    }

    pub(super) fn new() -> Self {
        Self {
            owners: FxHashSet::default(),
            addresses: SortedAddressLedger::new(),
            slot_addresses: SortedAddressLedger::new(),
            epoch: Some(collection_observation_epoch()),
            validated_reclaimed: reclaimed_collection_observation_count(),
            dump: BarrierWindow::NONE,
            full: EMPTY_BOUNDS,
            below: EMPTY_BOUNDS,
            above: EMPTY_BOUNDS,
            slot_below: EMPTY_BOUNDS,
            slot_above: EMPTY_BOUNDS,
            excluded: None,
            excluded_cons: 0,
            single_cons: 0,
            #[cfg(any(test, feature = "test-support"))]
            geometry_rebuilds: 0,
            #[cfg(any(test, feature = "test-support"))]
            validated_identities: 0,
        }
    }

    /// Refresh once per reclamation epoch or current dump-span change. An
    /// observer never initializes this ledger merely to reject an empty gate.
    pub(super) fn refresh(&mut self, dump: BarrierWindow) -> bool {
        self.refresh_inner(dump, || ())
    }

    #[cfg(test)]
    pub(super) fn refresh_with_concurrent_reclaim_for_test(
        &mut self,
        dump: BarrierWindow,
        after_reclaimed: impl FnOnce(),
    ) -> bool {
        self.refresh_inner(dump, after_reclaimed)
    }

    #[inline(always)]
    fn refresh_inner(&mut self, dump: BarrierWindow, after_reclaimed: impl FnOnce()) -> bool {
        let epoch = collection_observation_epoch();
        if self.epoch == Some(epoch) {
            if self.dump == dump {
                return false;
            }
            self.rebuild(dump);
            return true;
        }
        self.refresh_reclaimed_inner(dump, epoch, after_reclaimed)
    }

    /// Acknowledge a reclamation epoch. Every sweep slice that clears a mark
    /// advances the epoch, so validating the whole ledger here would make an
    /// incremental sweep cost O(slices * ledger). Instead prune only once the
    /// marks cleared since the last validation reach a fixed fraction of the
    /// ledger: each scan is charged to that many real clears, and the dead
    /// entries it can retain meanwhile are bounded by the same fraction.
    /// Retained dead entries are harmless: they only widen the gate, and
    /// `already_published` and the publishing caller of `insert` check exact
    /// metadata before trusting one.
    ///
    /// Ordering proof (pairs with `advance_collection_observation_epoch_by`):
    /// Writer executes `(W0) clear mark [Release] -> (W1) RECLAIMED_OBSERVATIONS
    /// += cleared [Release] -> (W2) OBSERVATION_EPOCH += 1 [Release]`.
    /// Both branches below Acquire `OBSERVATION_EPOCH` BEFORE
    /// `RECLAIMED_OBSERVATIONS`:
    /// - Deferred prune: caller Acquired `epoch` before this function Acquires
    ///   `reclaimed`. Every `(W2)` with side effect `<= epoch` synchronizes-with
    ///   `epoch` via the RMW release sequence on `OBSERVATION_EPOCH`, so its
    ///   `(W1)` happened-before `reclaimed`. Storing `self.epoch = Some(epoch)`
    ///   (never re-loading a later epoch after `reclaimed`) ensures `self.epoch`
    ///   never advances past a concurrent clear unseen by `reclaimed`.
    /// - Full prune: Acquires `epoch` first, then `reclaimed`, then scans
    ///   `retain()`. Every clear included in `epoch` is counted in `reclaimed`,
    ///   and every clear counted in `reclaimed` synchronizes-with `reclaimed`
    ///   and is visible to `retain()`.
    #[cold]
    #[inline(never)]
    fn refresh_reclaimed_inner(
        &mut self,
        dump: BarrierWindow,
        epoch: u64,
        after_reclaimed: impl FnOnce(),
    ) -> bool {
        let reclaimed = reclaimed_collection_observation_count();
        after_reclaimed();
        let debt = reclaimed.wrapping_sub(self.validated_reclaimed);
        if debt.saturating_mul(PRUNE_DEBT_RATIO) < self.owners.len() as u64 {
            self.epoch = Some(epoch);
            if self.dump != dump {
                self.rebuild(dump);
                return true;
            }
            return false;
        }
        // Acquire epoch BEFORE reclaimed count, and both before scanning:
        // every clear up to `epoch` is counted in `reclaimed`, and every clear
        // counted in `reclaimed` is visible to `retain()` below.
        let epoch = collection_observation_epoch();
        let reclaimed = reclaimed_collection_observation_count();
        let old_len = self.owners.len();
        #[cfg(any(test, feature = "test-support"))]
        {
            self.validated_identities += old_len;
        }
        self.owners
            .retain(|&bits| collection_observed_metadata(bits));
        self.validated_reclaimed = reclaimed;
        self.epoch = Some(epoch);
        // Other heaps share the reclamation epoch. Keep the ordered buffers
        // and bounds when none of this ledger's identities died; an unrelated
        // sweep must not rebuild this ledger.
        if self.owners.len() != old_len || self.dump != dump {
            self.rebuild(dump);
            return true;
        }
        false
    }

    /// Insert even when another mutator already set the process-wide mark.
    /// Native-store-eligible observations update the retained sorted address buffers;
    /// only reclamation or a dump-span change scans all local identities.
    /// A newly observed owner closes any gap covering it before the caller
    /// publishes its shared mark or snapshots the dependency's revision.
    pub(super) fn insert(&mut self, bits: usize, geometry: NativeWindowCollectionOwner) -> bool {
        let newly_owned = self.owners.insert(bits);
        // Retain headroom for replacement populations after pruning. Cleaning
        // tombstones can then reuse the warmed table instead of repeatedly
        // growing it for similarly sized owner populations in later epochs.
        if newly_owned && self.owners.len() > self.owners.capacity() / 2 {
            self.owners.reserve(self.owners.len());
        }
        let slot_geometry = match geometry {
            NativeWindowCollectionOwner::Other => return newly_owned,
            NativeWindowCollectionOwner::Cons => false,
            NativeWindowCollectionOwner::Slots => true,
        };
        let address = bits & !TAG_MASK;
        // A dead owner can be marked again at this address by another mutator
        // before epoch filtering removes our old identity. A real projection
        // must still promote Other -> Slots; positive membership alone may
        // remain conservative across reuse and never reads a stale header.
        let newly_native = self.addresses.insert(address);
        let newly_slot = slot_geometry && self.slot_addresses.insert(address);
        if !newly_native && !newly_slot {
            return newly_owned;
        }
        if self.single_cons == address {
            self.single_cons = 0;
        }
        if self.excluded.is_some_and(|gap| gap.covers(address)) {
            self.excluded = None;
            self.excluded_cons = 0;
        }
        if newly_native {
            self.include(address);
        }
        if newly_slot {
            self.include_slot(address);
        }
        true
    }

    /// Positive eligibility is cached from a live projection, independently
    /// of complete identity membership. It may conservatively survive reuse.
    pub(super) fn has_slot_geometry(&self, bits: usize) -> bool {
        self.slot_addresses.contains(bits & !TAG_MASK)
    }

    /// A completed local publication remains valid throughout a stable
    /// reclamation epoch. The ledger is changed only under State's exclusive
    /// borrow, and its insertion/mark/gate publication contains no Lisp call
    /// or safepoint. Another read cannot see the insertion before completion.
    /// Shared marks clear only on death, followed by Release epoch advancement
    /// before reuse/resumption; this Acquire check rejects every such change.
    #[inline]
    pub(super) fn already_published(&self, bits: usize) -> bool {
        self.epoch == Some(collection_observation_epoch())
            && self.owners.contains(&bits)
            && collection_observed_metadata(bits)
    }

    /// Compare with an epoch already acquired by the current pointer read.
    /// That read contains no callback or safepoint before its State borrow.
    pub(super) fn was_refreshed_at(&self, epoch: u64) -> bool {
        self.epoch == Some(epoch)
    }

    pub(super) fn full_bounds(&self) -> (usize, usize) {
        self.full
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn geometry_rebuilds_for_test(&self) -> usize {
        self.geometry_rebuilds
    }

    #[cfg(test)]
    pub(super) fn validated_identities_for_test(&self) -> usize {
        self.validated_identities
    }

    #[cfg(test)]
    pub(super) fn owner_count_for_test(&self) -> usize {
        self.owners.len()
    }

    #[cfg(test)]
    pub(super) fn epoch_for_test(&self) -> Option<u64> {
        self.epoch
    }

    #[cfg(test)]
    pub(super) fn validated_reclaimed_for_test(&self) -> u64 {
        self.validated_reclaimed
    }

    pub(super) fn ranges(&self) -> [BarrierWindow; 2] {
        [
            BarrierWindow::span(self.below.0, self.below.1),
            BarrierWindow::span(self.above.0, self.above.1),
        ]
    }

    pub(super) fn gate(&self) -> CompiledObservationGate {
        CompiledObservationGate {
            observed: self.ranges(),
            excluded: self.excluded,
            single_cons: self.single_cons,
            slots: [
                BarrierWindow::span(self.slot_below.0, self.slot_below.1),
                BarrierWindow::span(self.slot_above.0, self.slot_above.1),
            ],
        }
    }

    /// Exclude an interval containing no dependency owned by this mutator.
    /// Another mutator's sticky mark does not create a local certificate: the
    /// existing !Send journal contract requires shared writes separately.
    /// The ordinary dump is indivisible, even when none of it was observed.
    #[cold]
    #[inline(never)]
    pub(super) fn exclude_unobserved(&mut self, bits: usize) -> bool {
        let address = bits & !TAG_MASK;
        if self.owners.contains(&bits)
            || self.addresses.contains(address)
            || self.dump.covers(address)
            || self.single_cons == address
            || self.excluded.is_some_and(|gap| gap.covers(address))
        {
            return false;
        }
        let (previous, next) = self.addresses.neighbors(address);
        let mut lo = previous.map_or(0, |previous| previous + 1);
        let mut hi = next.unwrap_or(usize::MAX);
        if self.dump.len() != 0 {
            if address < self.dump.lo() {
                hi = hi.min(self.dump.lo());
            } else {
                lo = lo.max(self.dump.lo() + self.dump.len());
            }
        }
        let gap = BarrierWindow::span(lo, hi);
        debug_assert!(gap.covers(address));
        // The caller accepts only refusals inside the original hull. Its
        // separate hull predicate continues to reject every exterior store,
        // so an unrelated earlier hole must not monopolize this refinement.
        if self.single_cons != 0 && !gap.covers(self.single_cons) {
            // The independent exact refusal still covers another region.
        } else if self.excluded_cons != 0
            && self
                .excluded
                .is_some_and(|previous| previous.covers(self.excluded_cons))
            && !gap.covers(self.excluded_cons)
        {
            // Even a broad first gap needs an exact refusal when a second
            // disjoint region replaces it. Until then its range remains the
            // primary admission, avoiding an extra exact-path cost.
            self.single_cons = self.excluded_cons;
        } else if bits & TAG_MASK == TAG_CONS && gap.len() <= 2 * size_of::<ConsCell>() {
            self.single_cons = address;
        } else {
            self.single_cons = 0;
        }
        self.excluded = Some(gap);
        self.excluded_cons = if bits & TAG_MASK == TAG_CONS {
            address
        } else {
            0
        };
        true
    }

    #[cold]
    #[inline(never)]
    fn rebuild(&mut self, dump: BarrierWindow) {
        #[cfg(any(test, feature = "test-support"))]
        {
            self.geometry_rebuilds += 1;
        }
        // Removing dead identities cannot introduce an observed address into
        // an empty gap. A different dump span can, so only that change clears
        // the exclusion; collection alone keeps the proved gap available.
        if self.dump != dump {
            self.excluded = None;
            self.excluded_cons = 0;
            self.single_cons = 0;
        }
        self.dump = dump;
        // Keep the live-publication eligibility fact already stored in the
        // address buffer. Full ledger filtering above establishes lifetime;
        // these tag-only membership checks never read a possibly freed header.
        // A dump-span change preserves the same classification and buffer capacity.
        self.addresses.retain(|address| {
            self.owners.contains(&(address | TAG_CONS))
                || self.owners.contains(&(address | TAG_VECLIKE))
        });
        self.slot_addresses
            .retain(|address| self.owners.contains(&(address | TAG_VECLIKE)));
        self.full = EMPTY_BOUNDS;
        self.below = EMPTY_BOUNDS;
        self.above = EMPTY_BOUNDS;
        self.slot_below = EMPTY_BOUNDS;
        self.slot_above = EMPTY_BOUNDS;
        for address in self.addresses.iter() {
            include(&mut self.full, address);
            if dump.len() == 0 || address < dump.lo() {
                include(&mut self.below, address);
            } else if !dump.covers(address) {
                include(&mut self.above, address);
            }
        }
        for address in self.slot_addresses.iter() {
            if dump.len() == 0 || address < dump.lo() {
                include(&mut self.slot_below, address);
            } else if !dump.covers(address) {
                include(&mut self.slot_above, address);
            }
        }
    }

    fn include(&mut self, address: usize) {
        include(&mut self.full, address);
        if self.dump.len() == 0 || address < self.dump.lo() {
            include(&mut self.below, address);
        } else if !self.dump.covers(address) {
            include(&mut self.above, address);
        }
    }

    fn include_slot(&mut self, address: usize) {
        if self.dump.len() == 0 || address < self.dump.lo() {
            include(&mut self.slot_below, address);
        } else if !self.dump.covers(address) {
            include(&mut self.slot_above, address);
        }
    }
}

#[inline]
fn include(bounds: &mut (usize, usize), address: usize) {
    bounds.0 = bounds.0.min(address);
    bounds.1 = bounds.1.max(address + 1);
}

#[cfg(test)]
#[path = "tests/observed_reclamation.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/observed_disjoint_refinement.rs"]
mod disjoint_refinement_tests;

#[cfg(test)]
#[path = "tests/envelope_allocation_test.rs"]
mod allocation_tests;

#[cfg(test)]
#[path = "tests/observed_address_ledger.rs"]
mod address_ledger_tests;

/// Sorted address ledger with a bounded hot run.
///
/// A single retained sorted `Vec` makes live publication O(n): an address that
/// sorts below the retained bulk shifts every later address. The observed
/// workload inserts around index 81k of a ~1.15M-entry ledger, so every
/// observation moved ~8.5 MB (issue #767).
///
/// New addresses enter a small sorted hot run instead. Membership and
/// predecessor/successor queries search both runs; when the hot run fills it
/// merges into the bulk in one linear pass. Bulk capacity is retained across
/// reclamation, so steady-state publication reuses its warmed allocation.
struct SortedAddressLedger {
    bulk: Vec<usize>,
    hot: Vec<usize>,
}

/// Bounds both the per-insert shift and the amortized bulk merges. At ~1.1M
/// addresses one observation moves about `HOT_ADDRESS_LIMIT / 2 + n / HOT_ADDRESS_LIMIT`
/// elements (~1.6k, ~13 KB) instead of the whole tail.
const HOT_ADDRESS_LIMIT: usize = 1024;

impl SortedAddressLedger {
    fn new() -> Self {
        Self {
            bulk: Vec::new(),
            hot: Vec::new(),
        }
    }

    fn insert(&mut self, address: usize) -> bool {
        self.insert_inner::<false>(address).0
    }

    /// Returns whether the address was inserted and how many address slots
    /// moved. Only the issue #767 regression test asks for the count; the
    /// const generic removes the arithmetic from release builds.
    fn insert_inner<const COUNT: bool>(&mut self, address: usize) -> (bool, usize) {
        if self.bulk.binary_search(&address).is_ok() {
            return (false, 0);
        }
        let index = match self.hot.binary_search(&address) {
            Ok(_) => return (false, 0),
            Err(index) => index,
        };
        let shifted = if COUNT { self.hot.len() - index } else { 0 };
        self.hot.insert(index, address);
        let mut moved = shifted;
        if self.hot.len() >= HOT_ADDRESS_LIMIT {
            let merged = self.merge_hot();
            if COUNT {
                moved += merged;
            }
        }
        (true, moved)
    }

    fn contains(&self, address: usize) -> bool {
        self.bulk.binary_search(&address).is_ok() || self.hot.binary_search(&address).is_ok()
    }

    /// Nearest observed address strictly below and at-or-above `address`.
    /// The caller has already established that `address` itself is absent.
    fn neighbors(&self, address: usize) -> (Option<usize>, Option<usize>) {
        let (bulk_previous, bulk_next) = run_neighbors(&self.bulk, address);
        let (hot_previous, hot_next) = run_neighbors(&self.hot, address);
        (
            match (bulk_previous, hot_previous) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
            match (bulk_next, hot_next) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
        )
    }

    /// Iterates both runs in insertion-free order. Bounds only need each
    /// address once; sorted order is not required here.
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.bulk.iter().copied().chain(self.hot.iter().copied())
    }

    fn retain(&mut self, mut keep: impl FnMut(usize) -> bool) {
        self.bulk.retain(|&address| keep(address));
        self.hot.retain(|&address| keep(address));
    }

    /// Merge the sorted hot run into the sorted bulk in place. Reads and writes
    /// never cross: the destination index stays at or above the unread bulk
    /// element because the hot run always contributes at least one element per
    /// step. Capacity is expected to be warm in steady state; growth here is
    /// the same amortized allocation a growing `Vec` already performs.
    fn merge_hot(&mut self) -> usize {
        if self.hot.is_empty() {
            return 0;
        }
        let bulk_len = self.bulk.len();
        let hot_len = self.hot.len();
        self.bulk.resize(bulk_len + hot_len, 0);
        let mut bulk = bulk_len;
        let mut hot = hot_len;
        let mut destination = bulk_len + hot_len;
        while hot > 0 {
            destination -= 1;
            if bulk > 0 && self.bulk[bulk - 1] > self.hot[hot - 1] {
                self.bulk[destination] = self.bulk[bulk - 1];
                bulk -= 1;
            } else {
                self.bulk[destination] = self.hot[hot - 1];
                hot -= 1;
            }
        }
        self.hot.clear();
        bulk_len + hot_len
    }
}

fn run_neighbors(run: &[usize], address: usize) -> (Option<usize>, Option<usize>) {
    let next = run.partition_point(|&candidate| candidate < address);
    (
        next.checked_sub(1)
            .and_then(|previous| run.get(previous).copied()),
        run.get(next).copied(),
    )
}
