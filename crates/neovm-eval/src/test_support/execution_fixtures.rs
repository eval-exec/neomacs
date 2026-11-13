//! Hosted execution-fixture helpers (SHAPE (B), LEAD 10:4xZ).
//!
//! The Context-free home adapter lives here, beside the era-building fixtures,
//! and is built ONLY from value's published primitives: the existing
//! `OwnerRootSlot::word_for_test` route (AUTHORITY-ITEMS row 27), the
//! `read_cell_home` / `cell_home_is_cons` VIS primitives, and the
//! `nil_root_for_test` fence. Every call site keeps its
//! `ExecutionTestHomes::*` shape.

use neovm_value::tagged::gc::mutator_registration::execution::test_support_data::{
    cell_home_is_cons, read_cell_home,
};
use neovm_value::tagged::gc::{ImmediateResult, OwnerRootSlot, RegistrationError, RunAccess};

/// Context-free execution home adapter; every operation delegates to a
/// published value primitive and adds no new authority path.
pub struct ExecutionTestHomes;
impl ExecutionTestHomes {
    /// The existing AUTHORITY row 27 route; no new word export is added.
    pub fn word(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> usize {
        home.word_for_test(run)
    }

    pub fn read(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> ImmediateResult {
        read_cell_home(home, run)
    }

    pub fn is_cons(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> bool {
        cell_home_is_cons(home, run)
    }

    pub fn nil<'era>(
        run: &mut RunAccess<'_, 'era>,
    ) -> Result<OwnerRootSlot<'era>, RegistrationError> {
        neovm_value::test_support::authority::nil_root_for_test(run)
    }
}
