//! Local in-flight root-registry uninstallation, not World execution authority.
//! The caller supplies a unique empty sentinel so this function does not mint
//! a handle. Existing pins keep their original table. No slot or free-list is
//! cleared. The lazy getter is not called.

use super::{IN_FLIGHT_ROOTS, InFlightRegistryHandle, InFlightRootTable};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, MutexGuard, TryLockError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InFlightDetachError {
    SlotBorrowed,
    OriginalBusy,
    OriginalPoisoned,
    PlaceholderBusy,
    PlaceholderPoisoned,
    Displaced,
}

#[derive(Clone, Copy)]
enum TableRole {
    Original,
    Placeholder,
}

fn table_loan(
    handle: &InFlightRegistryHandle,
    role: TableRole,
) -> Result<MutexGuard<'_, InFlightRootTable>, InFlightDetachError> {
    match handle.table.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::WouldBlock) => Err(match role {
            TableRole::Original => InFlightDetachError::OriginalBusy,
            TableRole::Placeholder => InFlightDetachError::PlaceholderBusy,
        }),
        Err(TryLockError::Poisoned(guard)) => {
            // Drop the poisoned guard. Never recover or clear the poison.
            drop(guard);
            Err(match role {
                TableRole::Original => InFlightDetachError::OriginalPoisoned,
                TableRole::Placeholder => InFlightDetachError::PlaceholderPoisoned,
            })
        }
    }
}

enum Restoration {
    Removed,
    AlreadyInactive,
}

/// Affine local receipt. `PhantomData<Rc<()>>` keeps it `!Send` even though
/// the handle itself is `Send + Sync`.
#[must_use]
pub(crate) struct DetachedInFlightRegistry {
    original: InFlightRegistryHandle,
    placeholder: InFlightRegistryHandle,
    restoration: Restoration,
    local: PhantomData<Rc<()>>,
}

#[must_use]
pub(crate) struct InFlightDetachFailure {
    reason: InFlightDetachError,
    placeholder: InFlightRegistryHandle,
    local: PhantomData<Rc<()>>,
}

#[must_use]
pub(crate) struct InFlightRestoreFailure {
    reason: InFlightDetachError,
    receipt: DetachedInFlightRegistry,
}

impl std::fmt::Debug for InFlightDetachFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("InFlightDetachFailure")
            .field(&self.reason)
            .finish()
    }
}

impl std::fmt::Debug for InFlightRestoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("InFlightRestoreFailure")
            .field(&self.reason)
            .finish()
    }
}

impl InFlightDetachFailure {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L3 cold in-flight refusal returns the owned placeholder to tests"
        )
    )]
    pub(crate) fn into_parts(self) -> (InFlightDetachError, InFlightRegistryHandle) {
        (self.reason, self.placeholder)
    }
}

impl InFlightRestoreFailure {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L3 cold in-flight refusal returns the owned receipt to tests"
        )
    )]
    pub(crate) fn into_parts(self) -> (InFlightDetachError, DetachedInFlightRegistry) {
        (self.reason, self.receipt)
    }
}

static_assertions::assert_not_impl_any!(DetachedInFlightRegistry: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(InFlightDetachFailure: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(InFlightRestoreFailure: Send, Sync, Clone, Copy);

/// Uninstall only the exact retained table. A foreign installation is
/// preserved. `placeholder` is minted by the caller. This function does not
/// call [`InFlightRegistryHandle::new`] or the lazy getter, and it does not
/// end a caller's pin.
#[cold]
pub(crate) fn detach_in_flight_registry_handle(
    expected: &InFlightRegistryHandle,
    placeholder: InFlightRegistryHandle,
) -> Result<DetachedInFlightRegistry, InFlightDetachFailure> {
    match detach_prepared(expected, &placeholder) {
        Ok((original, restoration)) => Ok(DetachedInFlightRegistry {
            original,
            placeholder,
            restoration,
            local: PhantomData,
        }),
        Err(reason) => Err(InFlightDetachFailure {
            reason,
            placeholder,
            local: PhantomData,
        }),
    }
}

fn detach_prepared(
    expected: &InFlightRegistryHandle,
    placeholder: &InFlightRegistryHandle,
) -> Result<(InFlightRegistryHandle, Restoration), InFlightDetachError> {
    IN_FLIGHT_ROOTS.with(|slot| {
        let mut active = slot
            .try_borrow_mut()
            .map_err(|_| InFlightDetachError::SlotBorrowed)?;
        let table = table_loan(expected, TableRole::Original)?;
        let departing = active.take_if(|current| Arc::ptr_eq(&current.table, &expected.table));
        let result = match departing {
            Some(original) => {
                *active = Some(placeholder.clone());
                (original, Restoration::Removed)
            }
            None => (expected.clone(), Restoration::AlreadyInactive),
        };
        drop(table);
        drop(active);
        Ok(result)
    })
}

/// Reinstall only over this receipt's exact sentinel. A displaced sentinel is
/// an owned refusal. Drop does not restore. The lazy getter is not called.
#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 cold in-flight registry restore; no production caller enters it"
    )
)]
pub(crate) fn restore_in_flight_registry_handle(
    receipt: DetachedInFlightRegistry,
) -> Result<(), InFlightRestoreFailure> {
    match restore_inner(&receipt) {
        Ok(()) => Ok(()),
        Err(reason) => Err(InFlightRestoreFailure { reason, receipt }),
    }
}

fn restore_inner(receipt: &DetachedInFlightRegistry) -> Result<(), InFlightDetachError> {
    IN_FLIGHT_ROOTS.with(|slot| {
        let mut active = slot
            .try_borrow_mut()
            .map_err(|_| InFlightDetachError::SlotBorrowed)?;
        match receipt.restoration {
            Restoration::AlreadyInactive => {
                let table = table_loan(&receipt.original, TableRole::Original)?;
                drop(table);
                drop(active);
                Ok(())
            }
            Restoration::Removed => {
                if !active
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(&current.table, &receipt.placeholder.table))
                {
                    return Err(InFlightDetachError::Displaced);
                }
                let original = table_loan(&receipt.original, TableRole::Original)?;
                let placeholder = table_loan(&receipt.placeholder, TableRole::Placeholder)?;
                let displaced = active.replace(receipt.original.clone());
                drop(placeholder);
                drop(original);
                drop(active);
                drop(displaced);
                Ok(())
            }
        }
    })
}

/// Sealed carrier for the eval source shell. `DetachedInFlightRegistry` stays
/// crate-private; this wrapper has no public fields and no public constructor.
#[must_use]
#[doc(hidden)]
pub struct SourceShellInFlightReceipt {
    #[expect(
        dead_code,
        reason = "retained private receipt; the shell has no reader and Drop does not restore"
    )]
    receipt: DetachedInFlightRegistry,
    local: PhantomData<Rc<()>>,
}

/// Refusal of that carrier. Drop returns nothing to the slot.
#[must_use]
#[doc(hidden)]
pub struct SourceShellInFlightRefusal {
    failure: InFlightDetachFailure,
    local: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for SourceShellInFlightRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SourceShellInFlightRefusal")
            .field(&self.failure)
            .finish()
    }
}

static_assertions::assert_not_impl_any!(SourceShellInFlightReceipt: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SourceShellInFlightRefusal: Send, Sync, Clone, Copy);

/// Mint the empty placeholder and uninstall only the exact retained table.
///
/// # Safety
///
/// The caller has excluded every other use of this thread's in-flight slot
/// for the receipt's lifetime. The receipt names no World and does not resume
/// a source. Only the dormant source shell calls this.
#[cold]
#[doc(hidden)]
pub unsafe fn detach_source_shell_in_flight(
    expected: &InFlightRegistryHandle,
) -> Result<SourceShellInFlightReceipt, SourceShellInFlightRefusal> {
    let placeholder = InFlightRegistryHandle::new(None);
    match detach_in_flight_registry_handle(expected, placeholder) {
        Ok(receipt) => Ok(SourceShellInFlightReceipt {
            receipt,
            local: PhantomData,
        }),
        Err(failure) => Err(SourceShellInFlightRefusal {
            failure,
            local: PhantomData,
        }),
    }
}

#[cfg(test)]
#[path = "tests/registry_detach_test.rs"]
mod tests;
