//! Root enumeration at the generational collector's stop-all boundary.

use super::*;

impl Context {
    /// The current runtime registers one mutator. A future runtime registry
    /// can supply the same iterator after stopping all its mutators.
    #[inline]
    fn stopped_mutators_world_stopped(&mut self) -> impl Iterator<Item = &mut Context> {
        std::iter::once(self)
    }

    /// Seed every registered mutator's roots without keeping an unrooted
    /// collection of Values across allocation or a safe point.
    ///
    /// # Safety
    ///
    /// Every registered mutator is stopped; `heap_ptr` names their collector
    /// heap with no concurrent marker running. Enumeration reads Context
    /// state and only appends roots to the collector's gray worklist.
    #[cold]
    #[inline(never)]
    pub(super) unsafe fn seed_registered_mutator_roots_world_stopped(
        &mut self,
        heap_ptr: *mut crate::tagged::gc::TaggedHeap,
    ) {
        for mutator in self.stopped_mutators_world_stopped() {
            unsafe { mutator.seed_all_context_roots(heap_ptr) };
        }
    }
}
