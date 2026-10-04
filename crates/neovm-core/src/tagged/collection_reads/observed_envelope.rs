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
use super::super::value::TAG_MASK;
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;

const EMPTY_BOUNDS: (usize, usize) = (usize::MAX, 0);
const MAX_RECLAMATION_RETRIES: usize = 3;

pub(super) struct ObservedEnvelope {
    owners: FxHashSet<usize>,
    addresses: BTreeSet<usize>,
    epoch: Option<u64>,
    dump: BarrierWindow,
    full: (usize, usize),
    below: (usize, usize),
    above: (usize, usize),
    // Protocol bounds for a proven empty interval, never an owner hint or a
    // Lisp-state cache. Only this mutator's observed identities constrain it.
    excluded: Option<BarrierWindow>,
}

impl ObservedEnvelope {
    pub(super) fn new() -> Self {
        Self {
            owners: FxHashSet::default(),
            addresses: BTreeSet::new(),
            epoch: Some(collection_observation_epoch()),
            dump: BarrierWindow::NONE,
            full: EMPTY_BOUNDS,
            below: EMPTY_BOUNDS,
            above: EMPTY_BOUNDS,
            excluded: None,
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
        self.refresh_reclaimed(dump);
        true
    }

    /// The scanner cannot accept a completed concurrent clearing batch under
    /// its old epoch. Ordinary stop-the-world/cooperative batches are stable
    /// on their first pass. Continuous other-mutator reclamation is bounded:
    /// retain a conservative surviving superset and leave the epoch dirty so
    /// the next cold access retries. A false bit only removes a dead identity,
    /// never a live rooted dependency whose sticky mark survives collection.
    #[cold]
    #[inline(never)]
    fn refresh_reclaimed(&mut self, dump: BarrierWindow) {
        for _ in 0..MAX_RECLAMATION_RETRIES {
            let before = collection_observation_epoch();
            self.owners
                .retain(|&bits| collection_observed_metadata(bits));
            self.rebuild(dump);
            let after = collection_observation_epoch();
            if before == after {
                self.epoch = Some(after);
                return;
            }
        }
        self.epoch = None;
    }

    /// Insert even when another mutator already set the process-wide mark.
    /// New observations update both buckets and the ordered address ledger;
    /// only reclamation or a dump-span change scans all local identities.
    /// A newly observed owner closes any gap covering it before the caller
    /// publishes its shared mark or snapshots the dependency's revision.
    pub(super) fn insert(&mut self, bits: usize) -> bool {
        if !self.owners.insert(bits) {
            return false;
        }
        let address = bits & !TAG_MASK;
        if self.excluded.is_some_and(|gap| gap.covers(address)) {
            self.excluded = None;
        }
        self.addresses.insert(address);
        self.include(address);
        true
    }

    pub(super) fn full_bounds(&self) -> (usize, usize) {
        self.full
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
        self.excluded = Some(gap);
        true
    }

    #[cold]
    #[inline(never)]
    fn rebuild(&mut self, dump: BarrierWindow) {
        // Removing dead identities cannot introduce an observed address into
        // an empty gap. A different dump span can, so only that change clears
        // the exclusion; collection alone keeps the proved gap available.
        if self.dump != dump {
            self.excluded = None;
        }
        self.dump = dump;
        self.addresses.clear();
        self.full = EMPTY_BOUNDS;
        self.below = EMPTY_BOUNDS;
        self.above = EMPTY_BOUNDS;
        for &bits in &self.owners {
            let address = bits & !TAG_MASK;
            self.addresses.insert(address);
            include(&mut self.full, address);
            if dump.len() == 0 || address < dump.lo() {
                include(&mut self.below, address);
            } else if !dump.covers(address) {
                include(&mut self.above, address);
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
}

#[inline]
fn include(bounds: &mut (usize, usize), address: usize) {
    bounds.0 = bounds.0.min(address);
    bounds.1 = bounds.1.max(address + 1);
}
