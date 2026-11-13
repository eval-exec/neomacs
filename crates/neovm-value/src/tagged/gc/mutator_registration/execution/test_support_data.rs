//! Context-free execution-home read primitives shared with eval's
//! `test_support::execution_fixtures` (SHAPE (B), LEAD 10:4xZ).
//!
//! These are the VIS half of the former `ExecutionTestHomes` adapter: copied
//! reads of one resident home's word. They add no authority path - the word
//! read is the existing `OwnerRootSlot::word_for_test` route (AUTHORITY row
//! 27) and the nil mint goes through the `nil_root_for_test` fence, not here.
use super::*;

/// C4 copied read primitive (observe-only): decode one immediate result home.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn read_cell_home(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> ImmediateResult {
    ImmediateResult::from_word(home.word_for_test(run)).expect("scalar result home is immediate")
}

/// C4 copied read primitive (observe-only): the cons tag of one home's word.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn cell_home_is_cons(home: &OwnerRootSlot<'_>, run: &RunAccess<'_, '_>) -> bool {
    // SAFETY: the word comes from this live owner's rooted home; testing the
    // cons tag on it neither retains nor dereferences the pointer.
    unsafe { TaggedValue::from_bits(home.word_for_test(run)) }.is_cons()
}
