//! Grouped, immutable roots in the shared-root table.
//!
//! Preparation reserves all storage and one specific table slot. Filling admits
//! live same-heap words on the original mutator; publishing never reallocates.
//! A lease owns root registration and held mark exclusion, not the heap itself.
//! Explicit finish retires it. Unfinished Drop retains both obligations.

use std::fmt;
use std::marker::PhantomData;
use std::num::{NonZeroU64, NonZeroUsize};
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use typed_index_collections::TiVec;

use super::root_table::{RootTable, TracedWord, table_for};
use crate::tagged::gc::{
    FacadeEpochRetention, FacadeMarkExclusion, FacadeMarkExclusionError, HeapIdentity, TaggedHeap,
};
use crate::tagged::value::TaggedValue;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatchSlotId(NonZeroUsize);

impl TryFrom<usize> for BatchSlotId {
    type Error = RootBatchError;

    fn try_from(index: usize) -> Result<Self, Self::Error> {
        index
            .checked_add(1)
            .and_then(NonZeroUsize::new)
            .map(Self)
            .ok_or(RootBatchError::SlotSpaceExhausted)
    }
}

impl From<BatchSlotId> for usize {
    fn from(id: BatchSlotId) -> Self {
        id.0.get() - 1
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatchGeneration(NonZeroU64);

static_assertions::assert_impl_all!(BatchSlotId: Send, Sync, Copy, fmt::Debug);
static_assertions::assert_impl_all!(BatchGeneration: Send, Sync, Copy, fmt::Debug);
const _: () = {
    assert!(std::mem::size_of::<Option<BatchSlotId>>() == std::mem::size_of::<BatchSlotId>());
    assert!(
        std::mem::size_of::<Option<BatchGeneration>>() == std::mem::size_of::<BatchGeneration>()
    );
    assert!(std::mem::offset_of!(BatchSlotId, 0) == 0);
    assert!(std::mem::offset_of!(BatchGeneration, 0) == 0);
    assert!(std::mem::size_of::<BatchSlotId>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<BatchSlotId>() == std::mem::align_of::<usize>());
    assert!(std::mem::size_of::<BatchGeneration>() == std::mem::size_of::<u64>());
    assert!(std::mem::align_of::<BatchGeneration>() == std::mem::align_of::<u64>());
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatchKey {
    slot: BatchSlotId,
    generation: BatchGeneration,
}

#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, num_enum::IntoPrimitive, num_enum::TryFromPrimitive,
)]
enum SlotState {
    Reserved,
    Live,
    Closing,
    Retired,
    Cancelled,
}

static_assertions::assert_impl_all!(BatchKey: Send, Sync, Copy, fmt::Debug);
static_assertions::assert_impl_all!(SlotState: Send, Sync, Copy, fmt::Debug);

/// A failed batch admission, read or retirement. Errors contain metadata only.
#[derive(Debug, thiserror::Error)]
pub enum RootBatchError {
    #[error("root batch exclusion admission failed: {0}")]
    Exclusion(#[from] FacadeMarkExclusionError),
    #[error("root batch of heap {owner:?} read on heap {reader:?}")]
    ForeignHeap {
        owner: HeapIdentity,
        reader: HeapIdentity,
    },
    #[error("root batch needs {required} words but reserved {capacity}")]
    Capacity { required: usize, capacity: usize },
    #[error("root batch generation space is exhausted")]
    GenerationExhausted,
    #[error("root batch slot space is exhausted")]
    SlotSpaceExhausted,
    #[error("root batch host storage allocation failed: {0}")]
    StorageAllocation(#[from] std::collections::TryReserveError),
    #[error("root batch table is busy; retirement must be retried")]
    Busy,
    #[error("root batch reservation no longer owns its slot")]
    StaleReservation,
    #[error("root batch prepared storage is not uniquely owned")]
    NonUniquePayload,
    #[error("root batch was already published")]
    AlreadyPublished,
    #[error("root batch is not published")]
    NotPublished,
    #[error("root batch is closing and admits no new readers")]
    Closing,
    #[error("root batch is already retired")]
    Retired,
    #[error(
        "root batch has {readers} active readers and {abandoned} unfinished reader obligations"
    )]
    ReadersOutstanding { readers: usize, abandoned: usize },
    #[error("root batch reader count is exhausted")]
    ReaderCountExhausted,
    #[error("root batch reader completion is inconsistent")]
    ReaderCompletion,
    #[error("root batch slot has invalid state {state}")]
    InvalidState { state: u8 },
}

static_assertions::assert_impl_all!(RootBatchError: Send, Sync, fmt::Debug, std::error::Error);

/// Fully initialized storage: unused capacity contains None, not raw values or
/// uninitialized memory. Only the unique prepared owner can fill the prefix.
struct ImmutableRootBatch {
    heap: HeapIdentity,
    words: Box<[Option<TracedWord>]>,
    used: usize,
}

static_assertions::assert_impl_all!(ImmutableRootBatch: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(ImmutableRootBatch: Clone);

impl fmt::Debug for ImmutableRootBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImmutableRootBatch")
            .field("heap", &self.heap)
            .field("used", &self.used)
            .field("capacity", &self.words.len())
            .finish()
    }
}

impl ImmutableRootBatch {
    /// Called only after the table key or reader admission has checked the heap.
    /// The borrow supplies the live owning heap for this scoped decoding.
    fn append_roots(&self, heap: &TaggedHeap, out: &mut Vec<TaggedValue>) {
        debug_assert_eq!(self.heap, heap.heap_identity());
        out.extend(
            self.words[..self.used]
                .iter()
                .filter_map(|word| word.map(TracedWord::value)),
        );
    }
}

struct BatchControl {
    key: BatchKey,
    state: AtomicU8,
    payload: OnceLock<Arc<ImmutableRootBatch>>,
    readers: AtomicUsize,
    abandoned: AtomicUsize,
}

static_assertions::assert_impl_all!(BatchControl: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(BatchControl: Clone);

impl BatchControl {
    fn state(&self) -> Result<SlotState, RootBatchError> {
        let state = self.state.load(Ordering::Acquire);
        SlotState::try_from(state).map_err(|_| RootBatchError::InvalidState { state })
    }

    fn set_state(&self, state: SlotState) {
        self.state.store(state.into(), Ordering::Release);
    }
}

impl fmt::Debug for BatchControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchControl")
            .field("key", &self.key)
            .field("state", &self.state())
            .field("readers", &self.readers.load(Ordering::Acquire))
            .field("abandoned", &self.abandoned.load(Ordering::Acquire))
            .finish()
    }
}

/// Lazily installed in the existing table allocator, under its existing lock.
/// Each reused position receives a new immutable control and generation.
#[derive(Default)]
pub(super) struct BatchSlots {
    slots: TiVec<BatchSlotId, Arc<BatchControl>>,
    next_generation: u64,
}

static_assertions::assert_impl_all!(BatchSlots: Send, Sync, fmt::Debug);

impl fmt::Debug for BatchSlots {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchSlots")
            .field("slots", &self.slots.len())
            .field("next_generation", &self.next_generation)
            .finish()
    }
}

impl BatchSlots {
    fn reserve(&mut self) -> Result<Arc<BatchControl>, RootBatchError> {
        let next = self
            .next_generation
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(RootBatchError::GenerationExhausted)?;
        let reusable = self
            .slots
            .iter()
            .find(|slot| {
                matches!(slot.state(), Ok(SlotState::Retired | SlotState::Cancelled))
                    && slot.readers.load(Ordering::Acquire) == 0
                    && slot.abandoned.load(Ordering::Acquire) == 0
            })
            .map(|slot| slot.key.slot);
        let id = match reusable {
            Some(id) => id,
            None => BatchSlotId::try_from(self.slots.len())?,
        };
        if reusable.is_none() {
            self.slots.try_reserve(1)?;
        }
        let slot = Arc::new(BatchControl {
            key: BatchKey {
                slot: id,
                generation: BatchGeneration(next),
            },
            state: AtomicU8::new(SlotState::Reserved.into()),
            payload: OnceLock::new(),
            readers: AtomicUsize::new(0),
            abandoned: AtomicUsize::new(0),
        });
        // All allocations and checks precede the externally visible reservation.
        if let Some(index) = reusable {
            self.slots[index] = Arc::clone(&slot);
        } else {
            self.slots.push(Arc::clone(&slot));
        }
        self.next_generation = next.get();
        Ok(slot)
    }

    fn validate(&self, control: &Arc<BatchControl>) -> Result<(), RootBatchError> {
        let current = self
            .slots
            .get(control.key.slot)
            .ok_or(RootBatchError::StaleReservation)?;
        if current.key != control.key || !Arc::ptr_eq(current, control) {
            return Err(RootBatchError::StaleReservation);
        }
        Ok(())
    }

    /// Closing roots stay visible. This is a synchronous, nonescaping table
    /// read rather than admission of an explicit `RootBatchReader`. Holding
    /// the allocator lock retains the immutable storage for the whole walk;
    /// retirement cannot remove it until the read finishes and releases that
    /// lock. Closing only forbids new readers that escape this lock.
    pub(super) fn collect_roots(&self, heap: &TaggedHeap, out: &mut Vec<TaggedValue>) {
        for slot in &self.slots {
            match slot.state() {
                Ok(SlotState::Live | SlotState::Closing) | Err(_) => {
                    // Invalid metadata conservatively retains any installed
                    // roots; it is never a reason to omit them from collection.
                    if let Some(payload) = slot.payload.get() {
                        payload.append_roots(heap, out);
                    }
                }
                Ok(SlotState::Reserved | SlotState::Retired | SlotState::Cancelled) => {}
            }
        }
    }
}

#[derive(Debug)]
struct BatchOwnership {
    table: Arc<RootTable>,
    control: Arc<BatchControl>,
    epoch: FacadeEpochRetention,
}

static_assertions::assert_impl_all!(BatchOwnership: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(BatchOwnership: Clone);

#[derive(Debug)]
struct BatchReservation {
    ownership: Option<BatchOwnership>,
    _confined: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(BatchReservation: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(BatchReservation: fmt::Debug);

impl Drop for BatchReservation {
    fn drop(&mut self) {
        let Some(ownership) = self.ownership.take() else {
            return;
        };
        match ownership.control.state() {
            Ok(SlotState::Reserved) => ownership.control.set_state(SlotState::Cancelled),
            Ok(SlotState::Cancelled | SlotState::Retired) => {}
            Ok(SlotState::Live | SlotState::Closing) | Err(_) => {
                // A partially committed/inconsistent reservation may own the
                // only registered root. Preserve table AND exclusion without
                // locking, allocating, invoking Lisp or destroying the heap.
                std::mem::forget(ownership);
            }
        }
    }
}

/// Unique preallocated storage and a specific reserved table slot.
#[must_use = "prepare and publish the batch or cancel its reservation"]
pub struct PreparedRootBatch {
    reservation: BatchReservation,
    payload: Arc<ImmutableRootBatch>,
    _confined: PhantomData<Rc<()>>,
}

/// Frozen same-heap words; only this stage can publish a batch.
#[must_use = "publish the frozen batch before transferring mutator authority"]
pub struct FinalizedRootBatch {
    reservation: BatchReservation,
    payload: Arc<ImmutableRootBatch>,
    _confined: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(PreparedRootBatch: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(FinalizedRootBatch: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(PreparedRootBatch: fmt::Debug);
static_assertions::assert_impl_all!(FinalizedRootBatch: fmt::Debug);

impl fmt::Debug for PreparedRootBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedRootBatch")
            .field("payload", &self.payload)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for FinalizedRootBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FinalizedRootBatch")
            .field("payload", &self.payload)
            .finish_non_exhaustive()
    }
}

impl PreparedRootBatch {
    /// Reserve every host allocation and a stable table position before the
    /// owner's reverse exchange. This proves exclusion, not Value provenance.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Part B will admit registry publications through this prepared constructor"
        )
    )]
    pub(crate) fn reserve(
        heap: &TaggedHeap,
        exclusion: &FacadeMarkExclusion,
        capacity: usize,
    ) -> Result<Self, RootBatchError> {
        exclusion.validate(heap)?;
        let mut words = Vec::new();
        words.try_reserve_exact(capacity)?;
        words.resize(capacity, None);
        let payload = Arc::new(ImmutableRootBatch {
            heap: heap.heap_identity(),
            words: words.into_boxed_slice(),
            used: 0,
        });
        let epoch = exclusion.retain()?;
        let table = table_for(heap.heap_identity());
        let control = {
            let mut cells = table.lock();
            cells
                .batches
                .get_or_insert_with(BatchSlots::default)
                .reserve()?
        };
        Ok(Self {
            reservation: BatchReservation {
                ownership: Some(BatchOwnership {
                    table,
                    control,
                    epoch,
                }),
                _confined: PhantomData,
            },
            payload,
            _confined: PhantomData,
        })
    }

    /// Admit the owner's postexchange census without allocating or calling Lisp.
    ///
    /// # Safety
    /// The original owner is this heap's admitted sole mutator. Every traced
    /// input is live and belongs to this heap. Keep every input rooted until
    /// publication completes; no collection, callback, safepoint or ownership
    /// handoff may occur during this fill/publish phase. The held exclusion
    /// prevents concurrent marking but does not inhibit synchronous collection.
    /// The heap must outlive all root-table scans and reader materializations.
    /// Part B must additionally require its completed reverse-exchange witness.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Part B will finalize roots under its owner admission proof"
        )
    )]
    pub(crate) unsafe fn finalize(
        mut self,
        values: &[TaggedValue],
    ) -> Result<FinalizedRootBatch, RootBatchFinalizeFailure> {
        if values.len() > self.payload.words.len() {
            let error = RootBatchError::Capacity {
                required: values.len(),
                capacity: self.payload.words.len(),
            };
            return Err(RootBatchFinalizeFailure {
                prepared: self,
                error,
            });
        }
        let Some(payload) = Arc::get_mut(&mut self.payload) else {
            return Err(RootBatchFinalizeFailure {
                prepared: self,
                error: RootBatchError::NonUniquePayload,
            });
        };
        for word in values.iter().copied().filter_map(TracedWord::of) {
            payload.words[payload.used] = Some(word);
            payload.used += 1;
        }
        Ok(FinalizedRootBatch {
            reservation: self.reservation,
            payload: self.payload,
            _confined: PhantomData,
        })
    }
}

impl FinalizedRootBatch {
    /// Commit the immutable allocation to its reserved slot, with Release
    /// publication. Errors preserve this stage for original-owner recovery.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Part B will publish the registry's finalized root batches"
        )
    )]
    pub(crate) fn publish(mut self) -> Result<RootBatchLease, RootBatchPublishFailure> {
        let commit = (|| {
            let ownership = self
                .reservation
                .ownership
                .as_ref()
                .ok_or(RootBatchError::StaleReservation)?;
            let cells = ownership.table.lock();
            cells
                .batches
                .as_ref()
                .ok_or(RootBatchError::StaleReservation)?
                .validate(&ownership.control)?;
            match ownership.control.state()? {
                SlotState::Reserved => {}
                SlotState::Live => return Err(RootBatchError::AlreadyPublished),
                SlotState::Closing => return Err(RootBatchError::Closing),
                SlotState::Retired => return Err(RootBatchError::Retired),
                SlotState::Cancelled => return Err(RootBatchError::StaleReservation),
            }
            ownership
                .control
                .payload
                .set(Arc::clone(&self.payload))
                .map_err(|_| RootBatchError::AlreadyPublished)?;
            ownership.control.set_state(SlotState::Live);
            Ok(())
        })();
        if let Err(error) = commit {
            return Err(RootBatchPublishFailure {
                finalized: self,
                error,
            });
        }
        Ok(RootBatchLease {
            heap: self.payload.heap,
            roots: self.payload.used,
            ownership: self.reservation.ownership.take(),
        })
    }
}

/// Ownership of one rooted batch and its held exclusion. This does not own the
/// Lisp heap. Transfer carries opaque roots, never a raw Value or World view.
#[must_use = "explicitly finish the batch; unfinished Drop retains its roots and exclusion"]
pub struct RootBatchLease {
    heap: HeapIdentity,
    roots: usize,
    ownership: Option<BatchOwnership>,
}

static_assertions::assert_impl_all!(RootBatchLease: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(RootBatchLease: Clone, Copy);

impl RootBatchLease {
    pub fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }
    pub fn root_count(&self) -> usize {
        self.roots
    }

    /// Admit one confined reader borrowing both this registration and the
    /// owning heap. No new reader can enter after closing starts.
    pub fn reader<'r>(
        &'r self,
        heap: &'r TaggedHeap,
    ) -> Result<RootBatchReader<'r>, RootBatchError> {
        let ownership = self.ownership.as_ref().ok_or(RootBatchError::Retired)?;
        RootBatchReader::admit(ownership, heap)
    }

    /// Nonblocking close and retirement after explicit reader completion.
    /// Contention or an unfinished reader retains registration for retry.
    /// The facade registry must
    /// additionally require restored/terminated-owner and quiescent-collector
    /// proofs before invoking this transport primitive; it mints neither.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Part B will retire publications with restoration and quiescence proofs"
        )
    )]
    pub(crate) fn try_finish(mut self) -> Result<RetiredRootBatch, RootBatchFinishFailure> {
        let result = (|| {
            let ownership = self.ownership.as_ref().ok_or(RootBatchError::Retired)?;
            let cells = ownership.table.try_lock().ok_or(RootBatchError::Busy)?;
            cells
                .batches
                .as_ref()
                .ok_or(RootBatchError::StaleReservation)?
                .validate(&ownership.control)?;
            match ownership.control.state()? {
                SlotState::Live => ownership.control.set_state(SlotState::Closing),
                SlotState::Closing => {}
                SlotState::Retired => return Err(RootBatchError::Retired),
                SlotState::Reserved | SlotState::Cancelled => {
                    return Err(RootBatchError::NotPublished);
                }
            }
            // Read count first: observing the final reader's Release decrement
            // also observes an abandoned Drop's preceding counter increment.
            let readers = ownership.control.readers.load(Ordering::Acquire);
            let abandoned = ownership.control.abandoned.load(Ordering::Acquire);
            if readers != 0 || abandoned != 0 {
                return Err(RootBatchError::ReadersOutstanding { readers, abandoned });
            }
            ownership.control.set_state(SlotState::Retired);
            Ok(())
        })();
        if let Err(error) = result {
            return Err(RootBatchFinishFailure { lease: self, error });
        }
        let retired = RetiredRootBatch {
            heap: self.heap,
            roots: self.roots,
        };
        // Host payload may remain in an inert slot, but owns no epoch. Releasing
        // these obligations cannot block or invoke native/heap destruction.
        drop(self.ownership.take());
        Ok(retired)
    }
}

impl Drop for RootBatchLease {
    fn drop(&mut self) {
        if let Some(ownership) = self.ownership.take() {
            // No implicit unrooting, last-Arc table destruction, or release of
            // exclusion while a registered batch remains. Part B's registry
            // retains whole heap backing independently; this retains transport.
            std::mem::forget(ownership);
        }
    }
}

impl fmt::Debug for RootBatchLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RootBatchLease")
            .field("heap", &self.heap)
            .field("roots", &self.roots)
            .finish_non_exhaustive()
    }
}

/// An admitted read that cannot outlive its registration or heap borrow.
#[must_use = "finish the reader explicitly; Drop leaves an unfinished obligation"]
pub struct RootBatchReader<'r> {
    heap: &'r TaggedHeap,
    payload: Arc<ImmutableRootBatch>,
    control: Arc<BatchControl>,
    _epoch: FacadeEpochRetention,
    obligation: ReadObligation,
    _lease: PhantomData<&'r RootBatchLease>,
    _confined: PhantomData<Rc<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadObligation {
    Active,
    Finished,
}

static_assertions::assert_impl_all!(ReadObligation: Send, Sync, Copy, fmt::Debug);

static_assertions::assert_not_impl_any!(RootBatchReader<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(RootBatchReader<'static>: fmt::Debug);

impl<'r> RootBatchReader<'r> {
    fn admit(ownership: &'r BatchOwnership, heap: &'r TaggedHeap) -> Result<Self, RootBatchError> {
        if ownership.table.heap != heap.heap_identity() {
            return Err(RootBatchError::ForeignHeap {
                owner: ownership.table.heap,
                reader: heap.heap_identity(),
            });
        }
        let cells = ownership.table.lock();
        cells
            .batches
            .as_ref()
            .ok_or(RootBatchError::StaleReservation)?
            .validate(&ownership.control)?;
        match ownership.control.state()? {
            SlotState::Live => {}
            SlotState::Closing => return Err(RootBatchError::Closing),
            SlotState::Retired => return Err(RootBatchError::Retired),
            SlotState::Reserved | SlotState::Cancelled => return Err(RootBatchError::NotPublished),
        }
        let payload = Arc::clone(
            ownership
                .control
                .payload
                .get()
                .ok_or(RootBatchError::NotPublished)?,
        );
        let epoch = ownership.epoch.retain()?;
        ownership
            .control
            .readers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |readers| {
                readers.checked_add(1)
            })
            .map_err(|_| RootBatchError::ReaderCountExhausted)?;
        Ok(Self {
            heap,
            payload,
            control: Arc::clone(&ownership.control),
            _epoch: epoch,
            obligation: ReadObligation::Active,
            _lease: PhantomData,
            _confined: PhantomData,
        })
    }

    /// Copy roots into the owning collector's local work input. The returned
    /// raw values remain confined and need ordinary rooting beyond this read.
    pub fn append_roots(&self, out: &mut Vec<TaggedValue>) {
        self.payload.append_roots(self.heap, out);
    }

    /// Complete the last scoped read. Consuming self prevents any later read.
    pub fn finish(mut self) -> Result<(), RootBatchError> {
        self.control
            .readers
            .fetch_update(Ordering::Release, Ordering::Relaxed, |readers| {
                readers.checked_sub(1)
            })
            .map_err(|_| RootBatchError::ReaderCompletion)?;
        self.obligation = ReadObligation::Finished;
        Ok(())
    }
}

impl Drop for RootBatchReader<'_> {
    fn drop(&mut self) {
        if self.obligation == ReadObligation::Active {
            // A caller cannot silently substitute Drop for explicit finish.
            // Increment abandonment before the Release decrement that a finish
            // attempt observes with Acquire; no lock, callback or panic.
            // Exhaustion stays nonzero instead of wrapping into a false
            // "all readers finished" result. An exhausted slot is retained.
            let _ = self.control.abandoned.fetch_update(
                Ordering::Release,
                Ordering::Relaxed,
                |abandoned| abandoned.checked_add(1),
            );
            let _ = self.control.readers.fetch_update(
                Ordering::Release,
                Ordering::Relaxed,
                |readers| readers.checked_sub(1),
            );
        }
    }
}

impl fmt::Debug for RootBatchReader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RootBatchReader")
            .field("heap", &self.payload.heap)
            .field("roots", &self.payload.used)
            .field("obligation", &self.obligation)
            .finish()
    }
}

/// Finished metadata; no storage, exclusion, decoding or materialization API.
#[derive(Clone, Copy, Debug)]
pub struct RetiredRootBatch {
    heap: HeapIdentity,
    roots: usize,
}

static_assertions::assert_impl_all!(RetiredRootBatch: Send, Sync, Copy, fmt::Debug);

impl RetiredRootBatch {
    pub fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }
    pub fn root_count(&self) -> usize {
        self.roots
    }
}

#[derive(Debug, thiserror::Error)]
#[error("root batch finalization failed: {error}")]
pub struct RootBatchFinalizeFailure {
    prepared: PreparedRootBatch,
    #[source]
    error: RootBatchError,
}

#[derive(Debug, thiserror::Error)]
#[error("root batch publication failed: {error}")]
pub struct RootBatchPublishFailure {
    finalized: FinalizedRootBatch,
    #[source]
    error: RootBatchError,
}

#[derive(Debug, thiserror::Error)]
#[error("root batch retirement failed: {error}")]
pub struct RootBatchFinishFailure {
    lease: RootBatchLease,
    #[source]
    error: RootBatchError,
}

static_assertions::assert_not_impl_any!(RootBatchFinalizeFailure: Send, Sync);
static_assertions::assert_not_impl_any!(RootBatchPublishFailure: Send, Sync);
static_assertions::assert_impl_all!(RootBatchFinalizeFailure: fmt::Debug, std::error::Error);
static_assertions::assert_impl_all!(RootBatchPublishFailure: fmt::Debug, std::error::Error);
static_assertions::assert_impl_all!(RootBatchFinishFailure: Send, Sync, fmt::Debug, std::error::Error);

impl RootBatchFinalizeFailure {
    pub fn error(&self) -> &RootBatchError {
        &self.error
    }
    pub fn into_parts(self) -> (PreparedRootBatch, RootBatchError) {
        (self.prepared, self.error)
    }
}
impl RootBatchPublishFailure {
    pub fn error(&self) -> &RootBatchError {
        &self.error
    }
    pub fn into_parts(self) -> (FinalizedRootBatch, RootBatchError) {
        (self.finalized, self.error)
    }
}
impl RootBatchFinishFailure {
    pub fn error(&self) -> &RootBatchError {
        &self.error
    }
    pub fn into_parts(self) -> (RootBatchLease, RootBatchError) {
        (self.lease, self.error)
    }
}

#[cfg(test)]
#[path = "root_batch_tests.rs"]
mod tests;
