//! Per-cycle collector time for one heap.
//!
//! GNU keeps one process-wide `gc_elapsed` and sets `gc-elapsed` to that
//! running total (`alloc.c`). This bank is the scratch for the cycle in
//! progress. Every Context of the heap adds its slice here. The call that
//! completes the cycle takes the sum and publishes it once. The next cycle
//! starts at zero.

/// Time banked for the collection that has not been published yet, plus the
/// context-root seed scratch for `NEOVM_GC_TRACE`.
///
/// The seed words ride this trailing bank so the facade struct stays at its
/// line ceiling and no pinned `TaggedHeap` offset moves. `take` publishes
/// only the elapsed duration. A later cycle's seed does not rewrite the
/// microseconds already frozen for a deferred sweep line.
#[derive(Clone, Copy, Default)]
pub(crate) struct GcCycleTimeBank {
    elapsed: std::time::Duration,
    context_seed_us: u64,
    sweep_context_seed_us: u64,
}

impl GcCycleTimeBank {
    /// Add one Context's slice to the shared bank.
    pub(crate) fn bank(&mut self, slice: std::time::Duration) {
        self.elapsed += slice;
    }

    /// Take the bank. The caller publishes it. The next cycle starts at zero.
    pub(crate) fn take(&mut self) -> std::time::Duration {
        std::mem::take(&mut self.elapsed)
    }

    /// Time banked for the cycle in progress.
    pub(crate) fn peek(&self) -> std::time::Duration {
        self.elapsed
    }

    /// Record one context-root seed (`seed_all_context_roots` total).
    pub(crate) fn note_context_seed(&mut self, us: u64) {
        self.context_seed_us = us;
    }

    /// Freeze the seed for the incremental line before a later seed can land.
    pub(crate) fn snapshot_context_seed(&mut self) {
        self.sweep_context_seed_us = self.context_seed_us;
    }

    pub(crate) fn context_seed_us(&self) -> u64 {
        self.context_seed_us
    }

    pub(crate) fn sweep_context_seed_us(&self) -> u64 {
        self.sweep_context_seed_us
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_snapshot_keeps_the_seed_that_armed_it() {
        let mut bank = GcCycleTimeBank::default();
        bank.note_context_seed(528);
        bank.snapshot_context_seed();
        bank.note_context_seed(11);
        assert_eq!(bank.context_seed_us(), 11);
        assert_eq!(bank.sweep_context_seed_us(), 528);
        assert_eq!(bank.take(), std::time::Duration::ZERO);
        assert_eq!(bank.sweep_context_seed_us(), 528);
        assert_eq!(bank.peek(), std::time::Duration::ZERO);
    }
}
