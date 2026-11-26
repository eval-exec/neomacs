//! Cold, identity-checked uninstallation. Registry contents are never reset.
//! The receipt records a local effect. It is not permission to execute or
//! suspend a World. A consuming adapter must establish that exclusion itself.

use super::{HeapRegistryHandle, HeapRegistrySlot};
use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum RegistryDetachError {
    SlotBorrowed,
    RegistryBorrowed,
    PlaceholderBorrowed,
    WrongSlot,
    Displaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlotIdentity(NonNull<()>);

impl SlotIdentity {
    fn of<T>(slot: &HeapRegistrySlot<T>) -> Self {
        Self(NonNull::from(slot).cast())
    }
}

enum Restoration {
    Removed,
    AlreadyInactive,
}

/// Affine local receipt. The original contents and the exact replacement are
/// retained. Neither is exposed as a registry view. Ordinary install remains
/// possible, but a displaced receipt cannot overwrite that later installation.
///
/// # Safety
///
/// There is no safe constructor. [`HeapRegistrySlot::detach`] is the only
/// mint, and it is `unsafe` because the caller has to have excluded every
/// other use of the slot. The receipt is not `Send`, `Sync`, `Clone`, or
/// `Copy`. Drop never reinstalls it.
#[must_use]
#[doc(hidden)]
pub struct DetachedRegistry<T> {
    slot: SlotIdentity,
    original: HeapRegistryHandle<T>,
    placeholder: HeapRegistryHandle<T>,
    restoration: Restoration,
}

#[must_use]
#[doc(hidden)]
pub struct RegistryDetachFailure<T> {
    reason: RegistryDetachError,
    placeholder: HeapRegistryHandle<T>,
}

impl<T> std::fmt::Debug for RegistryDetachFailure<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RegistryDetachFailure")
            .field(&self.reason)
            .finish()
    }
}

impl<T> RegistryDetachFailure<T> {
    #[doc(hidden)]
    pub fn reason(&self) -> RegistryDetachError {
        self.reason
    }

    #[doc(hidden)]
    pub fn into_parts(self) -> (RegistryDetachError, HeapRegistryHandle<T>) {
        (self.reason, self.placeholder)
    }
}

#[must_use]
#[doc(hidden)]
pub struct RegistryRestoreFailure<T> {
    reason: RegistryDetachError,
    receipt: DetachedRegistry<T>,
}

impl<T> std::fmt::Debug for RegistryRestoreFailure<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RegistryRestoreFailure")
            .field(&self.reason)
            .finish()
    }
}

impl<T> RegistryRestoreFailure<T> {
    #[doc(hidden)]
    pub fn reason(&self) -> RegistryDetachError {
        self.reason
    }

    #[doc(hidden)]
    pub fn into_parts(self) -> (RegistryDetachError, DetachedRegistry<T>) {
        (self.reason, self.receipt)
    }
}

impl<T> std::fmt::Debug for DetachedRegistry<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetachedRegistry").finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(DetachedRegistry<()>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RegistryDetachFailure<()>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RegistryRestoreFailure<()>: Send, Sync, Clone, Copy);

impl<T> HeapRegistrySlot<T> {
    /// Replace only the exact expected handle. The caller supplies the empty
    /// placeholder value. It is allocated before either `RefCell` is borrowed,
    /// so no allocator or payload destructor runs under a registry guard.
    ///
    /// # Safety
    ///
    /// The caller has excluded every other use of this slot for the receipt's
    /// lifetime. The receipt is a local effect on this thread. It does not
    /// authorize World execution, suspension, or a source resume. A parked
    /// `None` slot is `AlreadyInactive` and is left empty.
    #[cold]
    #[doc(hidden)]
    pub unsafe fn detach(
        &self,
        expected: &HeapRegistryHandle<T>,
        placeholder: T,
    ) -> Result<DetachedRegistry<T>, RegistryDetachFailure<T>> {
        let placeholder = HeapRegistryHandle {
            // Uninstalled local storage. Never label it as the current heap.
            heap_identity: 0,
            registry: Arc::new(RefCell::new(placeholder)),
        };
        match self.detach_prepared(expected, &placeholder) {
            Ok((original, restoration)) => Ok(DetachedRegistry {
                slot: SlotIdentity::of(self),
                original,
                placeholder,
                restoration,
            }),
            Err(reason) => Err(RegistryDetachFailure {
                reason,
                placeholder,
            }),
        }
    }

    fn detach_prepared(
        &self,
        expected: &HeapRegistryHandle<T>,
        placeholder: &HeapRegistryHandle<T>,
    ) -> Result<(HeapRegistryHandle<T>, Restoration), RegistryDetachError> {
        let mut active = self
            .active
            .try_borrow_mut()
            .map_err(|_| RegistryDetachError::SlotBorrowed)?;
        // The outer guard misses a view borrowed directly through the handle.
        let source_view = expected
            .registry
            .try_borrow_mut()
            .map_err(|_| RegistryDetachError::RegistryBorrowed)?;
        let installed = active.take();
        let result = match installed {
            Some(original) if Arc::ptr_eq(&original.registry, &expected.registry) => {
                *active = Some(placeholder.clone());
                (original, Restoration::Removed)
            }
            other => {
                *active = other;
                (expected.clone(), Restoration::AlreadyInactive)
            }
        };
        drop(source_view);
        drop(active);
        Ok(result)
    }
}

impl<T> DetachedRegistry<T> {
    /// Reinstall only at the original slot and only over this receipt's exact
    /// placeholder. Failure owns the unchanged receipt. Drop never reinstalls
    /// a registry and never manufactures a source-resume permission.
    #[cold]
    #[doc(hidden)]
    pub fn restore(self, slot: &HeapRegistrySlot<T>) -> Result<(), RegistryRestoreFailure<T>> {
        match self.restore_inner(slot) {
            Ok(()) => Ok(()),
            Err(reason) => Err(RegistryRestoreFailure {
                reason,
                receipt: self,
            }),
        }
    }

    fn restore_inner(&self, slot: &HeapRegistrySlot<T>) -> Result<(), RegistryDetachError> {
        if SlotIdentity::of(slot) != self.slot {
            return Err(RegistryDetachError::WrongSlot);
        }
        let mut active = slot
            .active
            .try_borrow_mut()
            .map_err(|_| RegistryDetachError::SlotBorrowed)?;
        match self.restoration {
            Restoration::AlreadyInactive => {
                let original_view = self
                    .original
                    .registry
                    .try_borrow_mut()
                    .map_err(|_| RegistryDetachError::RegistryBorrowed)?;
                drop(original_view);
                drop(active);
                Ok(())
            }
            Restoration::Removed => {
                if !active.as_ref().is_some_and(|installed| {
                    Arc::ptr_eq(&installed.registry, &self.placeholder.registry)
                }) {
                    return Err(RegistryDetachError::Displaced);
                }
                let original_view = self
                    .original
                    .registry
                    .try_borrow_mut()
                    .map_err(|_| RegistryDetachError::RegistryBorrowed)?;
                let placeholder_view = self
                    .placeholder
                    .registry
                    .try_borrow_mut()
                    .map_err(|_| RegistryDetachError::PlaceholderBorrowed)?;
                let displaced = active.replace(self.original.clone());
                drop(placeholder_view);
                drop(original_view);
                drop(active);
                drop(displaced);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/detachment_test.rs"]
mod tests;
