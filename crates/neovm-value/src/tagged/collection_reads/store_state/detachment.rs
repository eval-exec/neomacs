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
#[doc(hidden)]
pub enum SourceObservationError {
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

/// Confined placeholder receipt. No public constructor. Not `Send`, `Sync`,
/// `Clone`, or `Copy`. The original observation `Rc` does not survive in it.
#[must_use]
#[doc(hidden)]
pub struct SourceObservationsDetached {
    heap: HeapIdentity,
    original: ObservationIdentity,
    placeholder: ObservationWorld,
    thread: ThreadId,
}
static_assertions::assert_not_impl_any!(SourceObservationsDetached: Send, Sync, Clone, Copy);

#[must_use]
pub(crate) struct SourceObservationRestoreFailure {
    pub(crate) reason: SourceObservationError,
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
        // A later heap must not adopt a TLS sentinel whose Rc is retained by
        // a pending source receipt. Reserve this allocation permanently.
        placeholder.0.bound.set(true);
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

    /// Full store detachment refreshes an existing TLS pointer mirror.
    /// Refuse its uncounted slot loans and a mismatched attached foreign owner
    /// before that existing operation, rather than panic during unwind.
    #[cold]
    pub(in crate::tagged) fn preflight_source_store_retirement(
        &self,
    ) -> Result<(), SourceObservationError> {
        STATE.with(|slot| {
            let owner = slot
                .owner
                .try_borrow()
                .map_err(|_| SourceObservationError::OwnerBorrowed)?;
            if !Rc::ptr_eq(&owner.0, &self.0) && !owner.is_detached() {
                return Err(SourceObservationError::WrongOwner);
            }
            if slot.store.get() != owner.root() {
                return Err(SourceObservationError::StoreDisplaced);
            }
            Ok(())
        })
    }

    /// Destination-local retirement: only remove our exact canonical TLS
    /// membership; preserve an unrelated current owner. No original Rc is
    /// retained in the discarded placeholder receipt.
    #[cold]
    pub(in crate::tagged) fn retire_source_membership(
        &self,
        heap: HeapIdentity,
    ) -> Result<(), SourceObservationError> {
        let current = STATE.with(|slot| {
            let owner = slot
                .owner
                .try_borrow()
                .map_err(|_| SourceObservationError::OwnerBorrowed)?;
            Ok(Rc::ptr_eq(&owner.0, &self.0))
        })?;
        if current {
            drop(self.detach_source(heap)?);
        }
        self.validate_source_export()
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
    /// Test census only. This compares identities and grants no owner handle.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn original_is_current_for_test(&self) -> bool {
        super::super::STATE
            .with(|slot| ObservationIdentity::of(&slot.owner.borrow()) == self.original)
    }

    /// Test census only. True when TLS still holds this receipt's placeholder.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn placeholder_is_current_for_test(&self) -> bool {
        super::super::STATE.with(|slot| {
            ObservationIdentity::of(&slot.owner.borrow())
                == ObservationIdentity::of(&self.placeholder)
        })
    }

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

    /// A receipt is checked against actual canonical backing at export too;
    /// possessing an earlier effect never exempts the sender from this census.
    #[cold]
    pub(in crate::tagged) fn validate_interval(
        &self,
        heap: HeapIdentity,
        canonical: &ObservationWorld,
    ) -> Result<(), SourceObservationError> {
        self.with_interval(heap, canonical, |_, _| ())
    }

    fn try_restore(
        &self,
        heap: HeapIdentity,
        canonical: &ObservationWorld,
    ) -> Result<(), SourceObservationError> {
        let departing = self.with_interval(heap, canonical, |slot, owner| {
            let departing = std::mem::replace(owner, canonical.clone());
            slot.store.set(canonical.root());
            ACTIVE.with(|active| active.set(false));
            departing
        })?;
        drop(departing);
        reset_recent_reads_for_heap_installation();
        Ok(())
    }

    fn with_interval<R>(
        &self,
        heap: HeapIdentity,
        canonical: &ObservationWorld,
        body: impl FnOnce(&StateSlot, &mut ObservationWorld) -> R,
    ) -> Result<R, SourceObservationError> {
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
        STATE.with(|slot| {
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
            Ok(body(slot, &mut owner))
        })
    }
}

#[cfg(test)]
#[path = "tests/detachment_test.rs"]
mod tests;
