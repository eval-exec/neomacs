//! Lifetime allocation total for a heap.
//!
//! Moved out of `gc.rs` so the facade can gain the trailing cycle-time bank
//! and stay within its line ceiling.

impl super::TaggedHeap {
    /// Every byte this heap has ever allocated.
    ///
    /// Derived rather than counted: `bytes_banked_at_resets` is advanced by
    /// [`Self::reset_bytes_since_gc`], the one place `bytes_since_gc` returns
    /// to zero, so the sum is exact by construction.
    #[doc(hidden)]
    pub fn total_allocated_bytes(&self) -> u64 {
        let delta = self.allocation_region_deltas();
        self.mutators()
            .map(|state| {
                state
                    .bytes_banked_at_resets
                    .saturating_add(state.exact_bytes_since_gc(delta) as u64)
            })
            .fold(0, u64::saturating_add)
    }
}
