//! The typed future replacement for the existing epoch word. There is NO new
//! thread_local instance or current-slot dereference in this dormant stage.
//! P9.3 must install/close it at real owner/stop/park boundaries before cutover.
use super::domain::{InternReadScope, InternView};
use std::cell::Cell;
use std::ptr::NonNull;

pub(crate) struct InstalledScope<'scope, 'domain> {
    scope: &'scope InternReadScope<'domain>,
}
impl<'scope, 'domain> InstalledScope<'scope, 'domain> {
    pub(crate) fn new(scope: &'scope InternReadScope<'domain>) -> Self {
        Self { scope }
    }
    pub(crate) fn borrow<'loan, T>(&'loan self, snapshot: &'loan T) -> InternView<'loan, T> {
        self.scope.borrow(snapshot)
    }
}

pub(crate) struct ScopeSlot<'scope, 'domain> {
    current: Cell<Option<NonNull<InstalledScope<'scope, 'domain>>>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScopeStillInstalled;
impl<'scope, 'domain> ScopeSlot<'scope, 'domain> {
    pub(crate) const fn new() -> Self {
        Self {
            current: Cell::new(None),
        }
    }
    pub(crate) fn ensure_empty(&self) -> Result<(), ScopeStillInstalled> {
        match self.current.get() {
            None => Ok(()),
            Some(_) => Err(ScopeStillInstalled),
        }
    }
    /// Local borrowed installation only. No u64-to-pointer decoding, erased
    /// lifetime, raw dereference or new TLS slot is required for this proof.
    pub(crate) fn install<'slot>(
        &'slot mut self,
        installed: &'scope InstalledScope<'scope, 'domain>,
    ) -> ScopeInstallation<'slot, 'scope, 'domain> {
        let previous = self.current.replace(Some(NonNull::from(installed)));
        ScopeInstallation {
            slot: self,
            previous,
        }
    }
}
#[must_use]
pub(crate) struct ScopeInstallation<'slot, 'scope, 'domain> {
    slot: &'slot mut ScopeSlot<'scope, 'domain>,
    previous: Option<NonNull<InstalledScope<'scope, 'domain>>>,
}
impl<'scope, 'domain> ScopeInstallation<'_, 'scope, 'domain> {
    pub(crate) fn install<'nested>(
        &'nested mut self,
        installed: &'scope InstalledScope<'scope, 'domain>,
    ) -> ScopeInstallation<'nested, 'scope, 'domain> {
        self.slot.install(installed)
    }
    pub(crate) fn slot(&self) -> &ScopeSlot<'scope, 'domain> {
        self.slot
    }
}
impl Drop for ScopeInstallation<'_, '_, '_> {
    fn drop(&mut self) {
        self.slot.current.set(self.previous);
    }
}

// Both supported architectures retain exactly the existing Cell<u64> word's
// footprint. The live THREAD_CACHE_EPOCH is not changed until reader cutover.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const _: () = {
    assert!(std::mem::size_of::<ScopeSlot<'static, 'static>>() == std::mem::size_of::<Cell<u64>>());
    assert!(
        std::mem::align_of::<ScopeSlot<'static, 'static>>() == std::mem::align_of::<Cell<u64>>()
    );
    assert!(std::mem::size_of::<ScopeSlot<'static, 'static>>() == 8);
    assert!(std::mem::align_of::<ScopeSlot<'static, 'static>>() == 8);
};
static_assertions::assert_not_impl_any!(ScopeSlot<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ScopeInstallation<'static, 'static, 'static>: Send, Sync, Clone, Copy);
