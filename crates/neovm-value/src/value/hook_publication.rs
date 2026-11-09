//! Cold publication of one immutable, process-lifetime callback descriptor.
//!
//! Descriptor identity is its data address, never a callback's code address.
//! Readers retain their original `OnceLock<Table>` and do not touch the
//! identity cell. An unsafe installer supplies each hook's semantic contract.

use std::sync::OnceLock;

#[must_use = "handle the callback table publication disposition"]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookInstallStatus {
    Installed,
    AlreadyInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookInstallError {
    DifferentTable,
}

/// The identity-selection half of publication, shared by this crate and the
/// evaluator's hook-set publisher (cross-crate K1 surface).
///
/// The first offered descriptor wins the slot.  A concurrent offer of the
/// SAME descriptor is idempotent; a different descriptor is the typed
/// refusal.  `get_or_init` serializes concurrent first publications and
/// returns the resident value, so the comparison sees what the cell actually
/// holds -- `OnceLock::set` cannot answer a conflict because its `Err`
/// payload is the REJECTED ARGUMENT (the caller's own table), not the
/// resident value.
#[doc(hidden)]
#[cold]
#[inline(never)]
pub fn select_published_identity<T: 'static>(
    identity: &OnceLock<&'static T>,
    offered: &'static T,
) -> Result<&'static T, HookInstallError> {
    let selected = identity.get_or_init(|| offered);
    if std::ptr::eq(*selected, offered) {
        Ok(*selected)
    } else {
        Err(HookInstallError::DifferentTable)
    }
}

/// Publish one payload cell through the shared identity rule, for the cold
/// per-table installers (cross-crate K1 surface; `value`'s own
/// `hook_publication::publish` callers and the evaluator's nine descriptor
/// installers share this one body).
///
/// Pure std `OnceLock` state: it compares descriptor addresses and copies a
/// `Copy` fn-pointer table into the payload cell.  No heap allocation, no
/// thread-local or carrier access, no callback invocation.
#[doc(hidden)]
#[cold]
#[inline(never)]
pub fn publish<T: Copy + Sync + 'static>(
    payload: &OnceLock<T>,
    identity: &OnceLock<&'static T>,
    offered: &'static T,
) -> Result<HookInstallStatus, HookInstallError> {
    let selected = select_published_identity(identity, offered)?;
    // Only the selected descriptor can reach this cell. `set` waits for a
    // concurrent same-descriptor publication to complete before returning.
    // Copying function pointers does not invoke any callback.
    match payload.set(*selected) {
        Ok(()) => Ok(HookInstallStatus::Installed),
        Err(_same_descriptor) => Ok(HookInstallStatus::AlreadyInstalled),
    }
}

#[cfg(test)]
#[path = "hook_publication/tests.rs"]
mod tests;
