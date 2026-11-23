//! Owned observations of the inert numeric shell through its current designator.

use super::SuspendedConsCollector;
use crate::tagged::gc::arena_pages::ArenaStorageSnapshot;

impl SuspendedConsCollector<'_> {
    pub(in crate::tagged::gc) fn numeric_storage_snapshot_for_test(
        &self,
    ) -> (ArenaStorageSnapshot, ArenaStorageSnapshot) {
        let current = self.heap.as_ptr();
        // SAFETY: suspension minted this current designator after the same
        // catalog install detached the real arenas. The original exclusive
        // collector loan is suspended. Only the two inert placeholder fields
        // are borrowed here; no whole-heap reference is created. The detached
        // real arenas are never touched, and only owned scalar/Vec metadata
        // returns, so no field or storage borrow escapes this observation.
        unsafe {
            (
                (&*std::ptr::addr_of!((*current).float_arena)).storage_snapshot_for_test(),
                (&*std::ptr::addr_of!((*current).bignum_arena)).storage_snapshot_for_test(),
            )
        }
    }
}
