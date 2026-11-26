//! Cold bootstrap refusal through the existing World and function policy.
//! The green slot is read with `try_borrow_mut`. No mutation is returned.

use super::super::parallel_entry::source_entry::SourceEntryRefusal;
use super::*;

impl Context {
    #[cold]
    pub(in crate::emacs_core::eval) fn validate_serial_source_world_entry(
        &self,
    ) -> Result<(), SourceEntryRefusal> {
        let green = self
            .obarray
            .lease
            .0
            .green
            .try_borrow_mut()
            .map_err(|_| SourceEntryRefusal::GreenSlotBorrowed)?;
        if green.is_some() {
            return Err(SourceEntryRefusal::ExistingGreenRuntime);
        }
        drop(green);
        // Reuse the actual N2/C4 policy, including specpdl and native checks.
        // Inactive compiled caches stay admitted.
        if !self.function_cells_quiescent() {
            return Err(SourceEntryRefusal::FunctionActivation);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/source_entry_test.rs"]
mod tests;
