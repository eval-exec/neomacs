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

#[cold]
#[inline(never)]
pub(crate) fn publish<T: Copy + Sync + 'static>(
    payload: &OnceLock<T>,
    identity: &OnceLock<&'static T>,
    offered: &'static T,
) -> Result<HookInstallStatus, HookInstallError> {
    let selected = identity.get_or_init(|| offered);
    if !std::ptr::eq(*selected, offered) {
        return Err(HookInstallError::DifferentTable);
    }
    // Only the selected descriptor can reach this cell. `set` waits for a
    // concurrent same-descriptor publication to complete before returning.
    // Copying function pointers does not invoke any callback.
    match payload.set(**selected) {
        Ok(()) => Ok(HookInstallStatus::Installed),
        Err(_same_descriptor) => Ok(HookInstallStatus::AlreadyInstalled),
    }
}

#[cfg(test)]
#[path = "hook_publication/tests.rs"]
mod tests;
