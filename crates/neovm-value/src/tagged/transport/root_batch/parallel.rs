//! Owner-side admission and stopped readers for the parallel rendezvous.
//!
//! The legacy PreparedRootBatch::finalize still requires a sole mutator. This
//! separate path accepts only a sealed owner witness containing already-owned
//! SharedRoots. It installs ordinary traceable roots in the existing table and
//! retains a second immutable all-word payload for opaque stopped-state scans.
//! Neither publication nor these readers grant heap dereference authority.

use super::*;
use crate::tagged::gc::{MutatorKey, OwnerRootAdmission, StoppedRootAdmission};

/// Metadata, not authority. The key includes World, heap and registration
/// identity. Publication epoch may be zero for native entry before any request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ParallelRootIdentity {
    key: MutatorKey,
    publication_generation: NonZeroU64,
    publication_epoch: u64,
}

impl ParallelRootIdentity {
    pub(crate) fn new(
        key: MutatorKey,
        publication_generation: NonZeroU64,
        publication_epoch: u64,
    ) -> Self {
        Self {
            key,
            publication_generation,
            publication_epoch,
        }
    }

    pub(crate) fn key(self) -> MutatorKey {
        self.key
    }

    pub(crate) fn heap_identity(self) -> HeapIdentity {
        self.key.heap_identity()
    }

    pub(crate) fn publication_generation(self) -> NonZeroU64 {
        self.publication_generation
    }

    pub(crate) fn publication_epoch(self) -> u64 {
        self.publication_epoch
    }
}

/// Parallel admission failures are separate from the legacy error enum: adding
/// variants there can change niche/layout decisions in dormant callers.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ParallelRootBatchError {
    #[error(transparent)]
    Transport(#[from] RootBatchError),
    #[error("parallel root publication identity does not match its admission")]
    Identity,
    #[error("publication epoch {publication} is not covered by stopped epoch {current}")]
    StopEpoch { publication: u64, current: u64 },
}

#[derive(Debug)]
struct ParallelRootPayload {
    identity: ParallelRootIdentity,
    words: Box<[usize]>,
    used: usize,
}

/// Every byte and the root-table slot are reserved before final fill. Prepared
/// storage is confined to the original owner, just like the serial reservation.
#[derive(Debug)]
#[must_use = "publish with the original owner's witness or cancel the reservation"]
pub(crate) struct PreparedParallelRootBatch {
    identity: ParallelRootIdentity,
    prepared: PreparedRootBatch,
    payload: Arc<ParallelRootPayload>,
}

impl PreparedParallelRootBatch {
    #[cold]
    #[inline(never)]
    pub(crate) fn reserve(
        identity: ParallelRootIdentity,
        epoch: FacadeEpochRetention,
        capacity: usize,
    ) -> Result<Self, ParallelRootBatchError> {
        let heap = identity.heap_identity();
        if epoch.heap_identity() != heap {
            return Err(RootBatchError::ForeignHeap {
                owner: epoch.heap_identity(),
                reader: heap,
            }
            .into());
        }
        let mut opaque = Vec::new();
        opaque
            .try_reserve_exact(capacity)
            .map_err(RootBatchError::from)?;
        opaque.resize(capacity, 0);
        let mut traced = Vec::new();
        traced
            .try_reserve_exact(capacity)
            .map_err(RootBatchError::from)?;
        traced.resize(capacity, None);
        let traced = Arc::new(ImmutableRootBatch {
            heap,
            words: traced.into_boxed_slice(),
            used: 0,
        });
        let payload = Arc::new(ParallelRootPayload {
            identity,
            words: opaque.into_boxed_slice(),
            used: 0,
        });
        let table = table_for(heap);
        let control = {
            let mut cells = table.lock();
            cells
                .batches
                .get_or_insert_with(BatchSlots::default)
                .reserve()?
        };
        Ok(Self {
            identity,
            prepared: PreparedRootBatch {
                reservation: BatchReservation {
                    ownership: Some(BatchOwnership {
                        table,
                        control,
                        epoch,
                    }),
                    _confined: PhantomData,
                },
                payload: traced,
                _confined: PhantomData,
            },
            payload,
        })
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn publish(
        mut self,
        admission: &OwnerRootAdmission<'_>,
    ) -> Result<ParallelRootBatchLease, ParallelRootBatchPublishFailure> {
        // The witness is constructed only by the original owner's checked
        // publication transition. It proves rooted inputs and prevents that
        // owner from running until the rendezvous accepts or rejects its ACK.
        // Unlike serial finalize, it does not assert that peers are stopped.
        let result = (|| {
            if admission.identity() != self.identity || self.payload.identity != self.identity {
                return Err(ParallelRootBatchError::Identity);
            }
            let roots = admission.roots();
            if roots.len() > self.payload.words.len() {
                return Err(RootBatchError::Capacity {
                    required: roots.len(),
                    capacity: self.payload.words.len(),
                }
                .into());
            }
            for root in roots {
                if root.heap_identity() != self.identity.heap_identity() {
                    return Err(RootBatchError::ForeignHeap {
                        owner: root.heap_identity(),
                        reader: self.identity.heap_identity(),
                    }
                    .into());
                }
            }
            let ownership = self
                .prepared
                .reservation
                .ownership
                .as_ref()
                .ok_or(RootBatchError::StaleReservation)?;
            if ownership.table.heap != self.identity.heap_identity()
                || ownership.epoch.heap_identity() != self.identity.heap_identity()
            {
                return Err(ParallelRootBatchError::Identity);
            }
            let cells = ownership.table.lock();
            cells
                .batches
                .as_ref()
                .ok_or(RootBatchError::StaleReservation)?
                .validate(&ownership.control)?;
            match ownership.control.state()? {
                SlotState::Reserved => {}
                SlotState::Live => return Err(RootBatchError::AlreadyPublished.into()),
                SlotState::Closing => return Err(RootBatchError::Closing.into()),
                SlotState::Retired => return Err(RootBatchError::Retired.into()),
                SlotState::Cancelled => return Err(RootBatchError::StaleReservation.into()),
            }
            if ownership.control.payload.get().is_some() {
                return Err(RootBatchError::AlreadyPublished.into());
            }
            let traced =
                Arc::get_mut(&mut self.prepared.payload).ok_or(RootBatchError::NonUniquePayload)?;
            let opaque = Arc::get_mut(&mut self.payload).ok_or(RootBatchError::NonUniquePayload)?;
            if traced.used != 0 || opaque.used != 0 || traced.words.len() < roots.len() {
                return Err(RootBatchError::AlreadyPublished.into());
            }
            // Every fallible check precedes mutation. No allocation, callback,
            // heap borrow, safepoint or handoff occurs through fill/publication.
            for root in roots {
                opaque.words[opaque.used] = root.parallel_word();
                opaque.used += 1;
                if let Some(word) = root.parallel_traced_word() {
                    traced.words[traced.used] = Some(word);
                    traced.used += 1;
                }
            }
            ownership
                .control
                .payload
                .set(Arc::clone(&self.prepared.payload))
                .expect("the reserved slot remains exclusive under its table lock");
            ownership.control.set_state(SlotState::Live);
            Ok(())
        })();
        if let Err(error) = result {
            return Err(ParallelRootBatchPublishFailure {
                prepared: self,
                error,
            });
        }
        Ok(ParallelRootBatchLease {
            identity: self.identity,
            lease: RootBatchLease {
                heap: self.identity.heap_identity(),
                roots: self.prepared.payload.used,
                ownership: self.prepared.reservation.ownership.take(),
            },
            payload: self.payload,
        })
    }
}

/// Owned immutable words plus the existing rooted registration/exclusion lease.
/// This carries no heap reference and is safe to place in the canonical entry.
#[derive(Debug)]
#[must_use = "explicit retirement retains roots on every refused attempt"]
pub(crate) struct ParallelRootBatchLease {
    identity: ParallelRootIdentity,
    lease: RootBatchLease,
    payload: Arc<ParallelRootPayload>,
}

impl ParallelRootBatchLease {
    pub(crate) fn identity(&self) -> ParallelRootIdentity {
        self.identity
    }

    pub(crate) fn word_count(&self) -> usize {
        self.payload.used
    }

    pub(crate) fn root_count(&self) -> usize {
        self.lease.root_count()
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn reader<'read>(
        &'read self,
        admission: &'read StoppedRootAdmission<'_>,
    ) -> Result<ParallelRootBatchReader<'read>, ParallelRootBatchError> {
        if admission.identity() != self.identity || self.payload.identity != self.identity {
            return Err(ParallelRootBatchError::Identity);
        }
        let current = admission.current_epoch();
        if current == 0 || self.identity.publication_epoch > current {
            return Err(ParallelRootBatchError::StopEpoch {
                publication: self.identity.publication_epoch,
                current,
            });
        }
        let ownership = self
            .lease
            .ownership
            .as_ref()
            .ok_or(RootBatchError::Retired)?;
        let cells = ownership.table.lock();
        cells
            .batches
            .as_ref()
            .ok_or(RootBatchError::StaleReservation)?
            .validate(&ownership.control)?;
        match ownership.control.state()? {
            // A stopped collector may still scan conservatively retained
            // Closing roots. Explicit counters protect it after this lock ends.
            SlotState::Live | SlotState::Closing => {}
            SlotState::Reserved | SlotState::Cancelled => {
                return Err(RootBatchError::NotPublished.into());
            }
            SlotState::Retired => return Err(RootBatchError::Retired.into()),
        }
        let epoch = ownership.epoch.retain().map_err(RootBatchError::from)?;
        ownership
            .control
            .readers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |readers| {
                readers.checked_add(1)
            })
            .map_err(|_| RootBatchError::ReaderCountExhausted)?;
        Ok(ParallelRootBatchReader {
            payload: Arc::clone(&self.payload),
            control: Arc::clone(&ownership.control),
            _epoch: epoch,
            obligation: ReadObligation::Active,
            _admission: PhantomData,
            _confined: PhantomData,
        })
    }

    /// Required even when a reader was forgotten and its Rust borrow ended.
    /// This is only an obligation census, never permission to resume by itself.
    pub(crate) fn validate_no_readers(&self) -> Result<(), ParallelRootBatchError> {
        let ownership = self
            .lease
            .ownership
            .as_ref()
            .ok_or(RootBatchError::Retired)?;
        let cells = ownership.table.try_lock().ok_or(RootBatchError::Busy)?;
        cells
            .batches
            .as_ref()
            .ok_or(RootBatchError::StaleReservation)?
            .validate(&ownership.control)?;
        let readers = ownership.control.readers.load(Ordering::Acquire);
        let abandoned = ownership.control.abandoned.load(Ordering::Acquire);
        if readers != 0 || abandoned != 0 {
            return Err(RootBatchError::ReadersOutstanding { readers, abandoned }.into());
        }
        Ok(())
    }

    /// The caller additionally proves restored-owner coverage or terminal
    /// quiescence, as required by the unchanged serial transport primitive.
    /// Failure returns this exact identity and payload for retry/retention.
    #[cold]
    #[inline(never)]
    pub(crate) fn try_finish(self) -> Result<RetiredRootBatch, ParallelRootBatchFinishFailure> {
        let Self {
            identity,
            lease,
            payload,
        } = self;
        match lease.try_finish() {
            Ok(retired) => Ok(retired),
            Err(failure) => {
                let (lease, error) = failure.into_parts();
                Err(ParallelRootBatchFinishFailure {
                    lease: Self {
                        identity,
                        lease,
                        payload,
                    },
                    error: error.into(),
                })
            }
        }
    }
}

/// A stopped, thread-confined opaque scan. It cannot materialize TaggedValue
/// or dereference heap objects; the sealed admission borrows AllStopped.
#[derive(Debug)]
#[must_use = "explicitly finish the last read; Drop records an unfinished obligation"]
pub(crate) struct ParallelRootBatchReader<'read> {
    payload: Arc<ParallelRootPayload>,
    control: Arc<BatchControl>,
    _epoch: FacadeEpochRetention,
    obligation: ReadObligation,
    _admission: PhantomData<&'read ()>,
    _confined: PhantomData<Rc<()>>,
}

impl ParallelRootBatchReader<'_> {
    pub(crate) fn append_words(&self, out: &mut Vec<usize>) {
        out.extend_from_slice(&self.payload.words[..self.payload.used]);
    }

    pub(crate) fn finish(mut self) -> Result<(), ParallelRootBatchError> {
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

impl Drop for ParallelRootBatchReader<'_> {
    fn drop(&mut self) {
        if self.obligation == ReadObligation::Active {
            // Match serial readers: abandonment is visible before the final
            // Release decrement that retirement observes with Acquire.
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

#[derive(Debug)]
#[must_use = "recover the prepared reservation or retain it on protocol failure"]
pub(crate) struct ParallelRootBatchPublishFailure {
    prepared: PreparedParallelRootBatch,
    error: ParallelRootBatchError,
}

impl ParallelRootBatchPublishFailure {
    pub(crate) fn error(&self) -> &ParallelRootBatchError {
        &self.error
    }

    pub(crate) fn into_parts(self) -> (PreparedParallelRootBatch, ParallelRootBatchError) {
        (self.prepared, self.error)
    }
}

#[derive(Debug)]
#[must_use = "recover the exact live lease after refused retirement"]
pub(crate) struct ParallelRootBatchFinishFailure {
    lease: ParallelRootBatchLease,
    error: ParallelRootBatchError,
}

impl ParallelRootBatchFinishFailure {
    pub(crate) fn error(&self) -> &ParallelRootBatchError {
        &self.error
    }

    pub(crate) fn into_parts(self) -> (ParallelRootBatchLease, ParallelRootBatchError) {
        (self.lease, self.error)
    }
}

static_assertions::assert_impl_all!(ParallelRootIdentity: Send, Sync, Copy, fmt::Debug);
static_assertions::assert_impl_all!(ParallelRootPayload: Send, Sync, fmt::Debug);
static_assertions::assert_impl_all!(ParallelRootBatchLease: Send, Sync, fmt::Debug);
static_assertions::assert_impl_all!(ParallelRootBatchError: Send, Sync, std::error::Error);
static_assertions::assert_not_impl_any!(PreparedParallelRootBatch: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ParallelRootBatchLease: Clone, Copy);
static_assertions::assert_not_impl_any!(ParallelRootBatchReader<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
