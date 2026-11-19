//! Local quit-poll activation uninstallation. Requests still target the
//! retained original flag. Nothing here takes or clears a request, and
//! nothing writes [`super::ASYNC_ATTENTION`] except the flag's own `Drop`.

use super::{QUIT_REQUESTED_TLS, QuitRequest};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum QuitDetachError {
    SlotBorrowed,
    Displaced,
}

enum Restoration {
    Removed,
    AlreadyInactive,
}

/// Affine local receipt. Identity is the `Arc` allocation, not the flag bit.
///
/// # Safety
///
/// There is no safe constructor. [`detach_quit_activation`] is the only mint.
/// The receipt is not `Send`, `Sync`, `Clone`, or `Copy`. Drop does not
/// restore the original flag.
#[must_use]
#[doc(hidden)]
pub struct DetachedQuitActivation {
    original: QuitRequest,
    placeholder: QuitRequest,
    restoration: Restoration,
    local: PhantomData<Rc<()>>,
}

#[must_use]
#[doc(hidden)]
pub struct QuitRestoreFailure {
    reason: QuitDetachError,
    receipt: DetachedQuitActivation,
}

impl std::fmt::Debug for QuitRestoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("QuitRestoreFailure")
            .field(&self.reason)
            .finish()
    }
}

impl QuitRestoreFailure {
    #[doc(hidden)]
    pub fn reason(&self) -> QuitDetachError {
        self.reason
    }

    #[doc(hidden)]
    pub fn into_parts(self) -> (QuitDetachError, DetachedQuitActivation) {
        (self.reason, self.receipt)
    }
}

static_assertions::assert_not_impl_any!(DetachedQuitActivation: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(QuitRestoreFailure: Send, Sync, Clone, Copy);

fn same_request(left: &QuitRequest, right: &QuitRequest) -> bool {
    Arc::ptr_eq(&left.0, &right.0)
}

/// Replace only the exact original with a unique lowered sentinel. The
/// receipt retains the original even if its source Context retires. It does
/// not keep a World or a Context alive.
///
/// # Safety
///
/// The caller has excluded every other writer of this thread's quit slot for
/// the receipt's lifetime. The receipt names no World. It does not authorize
/// a source resume. The write is a direct slot store. It is not a second
/// caller of the signed install or clear functions.
#[cold]
#[doc(hidden)]
pub unsafe fn detach_quit_activation(
    expected: &QuitRequest,
) -> Result<DetachedQuitActivation, QuitDetachError> {
    let placeholder = QuitRequest::new();
    let original = expected.clone();
    let restoration = detach_inner(expected, &placeholder)?;
    Ok(DetachedQuitActivation {
        original,
        placeholder,
        restoration,
        local: PhantomData,
    })
}

fn detach_inner(
    expected: &QuitRequest,
    placeholder: &QuitRequest,
) -> Result<Restoration, QuitDetachError> {
    QUIT_REQUESTED_TLS.with(|slot| {
        let mut active = slot
            .try_borrow_mut()
            .map_err(|_| QuitDetachError::SlotBorrowed)?;
        let restoration = match active.as_ref() {
            Some(current) if same_request(current, expected) => Restoration::Removed,
            Some(_) | None => Restoration::AlreadyInactive,
        };
        let displaced = match restoration {
            Restoration::Removed => active.replace(placeholder.clone()),
            Restoration::AlreadyInactive => None,
        };
        drop(active);
        drop(displaced);
        Ok(restoration)
    })
}

/// Restore over the exact sentinel only. A foreign or absent flag is
/// preserved. Drop and forget do not restore.
#[cold]
#[doc(hidden)]
pub fn restore_quit_activation(receipt: DetachedQuitActivation) -> Result<(), QuitRestoreFailure> {
    match restore_inner(&receipt) {
        Ok(()) => Ok(()),
        Err(reason) => Err(QuitRestoreFailure { reason, receipt }),
    }
}

fn restore_inner(receipt: &DetachedQuitActivation) -> Result<(), QuitDetachError> {
    QUIT_REQUESTED_TLS.with(|slot| {
        let mut active = slot
            .try_borrow_mut()
            .map_err(|_| QuitDetachError::SlotBorrowed)?;
        let displaced = match receipt.restoration {
            Restoration::AlreadyInactive => None,
            Restoration::Removed => {
                match active.as_ref() {
                    Some(current) if same_request(current, &receipt.placeholder) => {}
                    Some(_) | None => return Err(QuitDetachError::Displaced),
                }
                active.replace(receipt.original.clone())
            }
        };
        drop(active);
        drop(displaced);
        Ok(())
    })
}

#[cfg(test)]
#[path = "quit_detach_test.rs"]
mod tests;
