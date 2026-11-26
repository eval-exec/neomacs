//! Local arithmetic-policy uninstallation, not World execution authority.
//!
//! The hot reader upgrades a binding's weak owner before reading its
//! forwarder. A unique, permanently dead weak owner disables the binding
//! without a new TLS slot and without editing that reader.

use super::{INTEGER_WIDTH, IntegerWidthBinding, IntegerWidthContext};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Weak};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntegerWidthDetachError {
    SlotBorrowed,
    Displaced,
    OwnerRetired,
}

enum Restoration {
    Removed(IntegerWidthBinding),
    AlreadyInactive,
}

/// Retains the removed binding. It cannot reactivate a Context or authorize
/// reading the forwarder while detached.
#[must_use]
pub(crate) struct DetachedIntegerWidthActivation {
    owner: Weak<()>,
    placeholder: Weak<()>,
    restoration: Restoration,
    local: PhantomData<Rc<()>>,
}

#[must_use]
pub(crate) struct IntegerWidthRestoreFailure {
    reason: IntegerWidthDetachError,
    receipt: DetachedIntegerWidthActivation,
}

impl std::fmt::Debug for IntegerWidthRestoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("IntegerWidthRestoreFailure")
            .field(&self.reason)
            .finish()
    }
}

impl IntegerWidthRestoreFailure {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E2 L3 cold integer-width refusal returns the owned receipt to tests"
        )
    )]
    pub(crate) fn into_parts(self) -> (IntegerWidthDetachError, DetachedIntegerWidthActivation) {
        (self.reason, self.receipt)
    }
}

static_assertions::assert_not_impl_any!(DetachedIntegerWidthActivation: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(IntegerWidthRestoreFailure: Send, Sync, Clone, Copy);

impl IntegerWidthContext {
    /// Uninstall only this Context's activation. A newer Context is preserved.
    /// AlreadyInactive installs nothing. No `Drop` hook reinstalls the binding.
    #[cold]
    pub(crate) fn detach_activation(
        &self,
    ) -> Result<DetachedIntegerWidthActivation, IntegerWidthDetachError> {
        // Retire the strong token before any slot guard. An upgrade of this
        // sentinel must never enable it.
        let placeholder = {
            let owner = Arc::new(());
            Arc::downgrade(&owner)
        };
        let owner = Arc::downgrade(&self.lifetime);
        let restoration = INTEGER_WIDTH.with(|slot| {
            let mut active = slot
                .try_borrow_mut()
                .map_err(|_| IntegerWidthDetachError::SlotBorrowed)?;
            let departing = active.take_if(|binding| binding.owner.ptr_eq(&owner));
            let restoration = match departing {
                Some(original) => {
                    *active = Some(IntegerWidthBinding {
                        forwarder: original.forwarder,
                        owner: placeholder.clone(),
                    });
                    Restoration::Removed(original)
                }
                None => Restoration::AlreadyInactive,
            };
            drop(active);
            Ok(restoration)
        })?;
        Ok(DetachedIntegerWidthActivation {
            owner,
            placeholder,
            restoration,
            local: PhantomData,
        })
    }
}

/// Restore the captured binding only over its exact disabled sentinel.
/// `AlreadyInactive` never installs anything. This does not call the ordinary
/// activator.
#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 cold integer-width restore; no production caller enters it"
    )
)]
pub(crate) fn restore_integer_width_activation(
    receipt: DetachedIntegerWidthActivation,
) -> Result<(), IntegerWidthRestoreFailure> {
    match restore_inner(&receipt) {
        Ok(()) => Ok(()),
        Err(reason) => Err(IntegerWidthRestoreFailure { reason, receipt }),
    }
}

fn restore_inner(receipt: &DetachedIntegerWidthActivation) -> Result<(), IntegerWidthDetachError> {
    let _owner = receipt
        .owner
        .upgrade()
        .ok_or(IntegerWidthDetachError::OwnerRetired)?;
    INTEGER_WIDTH.with(|slot| {
        let mut active = slot
            .try_borrow_mut()
            .map_err(|_| IntegerWidthDetachError::SlotBorrowed)?;
        let displaced = match &receipt.restoration {
            Restoration::AlreadyInactive => None,
            Restoration::Removed(original) => {
                if !active
                    .as_ref()
                    .is_some_and(|binding| binding.owner.ptr_eq(&receipt.placeholder))
                {
                    return Err(IntegerWidthDetachError::Displaced);
                }
                active.replace(IntegerWidthBinding {
                    forwarder: original.forwarder,
                    owner: original.owner.clone(),
                })
            }
        };
        drop(active);
        drop(displaced);
        Ok(())
    })
}

#[cfg(test)]
#[path = "activation_test.rs"]
mod tests;
