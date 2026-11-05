//! Young-block payloads in the canonical cold SATB registration registry.
//!
//! The registry stays owner-confined. Only an owned young-only loan may move
//! to a worker; SATB buffers and hot MutatorGcState never leave their owner.
//! A lost loan deliberately leaves an outstanding count and blocks collection.
//! Allocator pending IDs are an O(young) work index, not a second registry.

use std::cell::Cell;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
use typed_index_collections::TiVec;

use super::HeapIdentity;
use super::cons_allocator::ConsBlockBase;
use super::mutator_gc::MutatorSatbSlot;
use super::scan_contract::StoppedMutatorsProof;

/// Identifies one canonical registration in one existing heap lifetime.
/// No constructor accepts an arbitrary ordinal or issues another heap identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct YoungRegistrationId {
    heap: HeapIdentity,
    ordinal: NonZeroUsize,
}

// Keep parallel registrations disjoint from the legacy current/test ordinals.
// Issuance is process-wide and never rolls back or reuses an ordinal, including
// after a failed start or an era's retirement. This counter carries identity
// only; registration/payload publication has its own synchronization.
const PARALLEL_ORDINAL_TAG: usize = 1usize << (usize::BITS - 1);
static NEXT_PARALLEL_ORDINAL: AtomicUsize = AtomicUsize::new(PARALLEL_ORDINAL_TAG);

/// Parallel admission failure stays separate from the legacy error enum: an
/// added legacy variant changes the success niche of its allocation Results.
#[derive(Debug)]
pub(super) struct ParallelYoungIdExhausted;

static_assertions::assert_impl_all!(YoungRegistrationId: Send, Sync, Copy);

impl YoungRegistrationId {
    #[inline]
    pub(super) fn heap_identity(self) -> HeapIdentity {
        self.heap
    }

    #[cold]
    #[inline(never)]
    pub(super) fn issue_parallel(heap: HeapIdentity) -> Result<Self, ParallelYoungIdExhausted> {
        // MAX remains the exhausted sentinel; wrapping would alias an old ID.
        let ordinal = NEXT_PARALLEL_ORDINAL
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| ParallelYoungIdExhausted)?;
        Ok(Self {
            heap,
            ordinal: NonZeroUsize::new(ordinal).expect("parallel ordinals have their high bit set"),
        })
    }

    #[inline]
    pub(super) fn is_parallel(self) -> bool {
        self.ordinal.get() >= PARALLEL_ORDINAL_TAG
    }
}

// Outbound indexing only: no conversion can mint an arbitrary registration ID.
#[cfg(test)]
impl From<YoungRegistrationId> for usize {
    fn from(id: YoungRegistrationId) -> Self {
        id.ordinal.get() - 1
    }
}

/// An opaque position in AllocatorState's pending registration work index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct YoungPendingIndex(NonZeroUsize);

impl YoungPendingIndex {
    pub(super) fn index(self) -> usize {
        self.0.get() - 1
    }
}

/// An opaque position in one canonical registration's block log.
/// The allocator stores it together with that registration's ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct YoungBlockIndex(NonZeroUsize);

impl YoungBlockIndex {
    pub(super) fn index(self) -> usize {
        self.0.get() - 1
    }
}

#[derive(Debug)]
#[must_use = "repair the moved block backlink before unregistering its predecessor"]
pub(super) struct YoungLogRemoval {
    pub(super) removed: ConsBlockBase,
    pub(super) moved: Option<ConsBlockBase>,
}

/// Owned only by a canonical cold entry or its unique outstanding loan.
/// Payload operations never clear block flags: only ConsStopAll may do that.
#[derive(Debug)]
pub(super) struct YoungBlockLog {
    id: YoungRegistrationId,
    blocks: Vec<ConsBlockBase>,
    pending: Option<YoungPendingIndex>,
}

static_assertions::assert_impl_all!(YoungBlockLog: Send);
static_assertions::assert_not_impl_any!(YoungBlockLog: Clone, Copy);

impl YoungBlockLog {
    #[inline]
    pub(super) fn id(&self) -> YoungRegistrationId {
        self.id
    }

    #[inline]
    pub(super) fn blocks(&self) -> &[ConsBlockBase] {
        &self.blocks
    }

    #[cfg(feature = "gc-memory-telemetry")]
    pub(super) fn allocation_bytes(&self) -> usize {
        self.blocks.capacity() * std::mem::size_of::<ConsBlockBase>()
    }

    #[inline]
    pub(super) fn pending_index(&self) -> Option<YoungPendingIndex> {
        self.pending
    }

    /// Append after the allocator's false-to-true flag winner. The allocator
    /// records this returned index in the block's registration/index backlink.
    #[must_use = "record the returned typed position in the block backlink"]
    pub(super) fn push(&mut self, base: ConsBlockBase) -> YoungBlockIndex {
        let index = YoungBlockIndex(
            self.blocks
                .len()
                .checked_add(1)
                .and_then(NonZeroUsize::new)
                .expect("young log position exhausted"),
        );
        self.blocks.push(base);
        index
    }

    /// O(1) removal. Repair the moved block's backlink before removing the
    /// removed block's base-to-metadata mapping, while both bases are valid.
    pub(super) fn swap_remove(&mut self, index: YoungBlockIndex) -> YoungLogRemoval {
        let removed = self.blocks.swap_remove(index.index());
        YoungLogRemoval {
            removed,
            moved: self.blocks.get(index.index()).copied(),
        }
    }

    /// First nonempty log insertion queues its canonical ID exactly once.
    /// This work index lives under the allocator's existing Sole/Mutex access.
    pub(super) fn ensure_pending(&mut self, pending: &mut Vec<YoungRegistrationId>) {
        if self.pending.is_none() {
            assert!(!self.blocks.is_empty(), "cannot queue an empty young log");
            let index = YoungPendingIndex(
                pending
                    .len()
                    .checked_add(1)
                    .and_then(NonZeroUsize::new)
                    .expect("pending young registration position exhausted"),
            );
            pending.push(self.id);
            self.pending = Some(index);
        }
    }

    /// Clear this log's pending index and return the ID moved into its slot.
    /// Release this log borrow before resolving that ID to repair its index.
    pub(super) fn remove_pending(
        &mut self,
        pending: &mut Vec<YoungRegistrationId>,
    ) -> Option<YoungRegistrationId> {
        assert!(
            self.blocks.is_empty(),
            "pending removal requires a drained log"
        );
        let index = self.pending.take()?;
        assert_eq!(
            pending[index.index()],
            self.id,
            "young pending backlink mismatch"
        );
        pending.swap_remove(index.index());
        pending.get(index.index()).copied()
    }

    /// The old position is supplied by the ID removed from the same work index.
    pub(super) fn repair_pending_index(&mut self, index: YoungPendingIndex) {
        assert!(self.pending.is_some(), "moved pending log was not pending");
        self.pending = Some(index);
    }

    /// Ordinary capture drains into the allocator's frozen worklist while
    /// retaining this owner's Vec capacity for the next minor. End the Drain
    /// borrow before remove_pending; consuming the last pending ID avoids any
    /// moved-owner repair during capture.
    #[must_use = "fully transfer these bases into frozen work before removing pending membership"]
    pub(super) fn drain_blocks(&mut self) -> std::vec::Drain<'_, ConsBlockBase> {
        self.blocks.drain(..)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct YoungLoanGeneration(NonZeroUsize);

#[derive(Debug)]
enum YoungLogState {
    Resident(YoungBlockLog),
    Loaned(YoungLoanGeneration),
}

/// This field travels with the very same cold owner as SATB during test-owner
/// switching, retirement and stopped-world enumeration. No separate owner map.
#[derive(Debug)]
pub(super) struct YoungLogSlot {
    id: YoungRegistrationId,
    state: YoungLogState,
    next_generation: YoungLoanGeneration,
}

impl YoungLogSlot {
    #[inline]
    pub(super) fn id(&self) -> YoungRegistrationId {
        self.id
    }

    #[cold]
    #[inline(never)]
    pub(super) fn new_parallel(id: YoungRegistrationId) -> Self {
        assert!(
            id.is_parallel(),
            "parallel slot requires a parallel registration"
        );
        Self {
            id,
            state: YoungLogState::Resident(YoungBlockLog {
                id,
                blocks: Vec::new(),
                pending: None,
            }),
            next_generation: YoungLoanGeneration(NonZeroUsize::MIN),
        }
    }

    /// The canonical Mutator's sealed publication transition owns this unique
    /// borrow and makes the empty shell inaccessible until checked restoration.
    /// This is not a worker loan and does not change outstanding-loan counters.
    /// Existing worker loans are refused before moving any payload.
    #[cold]
    #[inline(never)]
    pub(super) fn take_owner_payload(&mut self) -> Result<YoungBlockLog, YoungRegistryError> {
        let id = self.id;
        let resident = self.resident_mut()?;
        Ok(std::mem::replace(
            resident,
            YoungBlockLog {
                id,
                blocks: Vec::new(),
                pending: None,
            },
        ))
    }

    /// Validate the original identity and empty owner shell before committing.
    /// A rejected return owns the unchanged payload so the caller can retry or
    /// retain it while poisoning the era; no young flag/backlink is discarded.
    #[cold]
    #[inline(never)]
    pub(super) fn restore_owner_payload(
        &mut self,
        log: YoungBlockLog,
    ) -> Result<(), (YoungRegistryError, YoungBlockLog)> {
        let refusal = if log.id.heap != self.id.heap {
            Some(YoungRegistryError::WrongHeap)
        } else if log.id != self.id {
            Some(YoungRegistryError::UnknownRegistration)
        } else {
            match &self.state {
                YoungLogState::Loaned(_) => Some(YoungRegistryError::LogLoaned),
                YoungLogState::Resident(shell)
                    if !shell.blocks.is_empty() || shell.pending.is_some() =>
                {
                    Some(YoungRegistryError::LogNotLoaned)
                }
                YoungLogState::Resident(_) => None,
            }
        };
        if let Some(reason) = refusal {
            return Err((reason, log));
        }
        self.state = YoungLogState::Resident(log);
        Ok(())
    }

    /// Inventory covers canonical resident payloads. An outstanding loan owns
    /// its Vec outside the registry and is deliberately excluded until return;
    /// a stopped-world inventory cannot have such loans after preflight.
    #[cfg(feature = "gc-memory-telemetry")]
    pub(super) fn resident_allocation_bytes(&self) -> usize {
        match &self.state {
            YoungLogState::Resident(log) => log.allocation_bytes(),
            YoungLogState::Loaned(_) => 0,
        }
    }

    #[inline]
    pub(super) fn resident(&self) -> Result<&YoungBlockLog, YoungRegistryError> {
        match &self.state {
            YoungLogState::Resident(log) => Ok(log),
            YoungLogState::Loaned(_) => Err(YoungRegistryError::LogLoaned),
        }
    }

    #[inline]
    pub(super) fn resident_mut(&mut self) -> Result<&mut YoungBlockLog, YoungRegistryError> {
        match &mut self.state {
            YoungLogState::Resident(log) => Ok(log),
            YoungLogState::Loaned(_) => Err(YoungRegistryError::LogLoaned),
        }
    }
}

/// Unique transferable ownership of young payload ONLY. Dropping this does not
/// silently return, publish, clear flags or decrement the outstanding counter.
/// A dropped/panicking worker therefore makes later collection fail closed.
#[derive(Debug)]
#[must_use = "return this owned young log to its canonical registration before collection"]
pub(super) struct YoungLogLoan {
    log: YoungBlockLog,
    generation: YoungLoanGeneration,
    _not_sync: PhantomData<Cell<()>>,
}

static_assertions::assert_impl_all!(YoungLogLoan: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(YoungLogLoan: Sync, Clone, Copy);

impl YoungLogLoan {
    #[inline]
    pub(super) fn id(&self) -> YoungRegistrationId {
        self.log.id
    }

    #[inline]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(super) fn log_mut(&mut self) -> &mut YoungBlockLog {
        &mut self.log
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum YoungRegistryError {
    #[error("young log belongs to another heap")]
    WrongHeap,
    #[error("unknown canonical young registration")]
    UnknownRegistration,
    #[error("young registration is retired")]
    Retired,
    #[error("young log is loaned to a worker")]
    LogLoaned,
    #[error("young log is already resident")]
    LogNotLoaned,
    #[error("young loan generation mismatch")]
    GenerationMismatch,
    #[error("young loan generation space exhausted")]
    GenerationExhausted,
    #[cfg(test)]
    #[error("young registration ordinal space exhausted")]
    RegistrationExhausted,
    #[error("young outstanding loan count exhausted")]
    LoanCountExhausted,
    #[error("young loan count invariant violated")]
    LoanCountInvariant,
    #[error("collection requires all {0} young loans returned")]
    OutstandingLoans(usize),
}

/// Failure owns the original loan, so a rejected cross-heap return can retry
/// against the correct registry without losing the payload or its authority.
#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub(super) struct YoungLogReturnError {
    reason: YoungRegistryError,
    loan: YoungLogLoan,
}

impl YoungLogReturnError {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(super) fn reason(&self) -> YoungRegistryError {
        self.reason
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Young-only worker transfer remains a production capability; current production uses a confined mutator."
        )
    )]
    pub(super) fn into_loan(self) -> YoungLogLoan {
        self.loan
    }

    pub(super) fn new(reason: YoungRegistryError, loan: YoungLogLoan) -> Self {
        Self { reason, loan }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum YoungRegistrationLocation {
    Current,
    #[cfg(test)]
    Registered(YoungRegisteredIndex),
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) struct YoungRegisteredIndex(NonZeroUsize);

#[cfg(test)]
impl YoungRegisteredIndex {
    pub(super) fn index(self) -> usize {
        self.0.get() - 1
    }
}

/// One control record inside the existing boxed canonical registry. It reuses
/// the heap's identity; it never issues a new atomic identity or admission bit.
pub(super) struct YoungRegistryControl {
    heap: HeapIdentity,
    #[cfg(test)]
    locations: TiVec<YoungRegistrationId, YoungRegistrationLocation>,
    outstanding_loans: usize,
}

impl std::fmt::Debug for YoungRegistryControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TiVec's derived Debug requires an inbound usize -> ID conversion;
        // IDs deliberately have no such authority-creating conversion.
        let mut debug = f.debug_struct("YoungRegistryControl");
        debug.field("heap", &self.heap);
        #[cfg(test)]
        debug.field("locations", &self.locations.raw);
        debug
            .field("outstanding_loans", &self.outstanding_loans)
            .finish()
    }
}

impl YoungRegistryControl {
    #[inline]
    pub(super) fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }

    /// Read-only owner-confined preflight; no proof is needed to refuse
    /// shutdown or retain storage while another thread owns a young loan.
    #[inline]
    pub(super) fn has_outstanding_loans(&self) -> bool {
        self.outstanding_loans != 0
    }

    pub(super) fn new(heap: HeapIdentity) -> (Self, YoungLogSlot) {
        #[cfg(test)]
        let locations = {
            let mut locations = TiVec::new();
            locations.raw.push(YoungRegistrationLocation::Current);
            locations
        };
        let control = Self {
            heap,
            #[cfg(test)]
            locations,
            outstanding_loans: 0,
        };
        let current = control.make_slot(NonZeroUsize::MIN);
        (control, current)
    }

    fn make_slot(&self, ordinal: NonZeroUsize) -> YoungLogSlot {
        let id = YoungRegistrationId {
            heap: self.heap,
            ordinal,
        };
        YoungLogSlot {
            id,
            state: YoungLogState::Resident(YoungBlockLog {
                id,
                blocks: Vec::new(),
                pending: None,
            }),
            next_generation: YoungLoanGeneration(NonZeroUsize::MIN),
        }
    }

    #[cfg(test)]
    pub(super) fn register(&mut self) -> Result<YoungLogSlot, YoungRegistryError> {
        let ordinal = self
            .locations
            .raw
            .len()
            .checked_add(1)
            .and_then(NonZeroUsize::new)
            .filter(|ordinal| ordinal.get() < PARALLEL_ORDINAL_TAG)
            .ok_or(YoungRegistryError::RegistrationExhausted)?;
        let slot = self.make_slot(ordinal);
        self.locations
            .raw
            .push(YoungRegistrationLocation::Registered(YoungRegisteredIndex(
                NonZeroUsize::new(ordinal.get() - 1).expect("noncurrent ordinal"),
            )));
        Ok(slot)
    }

    pub(super) fn location(
        &self,
        id: YoungRegistrationId,
    ) -> Result<YoungRegistrationLocation, YoungRegistryError> {
        if id.heap != self.heap {
            return Err(YoungRegistryError::WrongHeap);
        }
        #[cfg(test)]
        {
            self.locations
                .raw
                .get(usize::from(id))
                .copied()
                .ok_or(YoungRegistryError::UnknownRegistration)
        }
        #[cfg(not(test))]
        {
            // Production currently has exactly the legacy current owner. A
            // future production K registry must adopt the canonical location
            // index below together with its actual registration machinery.
            if id.ordinal == NonZeroUsize::MIN {
                Ok(YoungRegistrationLocation::Current)
            } else {
                Err(YoungRegistryError::UnknownRegistration)
            }
        }
    }

    /// Update the O(1) canonical-ID -> physical-owner index alongside the exact
    /// same whole hot/cold swaps. This also runs in the infallible unwind guard.
    /// Registry APIs never remove entries; checks make this total even if that
    /// restriction changes. No allocation, callback, lock or assertion in Drop.
    #[cfg(test)]
    pub(super) fn swap_locations(&mut self, left: YoungRegistrationId, right: YoungRegistrationId) {
        let left = usize::from(left);
        let right = usize::from(right);
        if left < self.locations.raw.len() && right < self.locations.raw.len() {
            self.locations.raw.swap(left, right);
        }
    }

    #[cfg(feature = "gc-memory-telemetry")]
    pub(super) fn allocation_bytes(&self) -> usize {
        #[cfg(test)]
        {
            self.locations.raw.capacity() * std::mem::size_of::<YoungRegistrationLocation>()
        }
        #[cfg(not(test))]
        {
            0
        }
    }

    pub(super) fn ensure_stopped(&self) -> Result<(), YoungRegistryError> {
        if self.outstanding_loans == 0 {
            Ok(())
        } else {
            Err(YoungRegistryError::OutstandingLoans(self.outstanding_loans))
        }
    }

    pub(super) fn take_loan(
        &mut self,
        slot: &mut YoungLogSlot,
    ) -> Result<YoungLogLoan, YoungRegistryError> {
        self.location(slot.id)?;
        slot.resident()?;
        let generation = slot.next_generation;
        let next = generation
            .0
            .get()
            .checked_add(1)
            .and_then(NonZeroUsize::new)
            .ok_or(YoungRegistryError::GenerationExhausted)?;
        let outstanding = self
            .outstanding_loans
            .checked_add(1)
            .ok_or(YoungRegistryError::LoanCountExhausted)?;
        // All fallible validation precedes moving the payload. No allocation or
        // callback occurs between replacing the state and updating the counter.
        let log = match std::mem::replace(&mut slot.state, YoungLogState::Loaned(generation)) {
            YoungLogState::Resident(log) => log,
            YoungLogState::Loaned(_) => unreachable!("resident preflight"),
        };
        slot.next_generation = YoungLoanGeneration(next);
        self.outstanding_loans = outstanding;
        Ok(YoungLogLoan {
            log,
            generation,
            _not_sync: PhantomData,
        })
    }

    pub(super) fn return_loan(
        &mut self,
        slot: &mut YoungLogSlot,
        loan: YoungLogLoan,
    ) -> Result<(), YoungLogReturnError> {
        let validation = (|| {
            self.location(loan.id())?;
            if loan.id() != slot.id {
                return Err(YoungRegistryError::UnknownRegistration);
            }
            match &slot.state {
                YoungLogState::Resident(_) => return Err(YoungRegistryError::LogNotLoaned),
                YoungLogState::Loaned(generation) => {
                    if *generation != loan.generation {
                        return Err(YoungRegistryError::GenerationMismatch);
                    }
                }
            }
            if self.outstanding_loans == 0 {
                return Err(YoungRegistryError::LoanCountInvariant);
            }
            Ok(())
        })();
        if let Err(reason) = validation {
            return Err(YoungLogReturnError::new(reason, loan));
        }
        slot.state = YoungLogState::Resident(loan.log);
        self.outstanding_loans -= 1;
        Ok(())
    }
}

/// Borrow of the actual canonical registry under the same stopped-world proof
/// used by SATB folding. Constructor checks one outstanding counter, O(1).
/// Resolving each pending registration is O(1); verify enumeration is explicit.
#[derive(Debug)]
#[must_use = "the stopped registry borrow excludes young loans during collection"]
pub(super) struct StoppedYoungLogs<'s> {
    registry: &'s mut MutatorSatbSlot,
    proof: &'s StoppedMutatorsProof,
}

static_assertions::assert_not_impl_any!(StoppedYoungLogs<'static>: Send, Sync, Clone, Copy);

impl<'s> StoppedYoungLogs<'s> {
    #[inline]
    pub(super) fn heap_identity(&self) -> HeapIdentity {
        self.registry.young_heap_identity()
    }

    pub(super) fn new(
        registry: &'s mut MutatorSatbSlot,
        proof: &'s StoppedMutatorsProof,
    ) -> Result<Self, YoungRegistryError> {
        registry.ensure_young_logs_stopped(proof)?;
        Ok(Self { registry, proof })
    }

    pub(super) fn resolve_mut(
        &mut self,
        id: YoungRegistrationId,
    ) -> Result<&mut YoungBlockLog, YoungRegistryError> {
        self.registry.young_log_world_stopped(id, self.proof)
    }

    /// Full canonical enumeration is reserved for verification/inventory, never
    /// ordinary minor capture. Retired entries remain in the exact same set.
    pub(super) fn logs_for_verify(&self) -> impl Iterator<Item = &YoungBlockLog> {
        self.registry.young_logs_world_stopped(self.proof)
    }
}
