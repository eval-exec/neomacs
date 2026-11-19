//! Local effect on the existing observation TLS owner, after collector drain.
//! This receipt never retains the original non-atomic Rc or a Weak to it.
//! It is not source ownership, a return witness, or durable export permission.
//! The collector's existing closed-admission contract must already exclude
//! outstanding borrowed StoreState views/raw designators and their enclosing
//! frames. Rc/cold-state counts cannot discover such unowned views. Export
//! must wait until every collector/store-bearing argument/capture frame ends.
// Dormant until the owned source transaction is composed.
use super::*;
use crate::tagged::gc::HeapIdentity;
use std::cell::RefMut;
use std::thread::ThreadId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceObservationError {
    InstalledHeap,
    MissingOwner,
    AttachedStore,
    WrongHeap,
    WrongOwner,
    WrongThread,
    StoreDisplaced,
    OwnerBorrowed,
    SuspendedBorrowed,
    ColdBorrowed,
    SuspendedScopes,
    ActiveCaptures,
    LiveCertificates,
    EscapedOwner,
    PlaceholderDisplaced,
}

/// Compared only. The canonical heap OnceCell is the authority on return;
/// this address can neither reconstruct an Rc nor dereference its payload.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ObservationIdentity(NonNull<WorldObservations>);
impl ObservationIdentity {
    fn of(owner: &ObservationWorld) -> Self {
        Self(NonNull::from(owner.0.as_ref()))
    }
}

#[must_use]
pub(crate) struct SourceObservationsDetached {
    heap: HeapIdentity,
    original: ObservationIdentity,
    placeholder: ObservationWorld,
    thread: ThreadId,
}
static_assertions::assert_not_impl_any!(SourceObservationsDetached: Send, Sync, Clone, Copy);

#[must_use]
pub(crate) struct SourceObservationRestoreFailure {
    pub(crate) reason: SourceObservationError,
    /// The caller keeps this receipt and may retry. No product caller reads it yet.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the owned failure returns this receipt to its caller; observation tests read it"
        )
    )]
    pub(crate) receipt: SourceObservationsDetached,
}
impl std::fmt::Debug for SourceObservationRestoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SourceObservationRestoreFailure")
            .field(&self.reason)
            .finish()
    }
}

// Keep the actual exclusive cold-data borrow through the TLS swap. An
// uninitialized OnceCell needs no allocation merely to establish idleness.
enum IdleCold<'a> {
    Uninitialized,
    Borrowed { _loan: RefMut<'a, State> },
}
fn idle_cold(owner: &ObservationWorld) -> Result<IdleCold<'_>, SourceObservationError> {
    let Some(cell) = owner.0.cold.get() else {
        return Ok(IdleCold::Uninitialized);
    };
    let loan = cell
        .try_borrow_mut()
        .map_err(|_| SourceObservationError::ColdBorrowed)?;
    if !loan.captures.is_empty() {
        return Err(SourceObservationError::ActiveCaptures);
    }
    match loan.certificate_pool.availability() {
        certificate_storage::CertificateAvailability::Idle => {}
        certificate_storage::CertificateAvailability::Leased => {
            return Err(SourceObservationError::LiveCertificates);
        }
    }
    Ok(IdleCold::Borrowed { _loan: loan })
}
enum RefOwners {
    HeapOnly,
    HeapAndTls,
    ReceiptAndTls,
}
fn exclusive_refs(
    owner: &ObservationWorld,
    expected: RefOwners,
) -> Result<(), SourceObservationError> {
    let count = match expected {
        RefOwners::HeapOnly => 1,
        RefOwners::HeapAndTls | RefOwners::ReceiptAndTls => 2,
    };
    if Rc::strong_count(&owner.0) != count || Rc::weak_count(&owner.0) != 0 {
        return Err(SourceObservationError::EscapedOwner);
    }
    Ok(())
}

impl ObservationWorld {
    /// Called only with the canonical owner's reference obtained from the
    /// actual collector loan. Do not clone that owner before this census.
    #[cold]
    pub(in crate::tagged) fn detach_source(
        &self,
        heap: HeapIdentity,
    ) -> Result<SourceObservationsDetached, SourceObservationError> {
        if !self.is_detached() {
            return Err(SourceObservationError::AttachedStore);
        }
        // All allocation and Rc cloning precedes the borrowed-slot interval.
        let placeholder = Self::default();
        let arriving = placeholder.clone();
        let departing = STATE.with(|slot| {
            let mut owner = slot
                .owner
                .try_borrow_mut()
                .map_err(|_| SourceObservationError::OwnerBorrowed)?;
            let suspended = slot
                .suspended
                .try_borrow_mut()
                .map_err(|_| SourceObservationError::SuspendedBorrowed)?;
            if !Rc::ptr_eq(&owner.0, &self.0) {
                return Err(SourceObservationError::WrongOwner);
            }
            if slot.store.get() != self.root() {
                return Err(SourceObservationError::StoreDisplaced);
            }
            if !suspended.is_empty() {
                return Err(SourceObservationError::SuspendedScopes);
            }
            let _cold = idle_cold(self)?;
            // Exactly the canonical heap owner and the installed TLS owner.
            exclusive_refs(self, RefOwners::HeapAndTls)?;
            let departing = std::mem::replace(&mut *owner, arriving);
            slot.store.set(placeholder.root());
            ACTIVE.with(|active| active.set(false));
            Ok(departing)
        })?;
        // No original Rc destructor runs under any TLS or cold-data guard.
        // The heap is now its sole strong owner, with no external weak owners.
        drop(departing);
        reset_recent_reads_for_heap_installation();
        Ok(SourceObservationsDetached {
            heap,
            original: ObservationIdentity::of(self),
            placeholder,
            thread: std::thread::current().id(),
        })
    }

    /// Cold, instantaneous census for the actual heap export boundary. The
    /// detach effect may have become stale through later installation or a
    /// callback. Every export/re-export must validate again immediately before
    /// consuming collector authority, without intervening source execution.
    #[cold]
    pub(in crate::tagged) fn validate_source_export(&self) -> Result<(), SourceObservationError> {
        if !self.is_detached() {
            return Err(SourceObservationError::AttachedStore);
        }
        let _cold = idle_cold(self)?;
        exclusive_refs(self, RefOwners::HeapOnly)
    }
}

impl SourceObservationsDetached {
    /// Local restoration effect only. An owned source adapter must keep this
    /// private until its separate matching return control has been validated.
    #[cold]
    pub(in crate::tagged) fn restore(
        self,
        heap: HeapIdentity,
        canonical: &ObservationWorld,
    ) -> Result<(), SourceObservationRestoreFailure> {
        let result = self.try_restore(heap, canonical);
        result.map_err(|reason| SourceObservationRestoreFailure {
            reason,
            receipt: self,
        })
    }

    fn try_restore(
        &self,
        heap: HeapIdentity,
        canonical: &ObservationWorld,
    ) -> Result<(), SourceObservationError> {
        if self.thread != std::thread::current().id() {
            return Err(SourceObservationError::WrongThread);
        }
        if self.heap != heap {
            return Err(SourceObservationError::WrongHeap);
        }
        if self.original != ObservationIdentity::of(canonical) {
            return Err(SourceObservationError::WrongOwner);
        }
        if !canonical.is_detached() {
            return Err(SourceObservationError::AttachedStore);
        }
        let departing = STATE.with(|slot| {
            let mut owner = slot
                .owner
                .try_borrow_mut()
                .map_err(|_| SourceObservationError::OwnerBorrowed)?;
            let suspended = slot
                .suspended
                .try_borrow_mut()
                .map_err(|_| SourceObservationError::SuspendedBorrowed)?;
            if !Rc::ptr_eq(&owner.0, &self.placeholder.0) {
                return Err(SourceObservationError::PlaceholderDisplaced);
            }
            if slot.store.get() != self.placeholder.root() {
                return Err(SourceObservationError::StoreDisplaced);
            }
            if !self.placeholder.is_detached() {
                return Err(SourceObservationError::AttachedStore);
            }
            if !suspended.is_empty() {
                return Err(SourceObservationError::SuspendedScopes);
            }
            let _original = idle_cold(canonical)?;
            let _placeholder = idle_cold(&self.placeholder)?;
            exclusive_refs(canonical, RefOwners::HeapOnly)?;
            exclusive_refs(&self.placeholder, RefOwners::ReceiptAndTls)?;
            let departing = std::mem::replace(&mut *owner, canonical.clone());
            slot.store.set(canonical.root());
            ACTIVE.with(|active| active.set(false));
            Ok(departing)
        })?;
        drop(departing);
        reset_recent_reads_for_heap_installation();
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/detachment_test.rs"]
mod tests;
