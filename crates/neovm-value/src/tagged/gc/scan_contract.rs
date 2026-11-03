//! Admission and storage retention for the legacy single-writer scan protocol.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

use super::TaggedHeap;

/// A thread-confined admission to one heap's quiescent snapshot capture.
///
/// This is the legacy serialized-writer protocol. A parallel World must produce
/// this admission from its stop-all/writer capability, rather than assuming that
/// an ordinary heap borrow excludes writers reaching the heap through TLS.
#[must_use = "the admission keeps the heap exclusively borrowed during capture"]
pub(crate) struct SingleMutatorWorld<'h> {
    heap: &'h mut TaggedHeap,
    _thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(SingleMutatorWorld<'static>: Send, Sync);
static_assertions::assert_impl_all!(SingleMutatorWorld<'static>: std::fmt::Debug);

impl<'h> SingleMutatorWorld<'h> {
    /// Admit the heap's owner to a start snapshot.
    ///
    /// # Safety
    /// The caller is the heap's only writer, including through legacy TLS raw
    /// pointers. Capture runs without Lisp callbacks or allocation safepoints.
    /// The matching obarray has that same exclusive writer. Until this mark
    /// finishes or is abandoned, writes use SATB, vector retirement and symbol
    /// seqlock publication; storage is retained through the marker's last read.
    pub(crate) unsafe fn from_heap(heap: &'h mut TaggedHeap) -> Self {
        Self {
            heap,
            _thread: PhantomData,
        }
    }

    pub(crate) fn heap_identity(&self) -> usize {
        self.heap.identity()
    }

    pub(crate) fn heap(&self) -> &TaggedHeap {
        self.heap
    }
}

impl std::fmt::Debug for SingleMutatorWorld<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleMutatorWorld")
            .field("heap_identity", &self.heap_identity())
            .finish()
    }
}

/// The unique owner of a stable allocation's scan-retention identity.
///
/// Only this owner and its admitted leases hold the private Arc. Checking Arc
/// uniqueness proves that all leases have ended before storage is reclaimed.
/// The owner is not cloneable, and leases never expose or downgrade the Arc.
#[repr(transparent)]
pub(crate) struct ScanStorageOwner {
    storage: Arc<()>,
}

static_assertions::assert_impl_all!(ScanStorageOwner: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ScanStorageOwner: Clone);
const _: () = {
    assert!(std::mem::size_of::<ScanStorageOwner>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<ScanStorageOwner>() == std::mem::align_of::<usize>());
};

impl ScanStorageOwner {
    pub(crate) fn new() -> Self {
        Self {
            storage: Arc::new(()),
        }
    }

    /// True when a scan lease still requires this owner's storage.
    ///
    /// Arc's uniqueness check synchronizes with completed lease destruction.
    /// An outstanding lease causes the owner to retain its allocation instead
    /// of waiting. Private Arc construction prevents future readers after the
    /// owner has entered its exclusive destruction path.
    pub(crate) fn has_leases(&mut self) -> bool {
        Arc::get_mut(&mut self.storage).is_none()
    }
}

impl std::fmt::Debug for ScanStorageOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanStorageOwner")
            .field(
                "leases",
                &Arc::strong_count(&self.storage).saturating_sub(1),
            )
            .finish()
    }
}

/// A marker reader's lease on an owner-retained, stable storage allocation.
///
/// The lease holds the owner's private Arc through its final raw-pointer read.
/// Its ordinary Arc destructor ends retention; no second reader counter or
/// callback is needed. This lease is neither cloneable nor shareable.
#[must_use = "dropping the lease ends this reader's storage retention"]
pub(crate) struct ScanStorageLease {
    storage: Arc<()>,
    _exclusive_reader: PhantomData<Cell<()>>,
}

static_assertions::assert_impl_all!(ScanStorageLease: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ScanStorageLease: Sync, Clone);

impl ScanStorageLease {
    pub(crate) fn capture(owner: &ScanStorageOwner, _: &SingleMutatorWorld<'_>) -> Self {
        Self {
            storage: Arc::clone(&owner.storage),
            _exclusive_reader: PhantomData,
        }
    }
}

impl std::fmt::Debug for ScanStorageLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanStorageLease")
            .field(
                "leases",
                &Arc::strong_count(&self.storage).saturating_sub(1),
            )
            .finish()
    }
}

/// Only this module can mint the drain key, after unsafe closed-world
/// admission. The key cannot be sent to another owner or cloned.
#[derive(Debug)]
pub(super) struct StoppedMutatorsProof {
    _owner: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(StoppedMutatorsProof: Send, Sync, Copy, Clone);

/// Parallel drain authority is scoped to the unique AllStopped guard. It owns
/// no heap loan; A5 must additionally supply disjoint stopped-storage access.
#[derive(Debug)]
pub(in crate::tagged::gc) struct ParallelStoppedMutatorsProof<'epoch> {
    legacy: StoppedMutatorsProof,
    _epoch: PhantomData<&'epoch ()>,
}
impl<'epoch> ParallelStoppedMutatorsProof<'epoch> {
    pub(in crate::tagged::gc) fn from_all_stopped(_: &'epoch super::AllStopped<'_>) -> Self {
        Self {
            legacy: StoppedMutatorsProof {
                _owner: PhantomData,
            },
            _epoch: PhantomData,
        }
    }
    pub(in crate::tagged::gc) fn legacy(&self) -> &StoppedMutatorsProof {
        &self.legacy
    }
}
static_assertions::assert_not_impl_any!(ParallelStoppedMutatorsProof<'static>: Send, Sync, Clone, Copy);

#[derive(Debug, thiserror::Error)]
pub(super) enum SatbDrainError {
    #[error(transparent)]
    Poisoned(#[from] super::satb::SatbPoisoned),
    #[error("SATB words survived into a new collector epoch")]
    Residual,
}

/// Separate owner-resident words from batches already published to the marker.
#[derive(Debug)]
#[must_use]
struct SatbFoldCounts {
    total_words: usize,
    partial_words: usize,
}

impl SingleMutatorWorld<'_> {
    /// Fold only after the marker's last read has been joined. The caller
    /// excludes all registered mutators under the existing admission contract.
    pub(super) fn fold_satb(&mut self) -> Result<usize, super::satb::SatbPoisoned> {
        Ok(self.fold_satb_counts()?.total_words)
    }

    /// Record diagnostics only for an actual termination. A later facade
    /// quiescence fold must not overwrite this with an incidental empty fold.
    pub(super) fn fold_satb_for_termination(&mut self) -> Result<usize, super::satb::SatbPoisoned> {
        let counts = self.fold_satb_counts()?;
        // The successful fold checked the marker join; this capability has
        // retained exclusive stopped-world admission without running Lisp.
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap
            .record_termination_partial_words(counts.partial_words, &proof);
        Ok(counts.total_words)
    }

    fn fold_satb_counts(&mut self) -> Result<SatbFoldCounts, super::satb::SatbPoisoned> {
        assert!(
            !self.heap.concurrent_mark_running(),
            "SATB fold requires an explicit successful marker join"
        );
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        let batches = self.heap.satb_handoff.take_all()?;
        let mut words = 0;
        for chunk in batches {
            words += chunk.len();
            self.heap
                .gray_queue
                .extend(chunk.into_words().into_iter().map(super::MarkWord::value));
        }
        // Split through the heap-wide iterator so a future K-mutator registry
        // cannot accidentally leave an exited registration's partial behind.
        let mut partials = Vec::new();
        let mut partial_words = 0;
        for mutator in self.heap.mutator_gc_entries_world_stopped(&proof) {
            // Carry the paired hot-state loan through enumeration; future
            // registries must not enumerate SATB state independently.
            let _hot_owner = mutator.gc;
            if let Some(chunk) = mutator.satb.satb.take_partial(&proof) {
                words += chunk.len();
                partial_words += chunk.len();
                partials.push(chunk);
            }
        }
        for chunk in partials {
            self.heap
                .gray_queue
                .extend(chunk.into_words().into_iter().map(super::MarkWord::value));
        }
        Ok(SatbFoldCounts {
            total_words: words,
            partial_words,
        })
    }

    /// Production may choose a debug assertion at begin/launch while facade
    /// quiescence maps this result to ResidualMark. Never clear on failure.
    pub(super) fn ensure_satb_empty(&self) -> Result<(), SatbDrainError> {
        if !self.heap.satb_handoff.is_empty()?
            || self.heap.mutator_satb_states().any(|m| !m.satb.is_empty())
        {
            return Err(SatbDrainError::Residual);
        }
        Ok(())
    }

    /// Begin/launch calls validate before resetting only owner dedup sets.
    pub(super) fn begin_satb_cycle(&mut self) -> Result<(), SatbDrainError> {
        assert!(
            !self.heap.concurrent_mark_running(),
            "SATB begin requires a stopped collector"
        );
        self.ensure_satb_empty()?;
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        for mutator in self.heap.mutator_gc_entries_world_stopped(&proof) {
            mutator.satb.begin_cycle(&proof)?;
        }
        Ok(())
    }

    /// Union the dirty-owner logs before insertion repair. Retired registrations
    /// stay present and contribute exactly like active registrations.
    pub(super) fn take_satb_written_owners(&mut self) -> rustc_hash::FxHashSet<usize> {
        assert!(
            !self.heap.concurrent_mark_running(),
            "SATB insertion repair requires an explicit marker join"
        );
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        let mut owners = rustc_hash::FxHashSet::default();
        for mutator in self.heap.mutator_gc_entries_world_stopped(&proof) {
            owners.extend(mutator.satb.satb_snapshotted_owners.drain());
        }
        owners
    }

    /// Preflight before any minor cycle, mark, parity, promotion or gray-work
    /// mutation. Physical parking is insufficient while a worker owns a loan.
    /// Checks one canonical outstanding counter, not every retired entry.
    pub(super) fn ensure_young_logs_stopped(
        &self,
    ) -> Result<(), super::young_registry::YoungRegistryError> {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap
            ._satb_layout_reserve
            .ensure_young_logs_stopped(&proof)
    }

    /// Borrow disjoint heap fields under the existing closed-world admission.
    /// No Lisp callbacks, allocation safepoints or mutator resumes may occur
    /// inside `body`. A worker must have joined and every young-only loan must
    /// be returned before any caller mutation. The proof cannot escape `body`.
    pub(super) fn with_stopped_heap<R>(
        &mut self,
        body: impl FnOnce(&mut TaggedHeap, &StoppedMutatorsProof) -> R,
    ) -> Result<R, super::young_registry::YoungRegistryError> {
        assert!(
            !self.heap.concurrent_mark_running(),
            "stopped heap access requires an explicit successful marker join"
        );
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap
            ._satb_layout_reserve
            .ensure_young_logs_stopped(&proof)?;
        Ok(body(&mut *self.heap, &proof))
    }

    /// Split the allocator and the canonical cold registry under the same
    /// closed-world proof used for SATB folding. No callbacks into Lisp or GC,
    /// allocation safepoints, or mutator resumes are allowed inside `body`.
    ///
    /// The bridge itself never drains logs or clears flags. C2/C3 use its
    /// resolver for capture, then ConsStopAll clears flags only after promotion
    /// and detaches sweep work before releasing this physical stop-all.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Typed split bridge retained for worker integration; current collectors use with_stopped_heap."
        )
    )]
    pub(super) fn with_stopped_young_logs<R>(
        &mut self,
        body: impl FnOnce(
            &mut super::cons_allocator::ConsAllocatorOwner,
            &mut super::young_registry::StoppedYoungLogs<'_>,
            &[super::ConsBlock],
        ) -> R,
    ) -> Result<R, super::young_registry::YoungRegistryError> {
        assert!(
            !self.heap.concurrent_mark_running(),
            "young capture requires an explicit successful marker join"
        );
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        let heap = &mut *self.heap;
        let mut logs =
            super::young_registry::StoppedYoungLogs::new(&mut heap._satb_layout_reserve, &proof)?;
        Ok(body(&mut heap.cons_allocator, &mut logs, &heap.cons_blocks))
    }

    /// Transfer only a young payload to a worker; SATB remains owner-confined.
    /// The caller must return the loan before collection or owner retirement.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(super) fn take_young_log_loan(
        &mut self,
        id: super::young_registry::YoungRegistrationId,
    ) -> Result<super::young_registry::YoungLogLoan, super::young_registry::YoungRegistryError>
    {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap._satb_layout_reserve.take_young_loan(id, &proof)
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(super) fn return_young_log_loan(
        &mut self,
        loan: super::young_registry::YoungLogLoan,
    ) -> Result<(), super::young_registry::YoungLogReturnError> {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap
            ._satb_layout_reserve
            .return_young_loan(loan, &proof)
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(super) fn current_young_registration_id(
        &self,
    ) -> super::young_registry::YoungRegistrationId {
        self.heap._satb_layout_reserve.current_young_id()
    }

    #[cfg(test)]
    pub(super) fn registered_young_id_for_test(
        &self,
        id: super::mutator_gc::TestMutatorGcId,
    ) -> Result<super::young_registry::YoungRegistrationId, super::young_registry::YoungRegistryError>
    {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap
            ._satb_layout_reserve
            .registered_young_id(id, &proof)
    }

    /// Logical young IDs continue to resolve the same canonical cold owner
    /// while nested physical test-owner selections are installed.
    #[cfg(test)]
    pub(super) fn retire_young_registration_for_test(
        &mut self,
        id: super::young_registry::YoungRegistrationId,
    ) -> Result<super::satb::SatbRetired, super::mutator_gc::MutatorRetireError> {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        let handoff = self.heap.satb_handoff.clone();
        self.heap
            ._satb_layout_reserve
            .retire_young_registration(id, &handoff, &proof)
    }

    #[cfg(test)]
    pub(super) fn register_mutator_gc_for_test(&mut self) -> super::mutator_gc::TestMutatorGcId {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap.register_mutator_gc_for_test(&proof)
    }

    #[cfg(test)]
    pub(super) fn mutator_gc_for_test(
        &mut self,
        id: super::mutator_gc::TestMutatorGcId,
    ) -> super::mutator_gc::StoppedMutatorGc<'_> {
        let proof = StoppedMutatorsProof {
            _owner: PhantomData,
        };
        self.heap.registered_mutator_gc_for_test(id, &proof)
    }

    #[cfg(test)]
    #[must_use = "test retirement must be acknowledged before unregistering"]
    pub(super) fn retire_mutator_gc_for_test(
        &mut self,
        id: super::mutator_gc::TestMutatorGcId,
    ) -> Result<super::satb::SatbRetired, super::mutator_gc::MutatorRetireError> {
        let handoff = self.heap.satb_handoff.clone();
        self.mutator_gc_for_test(id).satb.retire(&handoff)
    }
}

#[cfg(test)]
impl StoppedMutatorsProof {
    /// Transport-only tests own every buffer locally and have no heap aliases.
    pub(super) fn for_test() -> Self {
        Self {
            _owner: PhantomData,
        }
    }
}
