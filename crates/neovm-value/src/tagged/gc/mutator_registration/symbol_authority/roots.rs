//! Keyless stopped census. This receiver never mints running access.
use super::*;
use crate::tagged::value::TaggedValue;

impl KeeperSymbols<'_> {
    /// Check the exact sidecar and original World roots before dereferencing
    /// storage. A heap address alone is not a World/obarray identity.
    pub(crate) fn check_symbol_backing(
        &self,
        backing: &SymbolBacking,
        keeper: &KeeperWorldDesignator<'_>,
    ) -> Result<(), RegistrationError> {
        if self.registry.keeper_thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread);
        }
        if self.registry.heap != keeper.heap() {
            return Err(RegistrationError::WrongHeap);
        }
        if !std::ptr::eq(backing, &self.registry.symbols.backing) {
            return Err(RegistrationError::StaleRegistration);
        }
        self.registry.symbols.ready()?;
        backing.check_root_designators(keeper.obarray(), keeper.projections())
    }

    /// Copy current cells and residual pools, including nonmember slots.
    /// The stop remains exclusively borrowed until this receiver drops.
    pub(crate) fn append_current_symbol_roots(
        &self,
        keeper: &KeeperWorldDesignator<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<(), RegistrationError> {
        // SAFETY: only AllStopped mints this receiver and its exclusive loan
        // prevents resume. The bridge revalidates word, identity and generation.
        unsafe {
            self.registry
                .symbols
                .backing
                .append_current_symbol_roots(self, keeper, out)
        }
    }

    /// Copy every currently issued canonical projection, not an admission
    /// snapshot. VOID has no managed word; no descriptor reference escapes.
    pub(crate) fn append_current_projection_roots(
        &self,
        keeper: &KeeperWorldDesignator<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<(), RegistrationError> {
        // SAFETY: matching exclusive stopped keeper as above, with table loans
        // confined to the validated bridge and released before collector work.
        unsafe {
            self.registry
                .symbols
                .backing
                .append_current_projection_roots(self, keeper, out)
        }
    }
}

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
mod tests;
