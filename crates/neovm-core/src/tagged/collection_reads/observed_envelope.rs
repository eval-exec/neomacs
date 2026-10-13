//! Exact local observation identities and the collector's conservative gate.
//!
//! This lives inside the existing mutator journal State, never in a Context
//! or a new TLS identity cache. Certificates and its ledger remain !Send;
//! only the executing mutator changes them. Shared permanent mark metadata
//! and the reclamation epoch are atomic. No old owner header is dereferenced.

use super::super::gc::{
    BarrierWindow, CompiledObservationGate, collection_observation_epoch,
    collection_observed_metadata,
};
use super::super::header::ConsCell;
use super::super::value::{TAG_CONS, TAG_MASK, TAG_VECLIKE};
use super::NativeWindowCollectionOwner;
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;

const EMPTY_BOUNDS: (usize, usize) = (usize::MAX, 0);
const MAX_RECLAMATION_RETRIES: usize = 3;

pub(super) struct ObservedEnvelope {
    // Complete lifetime publication ledger, including non-inline subtypes.
    owners: FxHashSet<usize>,
    // Cached native-store eligibility from live publication. A Buffer/Window/
    // Frame/Bytecode/String owner is still marked/replay-qualified in `owners`,
    // but cannot widen a native write gate or bound its empty intervals.
    addresses: BTreeSet<usize>,
    // Vector/Record eligibility is also cached at live publication. Native
    // slot stores use these bounds independently of observed Cons addresses.
    slot_addresses: BTreeSet<usize>,
    epoch: Option<u64>,
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
    #[cfg(test)]
    geometry_rebuilds: usize,
}

impl ObservedEnvelope {
    pub(super) fn new() -> Self {
        Self {
            owners: FxHashSet::default(),
            addresses: BTreeSet::new(),
            slot_addresses: BTreeSet::new(),
            epoch: Some(collection_observation_epoch()),
            dump: BarrierWindow::NONE,
            full: EMPTY_BOUNDS,
            below: EMPTY_BOUNDS,
            above: EMPTY_BOUNDS,
            slot_below: EMPTY_BOUNDS,
            slot_above: EMPTY_BOUNDS,
            excluded: None,
            excluded_cons: 0,
            single_cons: 0,
            #[cfg(test)]
            geometry_rebuilds: 0,
        }
    }

    /// Refresh once per reclamation epoch or current dump-span change. An
    /// observer never initializes this ledger merely to reject an empty gate.
    pub(super) fn refresh(&mut self, dump: BarrierWindow) -> bool {
        let epoch = collection_observation_epoch();
        if self.epoch == Some(epoch) {
            if self.dump == dump {
                return false;
            }
            self.rebuild(dump);
            return true;
        }
        self.refresh_reclaimed(dump)
    }

    /// The scanner cannot accept a completed concurrent clearing batch under
    /// its old epoch. Ordinary stop-the-world/cooperative batches are stable
    /// on their first pass. Continuous other-mutator reclamation is bounded:
    /// retain a conservative surviving superset and leave the epoch dirty so
    /// the next cold access retries. A false bit only removes a dead identity,
    /// never a live rooted dependency whose sticky mark survives collection.
    #[cold]
    #[inline(never)]
    fn refresh_reclaimed(&mut self, dump: BarrierWindow) -> bool {
        let mut changed = false;
        for _ in 0..MAX_RECLAMATION_RETRIES {
            let before = collection_observation_epoch();
            let old_len = self.owners.len();
            self.owners
                .retain(|&bits| collection_observed_metadata(bits));
            // Other heaps share the reclamation epoch. Validate all local
            // identities, but keep the ordered nodes and bounds when none of
            // them died; an unrelated sweep must not rebuild this ledger.
            if self.owners.len() != old_len || self.dump != dump {
                self.rebuild(dump);
                changed = true;
            }
            let after = collection_observation_epoch();
            if before == after {
                self.epoch = Some(after);
                return changed;
            }
        }
        self.epoch = None;
        changed
    }

    /// Insert even when another mutator already set the process-wide mark.
    /// Native-store-eligible observations update the address buckets/tree;
    /// only reclamation or a dump-span change scans all local identities.
    /// A newly observed owner closes any gap covering it before the caller
    /// publishes its shared mark or snapshots the dependency's revision.
    pub(super) fn insert(&mut self, bits: usize, geometry: NativeWindowCollectionOwner) -> bool {
        let newly_owned = self.owners.insert(bits);
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
        self.slot_addresses.contains(&(bits & !TAG_MASK))
    }

    /// A completed local publication remains valid throughout a stable
    /// reclamation epoch. The ledger is changed only under State's exclusive
    /// borrow, and its insertion/mark/gate publication contains no Lisp call
    /// or safepoint. Another read cannot see the insertion before completion.
    /// Shared marks clear only on death, followed by Release epoch advancement
    /// before reuse/resumption; this Acquire check rejects every such change.
    #[inline]
    pub(super) fn already_published(&self, bits: usize) -> bool {
        self.epoch == Some(collection_observation_epoch()) && self.owners.contains(&bits)
    }

    /// Compare with an epoch already acquired by the current pointer read.
    /// That read contains no callback or safepoint before its State borrow.
    pub(super) fn was_refreshed_at(&self, epoch: u64) -> bool {
        self.epoch == Some(epoch)
    }

    pub(super) fn full_bounds(&self) -> (usize, usize) {
        self.full
    }

    #[cfg(test)]
    pub(super) fn geometry_rebuilds_for_test(&self) -> usize {
        self.geometry_rebuilds
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
            || self.addresses.contains(&address)
            || self.dump.covers(address)
            || self.single_cons == address
            || self.excluded.is_some_and(|gap| gap.covers(address))
        {
            return false;
        }
        let mut lo = self
            .addresses
            .range(..address)
            .next_back()
            .map_or(0, |previous| previous + 1);
        let mut hi = self
            .addresses
            .range(address..)
            .next()
            .copied()
            .unwrap_or(usize::MAX);
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
        #[cfg(test)]
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
        // address tree. Full ledger filtering above establishes lifetime;
        // these tag-only membership checks never read a possibly freed header.
        // A dump-span change preserves the same classification and tree nodes.
        self.addresses.retain(|&address| {
            self.owners.contains(&(address | TAG_CONS))
                || self.owners.contains(&(address | TAG_VECLIKE))
        });
        self.slot_addresses
            .retain(|&address| self.owners.contains(&(address | TAG_VECLIKE)));
        self.full = EMPTY_BOUNDS;
        self.below = EMPTY_BOUNDS;
        self.above = EMPTY_BOUNDS;
        self.slot_below = EMPTY_BOUNDS;
        self.slot_above = EMPTY_BOUNDS;
        for &address in &self.addresses {
            include(&mut self.full, address);
            if dump.len() == 0 || address < dump.lo() {
                include(&mut self.below, address);
            } else if !dump.covers(address) {
                include(&mut self.above, address);
            }
        }
        for &address in &self.slot_addresses {
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
#[path = "tests/observed_native_geometry.rs"]
mod native_geometry_tests;

#[cfg(test)]
#[path = "tests/observed_refinement.rs"]
mod refinement_tests;

#[cfg(test)]
#[path = "tests/observed_disjoint_refinement.rs"]
mod disjoint_refinement_tests;
