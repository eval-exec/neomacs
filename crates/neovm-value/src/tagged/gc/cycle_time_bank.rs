//! Per-cycle collector time for one heap.
//!
//! GNU keeps one process-wide `gc_elapsed` and sets `gc-elapsed` to that
//! running total (`alloc.c`). This bank is the scratch for the cycle in
//! progress. Every Context of the heap adds its slice here. The call that
//! completes the cycle takes the sum and publishes it once. The next cycle
//! starts at zero.

/// Time banked for the collection that has not been published yet.
#[derive(Clone, Copy, Default)]
pub(crate) struct GcCycleTimeBank(std::time::Duration);

impl GcCycleTimeBank {
    /// Add one Context's slice to the shared bank.
    pub(crate) fn bank(&mut self, slice: std::time::Duration) {
        self.0 += slice;
    }

    /// Take the bank. The caller publishes it. The next cycle starts at zero.
    pub(crate) fn take(&mut self) -> std::time::Duration {
        std::mem::take(&mut self.0)
    }

    /// Time banked for the cycle in progress.
    pub(crate) fn peek(&self) -> std::time::Duration {
        self.0
    }
}
