//! Closed concurrent admission and retained Phase-0 facade exclusion.
//!
//! This is the serialized-owner protocol, not admission for simultaneous
//! mutators. The cold state never contains a heap pointer or a Lisp value.

use super::*;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

/// Why an owner could not begin or publish a concurrent cycle.
#[derive(Debug, thiserror::Error)]
pub enum ConcurrentMarkAdmissionError {
    #[error("facade roots still exclude concurrent marking for heap {0:?}")]
    Excluded(HeapIdentity),
    #[error("concurrent admission belongs to heap {expected:?}, not {actual:?}")]
    ForeignHeap {
        expected: HeapIdentity,
        actual: HeapIdentity,
    },
    #[error("the heap has unfinished collector work")]
    CollectorBusy,
    #[error("the heap is not in an unlaunched concurrent start")]
    NotStarted,
}

/// Failure to retain or validate a facade epoch.
#[derive(Debug, thiserror::Error)]
pub enum FacadeMarkExclusionError {
    #[error("facade exclusion belongs to heap {expected:?}, not {actual:?}")]
    ForeignHeap {
        expected: HeapIdentity,
        actual: HeapIdentity,
    },
    #[error("facade epoch retention count is exhausted")]
    RetentionLimit,
}

/// A complete stopped-owner drain failed to establish collector quiescence.
#[derive(Debug, thiserror::Error)]
pub enum CollectorQuiescenceError {
    #[error(transparent)]
    Marker(#[from] MarkFinishError),
    #[error("collector state was poisoned")]
    PoisonedCollectorState,
    #[error("marking or sweeping is still active")]
    CollectorBusy,
    #[error("residual marking work has no active collection to finish")]
    ResidualMark,
    #[error("a concurrent start snapshot is still pending")]
    PendingSnapshot,
    #[error("first-partition collection has not completed")]
    PendingPartition,
}

static_assertions::assert_impl_all!(ConcurrentMarkAdmissionError: Send, Sync, std::fmt::Debug);
static_assertions::assert_impl_all!(FacadeMarkExclusionError: Send, Sync, std::fmt::Debug);
static_assertions::assert_impl_all!(CollectorQuiescenceError: Send, Sync, std::fmt::Debug);

/// One count per authority or retained publication/reader obligation.
#[repr(transparent)]
#[derive(Debug)]
struct FacadeRetentionCount(AtomicUsize);

static_assertions::assert_impl_all!(FacadeRetentionCount: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(FacadeRetentionCount: Copy, Clone);
const _: () = {
    assert!(std::mem::size_of::<FacadeRetentionCount>() == std::mem::size_of::<AtomicUsize>());
    assert!(std::mem::align_of::<FacadeRetentionCount>() == std::mem::align_of::<AtomicUsize>());
    assert!(std::mem::offset_of!(FacadeRetentionCount, 0) == 0);
};

impl FacadeRetentionCount {
    fn retain(&self) -> Result<(), FacadeMarkExclusionError> {
        self.0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                count.checked_add(1)
            })
            .map(|_| ())
            .map_err(|_| FacadeMarkExclusionError::RetentionLimit)
    }

    fn is_held(&self) -> bool {
        // Every token's final read precedes its Release decrement. Reading
        // zero with Acquire synchronizes with the last decrement's release
        // sequence, including readers on other threads. Creation is serialized
        // with launch by the unique owner admission; this is not a launcher CAS.
        self.0.load(Ordering::Acquire) != 0
    }

    fn release(&self) {
        // Only the private successful retain path constructs a token. Tokens
        // are not Copy/Clone and release once, so this cannot underflow. Drop
        // performs no lock, callback, wait or assertion.
        self.0.fetch_sub(1, Ordering::Release);
    }
}

pub(super) struct FacadeMarkState {
    heap: HeapIdentity,
    retained: FacadeRetentionCount,
}

static_assertions::assert_impl_all!(FacadeMarkState: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(FacadeMarkState: Copy, Clone);

impl std::fmt::Debug for FacadeMarkState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FacadeMarkState")
            .field("heap", &self.heap)
            .field("retained", &self.retained.0.load(Ordering::Relaxed))
            .finish()
    }
}

/// Metadata retention only; it grants no heap or Lisp-value access.
///
/// A publication and its readers keep this token through their final read.
/// Abandoning an incomplete publication must retain, rather than drop, the
/// token with its root table. Retired inert payloads hold no such token.
#[repr(transparent)]
#[must_use = "the token retains exclusion through the final publication or reader obligation"]
pub struct FacadeEpochRetention {
    state: Arc<FacadeMarkState>,
}

static_assertions::assert_impl_all!(FacadeEpochRetention: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(FacadeEpochRetention: Copy, Clone);
const _: () = {
    assert!(std::mem::size_of::<FacadeEpochRetention>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<FacadeEpochRetention>() == std::mem::align_of::<usize>());
    assert!(std::mem::offset_of!(FacadeEpochRetention, state) == 0);
};

impl FacadeEpochRetention {
    pub fn heap_identity(&self) -> HeapIdentity {
        self.state.heap
    }

    pub fn retain(&self) -> Result<Self, FacadeMarkExclusionError> {
        self.state.retained.retain()?;
        Ok(Self {
            state: Arc::clone(&self.state),
        })
    }

    fn validate(&self, heap: &TaggedHeap) -> Result<(), FacadeMarkExclusionError> {
        let actual = heap.heap_identity();
        if actual == self.heap_identity() {
            Ok(())
        } else {
            Err(FacadeMarkExclusionError::ForeignHeap {
                expected: self.heap_identity(),
                actual,
            })
        }
    }
}

impl Drop for FacadeEpochRetention {
    fn drop(&mut self) {
        self.state.retained.release();
    }
}

impl std::fmt::Debug for FacadeEpochRetention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FacadeEpochRetention")
            .field(&self.state)
            .finish()
    }
}

/// Owner-confined authority to retain Phase-0 concurrent-mark exclusion.
#[must_use = "the authority or its retained tokens must cover every facade obligation"]
pub struct FacadeMarkExclusion {
    epoch: FacadeEpochRetention,
    _owner: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(FacadeMarkExclusion: Send, Sync, Copy, Clone);
static_assertions::assert_impl_all!(FacadeMarkExclusion: std::fmt::Debug);

impl FacadeMarkExclusion {
    pub fn heap_identity(&self) -> HeapIdentity {
        self.epoch.heap_identity()
    }

    pub fn validate(&self, heap: &TaggedHeap) -> Result<(), FacadeMarkExclusionError> {
        self.epoch.validate(heap)
    }

    pub fn retain(&self) -> Result<FacadeEpochRetention, FacadeMarkExclusionError> {
        self.epoch.retain()
    }
}

impl std::fmt::Debug for FacadeMarkExclusion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FacadeMarkExclusion")
            .field("epoch", &self.epoch)
            .finish()
    }
}

/// An exclusive, serialized-owner loan admitted before any concurrent start.
#[must_use = "begin consumes the admission into the concurrent capture extent"]
pub struct ConcurrentMarkPermit<'heap> {
    heap: &'heap mut TaggedHeap,
    heap_identity: HeapIdentity,
    _owner: PhantomData<Rc<()>>,
}

/// A begun, unlaunched concurrent cycle with exclusive capture access.
///
/// Dropping this loan leaves the started cycle and its gray work intact. Its
/// owner must explicitly drain that cycle before starting another collection.
#[must_use = "launch the captured cycle or explicitly drain its residual work"]
pub struct ConcurrentMarkCapture<'heap> {
    heap: &'heap mut TaggedHeap,
    heap_identity: HeapIdentity,
    _owner: PhantomData<Rc<()>>,
}

/// A complete, stopped-owner mark-and-sweep drain of this heap.
#[must_use = "consume this loan to acquire facade marking exclusion"]
pub struct QuiescentCollector<'heap> {
    heap: &'heap mut TaggedHeap,
    _owner: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(ConcurrentMarkPermit<'static>: Send, Sync, Copy, Clone);
static_assertions::assert_not_impl_any!(ConcurrentMarkCapture<'static>: Send, Sync, Copy, Clone);
static_assertions::assert_not_impl_any!(QuiescentCollector<'static>: Send, Sync, Copy, Clone);
static_assertions::assert_impl_all!(ConcurrentMarkPermit<'static>: std::fmt::Debug);
static_assertions::assert_impl_all!(ConcurrentMarkCapture<'static>: std::fmt::Debug);
static_assertions::assert_impl_all!(QuiescentCollector<'static>: std::fmt::Debug);

// Only this module can create these proofs. Collector implementation methods
// require one even though their storage operations live in the sibling module.
#[repr(transparent)]
#[derive(Debug)]
pub(super) struct ConcurrentMarkStageProof(HeapIdentity);

static_assertions::assert_impl_all!(ConcurrentMarkStageProof: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(ConcurrentMarkStageProof: Copy, Clone);
const _: () = {
    assert!(std::mem::size_of::<ConcurrentMarkStageProof>() == std::mem::size_of::<HeapIdentity>());
    assert!(
        std::mem::align_of::<ConcurrentMarkStageProof>() == std::mem::align_of::<HeapIdentity>()
    );
    assert!(std::mem::offset_of!(ConcurrentMarkStageProof, 0) == 0);
};

impl ConcurrentMarkStageProof {
    pub(super) fn matches(&self, heap: &TaggedHeap) -> bool {
        self.0 == heap.heap_identity()
    }
}

impl<'heap> ConcurrentMarkPermit<'heap> {
    pub fn arm_first_cycle(&mut self) {
        let proof = ConcurrentMarkStageProof(self.heap_identity);
        self.heap.raw_arm_first_cycle_concurrent(&proof);
    }

    pub fn begin(self) -> ConcurrentMarkCapture<'heap> {
        let proof = ConcurrentMarkStageProof(self.heap_identity);
        self.heap.raw_concurrent_begin(&proof);
        ConcurrentMarkCapture {
            heap: self.heap,
            heap_identity: self.heap_identity,
            _owner: PhantomData,
        }
    }
}

impl ConcurrentMarkCapture<'_> {
    pub fn heap_mut(&mut self) -> &mut TaggedHeap {
        self.heap
    }

    pub fn launch(self) -> Result<(), ConcurrentMarkAdmissionError> {
        let actual = self.heap.heap_identity();
        if self.heap_identity != actual {
            return Err(ConcurrentMarkAdmissionError::ForeignHeap {
                expected: self.heap_identity,
                actual,
            });
        }
        self.heap.check_concurrent_capture()?;
        let proof = ConcurrentMarkStageProof(self.heap_identity);
        self.heap.raw_launch_concurrent_mark(&proof);
        Ok(())
    }

    #[cfg(test)]
    /// # Safety
    /// The same serialized fixture owner admitted this heap's unlaunched
    /// start, and still excludes its obarray and TLS writers without callbacks.
    pub(super) unsafe fn resume_for_test(
        heap: &mut TaggedHeap,
    ) -> Result<ConcurrentMarkCapture<'_>, ConcurrentMarkAdmissionError> {
        heap.check_concurrent_capture()?;
        let heap_identity = heap.heap_identity();
        Ok(ConcurrentMarkCapture {
            heap,
            heap_identity,
            _owner: PhantomData,
        })
    }
}

impl QuiescentCollector<'_> {
    pub fn heap_identity(&self) -> HeapIdentity {
        self.heap.heap_identity()
    }

    pub fn exclude_concurrent_mark(self) -> Result<FacadeMarkExclusion, FacadeMarkExclusionError> {
        let state = self.heap.facade_mark_state_or_install();
        state.retained.retain()?;
        Ok(FacadeMarkExclusion {
            epoch: FacadeEpochRetention { state },
            _owner: PhantomData,
        })
    }
}

macro_rules! heap_loan_debug {
    ($ty:ident) => {
        impl std::fmt::Debug for $ty<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("heap", &self.heap.heap_identity())
                    .finish()
            }
        }
    };
}
heap_loan_debug!(ConcurrentMarkPermit);
heap_loan_debug!(ConcurrentMarkCapture);
heap_loan_debug!(QuiescentCollector);

impl TaggedHeap {
    pub(super) fn facade_mark_state_or_install(&mut self) -> Arc<FacadeMarkState> {
        let heap = self.heap_identity();
        let carrier = self
            .census
            .get_or_insert_with(|| Box::new(GenCensus::disabled()));
        Arc::clone(carrier.facade_mark.get_or_insert_with(|| {
            Arc::new(FacadeMarkState {
                heap,
                retained: FacadeRetentionCount(AtomicUsize::new(0)),
            })
        }))
    }

    /// Cold per-cycle admission check; never used by allocation or TLS polls.
    pub fn facade_mark_is_excluded(&self) -> bool {
        self.census
            .as_deref()
            .and_then(|carrier| carrier.facade_mark.as_deref())
            .is_some_and(|state| state.retained.is_held())
    }

    /// Admit the sole serialized owner before any blackening or capture.
    ///
    /// # Safety
    /// The caller excludes every writer of this heap, its obarray and installed
    /// TLS aliases. Owner transfer synchronizes prior facade acquisitions and
    /// releases. No Lisp callback, allocation safepoint, nested collection or
    /// independent launcher runs through this loan. This serial admission, not
    /// the counter load alone, excludes a concurrent exclusion acquisition.
    pub unsafe fn permit_concurrent_mark(
        &mut self,
    ) -> Result<ConcurrentMarkPermit<'_>, ConcurrentMarkAdmissionError> {
        if self.facade_mark_is_excluded() {
            return Err(ConcurrentMarkAdmissionError::Excluded(self.heap_identity()));
        }
        if self.concurrent_mark_running
            || self.mark_in_progress
            || self.sweep_in_progress
            || !self.gray_queue.is_empty()
            || self.pending_obarray_scan.is_some()
        {
            return Err(ConcurrentMarkAdmissionError::CollectorBusy);
        }
        let heap_identity = self.heap_identity();
        Ok(ConcurrentMarkPermit {
            heap: self,
            heap_identity,
            _owner: PhantomData,
        })
    }

    fn check_concurrent_capture(&self) -> Result<(), ConcurrentMarkAdmissionError> {
        if self.facade_mark_is_excluded() {
            return Err(ConcurrentMarkAdmissionError::Excluded(self.heap_identity()));
        }
        if !self.mark_in_progress || self.concurrent_mark_running || self.sweep_in_progress {
            return Err(ConcurrentMarkAdmissionError::NotStarted);
        }
        Ok(())
    }

    /// Finish an existing cycle completely, then lend the quiescent collector.
    /// Joining the worker alone never creates this capability.
    ///
    /// # Safety
    /// The caller holds the closed sole-writer admission for this heap and its
    /// matching obarray, including legacy TLS aliases. No concurrent start loan,
    /// synchronous collection extent, callback or safepoint is outstanding.
    /// If a mark is active, `reseed` must enumerate ALL current external roots
    /// (including every registered owner and all new symbol cells) for this
    /// heap; it may only seed roots, with no Lisp execution, nested collection
    /// or allocation safepoint. Internal roots are reseeded here. The admission
    /// and heap lifetime remain valid until this returned loan is consumed.
    /// The existing sweep may invoke native finalizers: the caller must also
    /// exclude their reentry or mutation of this heap throughout this drain.
    pub unsafe fn drain_to_quiescent(
        &mut self,
        reseed: impl FnOnce(&mut TaggedHeap),
    ) -> Result<QuiescentCollector<'_>, CollectorQuiescenceError> {
        if self.gc_locks_poisoned() {
            return Err(CollectorQuiescenceError::PoisonedCollectorState);
        }
        let had_cycle =
            self.concurrent_mark_running || self.mark_in_progress || self.sweep_in_progress;
        if self.first_cycle_concurrent && !had_cycle {
            // Dropping an armed but unbegun permit has traced nothing; it
            // cannot turn the mapped image permanent by minting quiescence.
            return Err(CollectorQuiescenceError::PendingPartition);
        }
        self.finish_concurrent_mark()?;
        if self.mark_in_progress {
            self.reseed_runtime_and_remembered_roots();
            reseed(self);
            // A worker finish already folded these queues. An unpublished
            // start can also hold owner-fed SATB work; fold it under the same
            // stopped-owner proof instead of leaving a queue behind.
            let (satb, deferred) = {
                let mut satb = self
                    .satb_shared
                    .lock()
                    .map_err(|_| CollectorQuiescenceError::PoisonedCollectorState)?;
                let mut deferred = self
                    .deferred_veclikes
                    .lock()
                    .map_err(|_| CollectorQuiescenceError::PoisonedCollectorState)?;
                (std::mem::take(&mut *satb), std::mem::take(&mut *deferred))
            };
            self.gray_queue
                .extend(satb.into_iter().map(MarkWord::value));
            self.gray_queue
                .extend(deferred.into_iter().map(MarkWord::value));
            // A full obarray reseed in the owner's callback covers this range;
            // no old start boundary is allowed to escape into the next cycle.
            self.take_concurrent_obarray_start_slots();
            // A dropped capture may have staged snapshots without publishing
            // a job. Successful finish above proves there is no worker, and
            // the full owner reseed covers symbol cells. Mapped children that
            // the unpublished first-partition job never scanned must also be
            // seeded before any sweep or image blackening can happen.
            if self.staged_mapped_cons_scan.is_some() || self.staged_mapped_veclikes.is_some() {
                self.seed_all_mapped_children();
            }
            self.pending_obarray_scan = None;
            self.staged_mapped_cons_scan = None;
            self.staged_mapped_veclikes = None;
            let bytes_before = self.live_bytes();
            let pause = std::time::Instant::now();
            self.incremental_drain_all();
            self.incremental_finish(bytes_before, pause);
        }
        if self.sweep_in_progress {
            self.finish_incremental_sweep_now();
        }
        self.finish_first_partition_cycle();
        self.close_alloc_regions();
        if self.concurrent_mark_running
            || self.mark_in_progress
            || self.sweep_in_progress
            || self.generational.major_in_progress
            || !self.sweep_noncons_pending.is_null()
            || !self.generational.old_sweep_pending.is_null()
        {
            return Err(CollectorQuiescenceError::CollectorBusy);
        }
        if !self.gray_queue.is_empty() {
            return Err(CollectorQuiescenceError::ResidualMark);
        }
        if !self
            .satb_shared
            .lock()
            .map_err(|_| CollectorQuiescenceError::PoisonedCollectorState)?
            .is_empty()
            || !self
                .deferred_veclikes
                .lock()
                .map_err(|_| CollectorQuiescenceError::PoisonedCollectorState)?
                .is_empty()
        {
            return Err(CollectorQuiescenceError::ResidualMark);
        }
        if self.pending_obarray_scan.is_some()
            || self.concurrent_obarray_start_slots.is_some()
            || self.staged_mapped_cons_scan.is_some()
            || self.staged_mapped_veclikes.is_some()
            || self.concurrent_hash_snapshot().is_some()
            || self.gc_exited.is_some()
            || !self.retired_vector_buffers.is_empty()
        {
            return Err(CollectorQuiescenceError::PendingSnapshot);
        }
        if self.first_cycle_concurrent {
            return Err(CollectorQuiescenceError::PendingPartition);
        }
        Ok(QuiescentCollector {
            heap: self,
            _owner: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiescent(heap: &mut TaggedHeap) -> QuiescentCollector<'_> {
        // SAFETY: each fixture owns its heap directly without obarray/TLS
        // aliases or callbacks; this helper is used only when it has no
        // external roots or its previous collection has already completed.
        unsafe { heap.drain_to_quiescent(|_| {}) }.unwrap()
    }

    fn admit(
        heap: &mut TaggedHeap,
    ) -> Result<ConcurrentMarkPermit<'_>, ConcurrentMarkAdmissionError> {
        // SAFETY: these direct Rust fixtures are the serialized heap owner;
        // worker threads below own only atomic retention metadata.
        unsafe { heap.permit_concurrent_mark() }
    }

    #[test]
    fn dormant_admission_does_not_install_a_cold_carrier() {
        let mut heap = knobs::with_concurrent_claims_for_test(false, TaggedHeap::new);
        let had_carrier = heap.census.is_some();
        drop(admit(&mut heap).unwrap());
        drop(quiescent(&mut heap));
        assert_eq!(heap.census.is_some(), had_carrier);
        assert!(!heap.facade_mark_is_excluded());
        assert!(heap.census.as_ref().is_none_or(|c| c.facade_mark.is_none()));
    }

    #[test]
    fn dropped_armed_permit_cannot_blacken_before_a_completed_cycle() {
        let mut heap = TaggedHeap::new();
        let image_child = heap.alloc_float(8.25);
        let image = Box::into_raw(Box::new(ConsCell {
            car: image_child,
            cdr_or_next: crate::tagged::header::ConsCdrOrNext {
                cdr: TaggedValue::NIL,
            },
        }));
        // SAFETY: the leaked aligned initialized cell acts as a writable
        // mapped image and remains live longer than this heap. The fixture is
        // its only writer and the image points at this heap's live child.
        unsafe { heap.register_mapped_cons_range(image, 1) };
        let mut permit = admit(&mut heap).unwrap();
        permit.arm_first_cycle();
        drop(permit);
        // SAFETY: no cycle was begun and no writer, worker or callback exists.
        // The registered image is the complete internal root source here.
        assert!(matches!(
            unsafe { heap.drain_to_quiescent(|_| {}) },
            Err(CollectorQuiescenceError::PendingPartition)
        ));
        assert!(!heap.dump_blackened);
        assert!(!heap.value_is_tenured(image_child));
        assert!(!heap.mark_in_progress && !heap.sweep_in_progress);
        assert!(heap.staged_mapped_cons_scan.is_none());
        assert!(heap.staged_mapped_veclikes.is_none());
        assert!(!heap.facade_mark_is_excluded());

        // Re-admission remains possible. Dropping this now-begun capture
        // exercises the full synchronous recovery of its unpublished image
        // staging; the image supplies its child's reachability.
        drop(admit(&mut heap).unwrap().begin());
        assert!(heap.staged_mapped_cons_scan.is_some());
        // SAFETY: the start loan has ended without launching a worker. The
        // sole owner has no external roots beyond the registered image graph.
        let idle = unsafe { heap.drain_to_quiescent(|_| {}) }.unwrap();
        let exclusion = idle.exclude_concurrent_mark().unwrap();
        assert!(heap.dump_blackened);
        assert!(heap.value_is_tenured(image_child));
        assert_eq!(image_child.xfloat(), 8.25);
        assert!(!heap.first_cycle_concurrent);
        assert!(!heap.mark_in_progress && !heap.sweep_in_progress);
        assert!(heap.staged_mapped_cons_scan.is_none());
        assert!(heap.staged_mapped_veclikes.is_none());
        drop(exclusion);
    }

    #[test]
    fn held_exclusion_rejects_before_blackening_or_start_capture() {
        let mut heap = TaggedHeap::new();
        let exclusion = quiescent(&mut heap).exclude_concurrent_mark().unwrap();
        let parity = heap.mark_parity;
        let starts = heap.handshake.start_count;
        assert!(matches!(
            admit(&mut heap),
            Err(ConcurrentMarkAdmissionError::Excluded(_))
        ));
        assert_eq!(heap.mark_parity, parity);
        assert_eq!(heap.handshake.start_count, starts);
        assert!(!heap.mark_in_progress && !heap.concurrent_mark_running);
        assert!(!heap.first_cycle_concurrent && !heap.dump_blackened);
        assert!(heap.pending_obarray_scan.is_none());
        assert!(heap.staged_mapped_cons_scan.is_none());
        assert!(heap.staged_mapped_veclikes.is_none());
        drop(exclusion);
        drop(admit(&mut heap).unwrap());
    }

    #[test]
    fn foreign_heap_exclusion_is_rejected() {
        let mut owner = TaggedHeap::new();
        let mut foreign = TaggedHeap::new();
        let exclusion = quiescent(&mut owner).exclude_concurrent_mark().unwrap();
        exclusion.validate(&owner).unwrap();
        assert!(matches!(
            exclusion.validate(&foreign),
            Err(FacadeMarkExclusionError::ForeignHeap { .. })
        ));
        assert!(!foreign.facade_mark_is_excluded());
        drop(admit(&mut foreign).unwrap());
    }

    #[test]
    fn replacing_capture_heap_cannot_publish_under_old_admission() {
        let mut heap = TaggedHeap::new();
        let mut foreign = TaggedHeap::new();
        let admitted_identity = heap.heap_identity();
        let root = heap.alloc_float(8.75);
        let mut capture = admit(&mut heap).unwrap().begin();
        capture.heap_mut().seed_root(root);
        std::mem::swap(capture.heap_mut(), &mut foreign);
        assert!(
            matches!(capture.launch(), Err(ConcurrentMarkAdmissionError::ForeignHeap { expected, .. }) if expected == admitted_identity)
        );
        assert!(!heap.concurrent_mark_running);
        assert_eq!(heap.handshake.start_count, 0);
        assert_eq!(foreign.heap_identity(), admitted_identity);
        // SAFETY: the swapped-out admitted heap is still exclusively owned;
        // root is its complete root set, and no worker was ever published.
        drop(unsafe { foreign.drain_to_quiescent(|heap| heap.seed_root(root)) }.unwrap());
    }

    #[test]
    fn last_reader_release_controls_concurrent_admission() {
        let mut heap = TaggedHeap::new();
        let exclusion = quiescent(&mut heap).exclude_concurrent_mark().unwrap();
        let reader = exclusion.retain().unwrap();
        let final_reader = reader.retain().unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            wait.recv().unwrap();
            drop(final_reader);
        });
        drop(exclusion);
        drop(reader);
        assert!(matches!(
            admit(&mut heap),
            Err(ConcurrentMarkAdmissionError::Excluded(_))
        ));
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(!heap.facade_mark_is_excluded());
        drop(admit(&mut heap).unwrap());
    }

    #[test]
    fn dropped_capture_drains_residual_mark_and_entire_sweep() {
        for generational in [false, true] {
            let mut heap = TaggedHeap::new();
            heap.generational = generational::GenState::new(generational);
            let child = heap.alloc_float(9.5);
            let root = heap.alloc_cons(child, TaggedValue::NIL);
            let garbage = heap.alloc_float(10.5);
            let garbage_address = garbage.as_float_ptr().unwrap().cast::<u8>();
            let mut capture = admit(&mut heap).unwrap().begin();
            capture.heap_mut().seed_root(root);
            drop(capture);
            assert!(heap.mark_in_progress);
            assert!(!heap.concurrent_mark_running);
            assert!(!heap.gray_queue.is_empty());
            let mut reseeded = false;
            // SAFETY: root is this heap's complete external root set. The
            // start loan has ended; no worker, writer alias or callback exists.
            let idle = unsafe {
                heap.drain_to_quiescent(|heap| {
                    reseeded = true;
                    heap.seed_root(root);
                })
            }
            .unwrap();
            assert!(reseeded);
            let exclusion = idle.exclude_concurrent_mark().unwrap();
            assert!(!heap.mark_in_progress && !heap.sweep_in_progress);
            assert!(!heap.concurrent_mark_running);
            assert!(heap.gray_queue.is_empty());
            assert!(!heap.owns_non_cons_object(garbage_address));
            assert!(heap.owns_non_cons_object(child.as_float_ptr().unwrap().cast::<u8>()));
            assert_eq!(root.cons_car(), child);
            assert_eq!(child.xfloat(), 9.5);
            drop(exclusion);
        }
    }

    #[test]
    fn joined_worker_still_requires_residual_mark_and_sweep() {
        let mut heap = TaggedHeap::new();
        let root = heap.alloc_float(11.5);
        let mut capture = admit(&mut heap).unwrap().begin();
        capture.heap_mut().seed_root(root);
        capture.launch().unwrap();
        heap.finish_concurrent_mark().unwrap();
        assert!(!heap.concurrent_mark_running);
        assert!(heap.mark_in_progress);
        assert!(matches!(
            admit(&mut heap),
            Err(ConcurrentMarkAdmissionError::CollectorBusy)
        ));
        // SAFETY: this owner has joined its sole worker; root is its complete
        // external root set and the closure cannot execute Lisp or allocate.
        let idle = unsafe { heap.drain_to_quiescent(|heap| heap.seed_root(root)) }.unwrap();
        drop(idle);
        assert!(!heap.mark_in_progress && !heap.sweep_in_progress);
        assert!(heap.gray_queue.is_empty());
        assert_eq!(root.xfloat(), 11.5);
    }

    #[test]
    fn dropped_capture_clears_its_unpublished_obarray_snapshot_after_reseed() {
        let mut heap = TaggedHeap::new();
        let mut obarray = crate::emacs_core::symbol::Obarray::new();
        let root = heap.alloc_float(11.75);
        obarray.set_symbol_value("facade-unpublished-root", root);
        let mut capture = admit(&mut heap).unwrap().begin();
        let snapshot = {
            // SAFETY: this fixture exclusively owns heap and obarray without
            // TLS aliases or callbacks. No job is published before capture Drop.
            let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(capture.heap_mut()) };
            obarray.scan_snapshot(&world)
        };
        capture.heap_mut().set_pending_obarray_scan(snapshot);
        drop(capture);
        assert!(heap.pending_obarray_scan.is_some());
        // SAFETY: no worker was launched. The obarray cell is this fixture's
        // complete external root set; the callback seeds it without a safepoint.
        let idle = unsafe { heap.drain_to_quiescent(|heap| heap.seed_root(root)) }.unwrap();
        let exclusion = idle.exclude_concurrent_mark().unwrap();
        assert!(heap.pending_obarray_scan.is_none());
        assert!(heap.concurrent_obarray_start_slots.is_none());
        assert!(!heap.mark_in_progress && !heap.sweep_in_progress);
        assert_eq!(root.xfloat(), 11.75);
        drop(exclusion);
    }

    #[test]
    fn census_history_drain_preserves_facade_retention() {
        knobs::set_census_mode_for_test(Some(knobs::CensusMode::Survivors));
        let mut heap = TaggedHeap::new();
        let root = heap.alloc_float(11.875);
        let exclusion = quiescent(&mut heap).exclude_concurrent_mark().unwrap();
        let state = Arc::as_ptr(heap.census.as_ref().unwrap().facade_mark.as_ref().unwrap());
        for _ in 0..2 {
            heap.collect_exact(std::iter::once(root));
            assert!(heap.last_census_for_test().is_some());
            assert_eq!(
                Arc::as_ptr(heap.census.as_ref().unwrap().facade_mark.as_ref().unwrap()),
                state
            );
            assert!(heap.facade_mark_is_excluded());
            assert!(matches!(
                admit(&mut heap),
                Err(ConcurrentMarkAdmissionError::Excluded(_))
            ));
        }
        drop(exclusion);
        assert!(!heap.facade_mark_is_excluded());
        knobs::set_census_mode_for_test(None);
    }

    #[test]
    fn sweep_continuation_is_fully_drained_before_exclusion() {
        let mut heap = TaggedHeap::new();
        let root = heap.alloc_float(12.5);
        let mut capture = admit(&mut heap).unwrap().begin();
        capture.heap_mut().seed_root(root);
        drop(capture);
        heap.incremental_drain_all();
        heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
        assert!(!heap.mark_in_progress && heap.sweep_in_progress);
        assert!(matches!(
            admit(&mut heap),
            Err(ConcurrentMarkAdmissionError::CollectorBusy)
        ));
        let exclusion = quiescent(&mut heap).exclude_concurrent_mark().unwrap();
        assert!(!heap.sweep_in_progress);
        assert_eq!(root.xfloat(), 12.5);
        drop(exclusion);
    }

    #[test]
    fn orphan_gray_work_cannot_mint_quiescence() {
        let mut heap = TaggedHeap::new();
        let root = heap.alloc_float(13.5);
        heap.seed_root(root);
        // SAFETY: the fixture has no active collection or external writer;
        // intentionally pending work must be rejected rather than swept.
        assert!(matches!(
            unsafe { heap.drain_to_quiescent(|_| {}) },
            Err(CollectorQuiescenceError::ResidualMark)
        ));
        assert!(!heap.facade_mark_is_excluded());
        assert!(!heap.gray_queue.is_empty());
    }

    #[test]
    fn idle_drop_with_retention_keeps_backing_without_waiting() {
        let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
        let mut heap = TaggedHeap::new();
        heap.alloc_float(14.5);
        let exclusion = quiescent(&mut heap).exclude_concurrent_mark().unwrap();
        let reader = exclusion.retain().unwrap();
        let state = Arc::downgrade(&reader.state);
        let wake = Arc::clone(&heap.gc_wake);
        let _wake_held = wake.0.lock().unwrap();
        drop(exclusion);
        drop(heap);
        // This allocation counter proves storage retention without reading a
        // dangling object pointer. Holding the wake lock would deadlock any
        // accidental waiting destructor path.
        assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 1);
        assert!(state.upgrade().is_some());
        drop(reader);
        assert!(state.upgrade().is_none());
    }

    #[test]
    fn explicit_shutdown_cannot_reclaim_facade_retained_backing() {
        let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
        let mut heap = TaggedHeap::new();
        heap.alloc_float(15.5);
        let exclusion = quiescent(&mut heap).exclude_concurrent_mark().unwrap();
        assert!(matches!(
            heap.shutdown(),
            Err(MarkFinishError::FacadeRetained(_))
        ));
        assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 1);
        drop(exclusion);
    }
}
