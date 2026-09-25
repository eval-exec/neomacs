//! The write barrier's owner window: the one address range whose owners must
//! leave the inline barrier for its out-of-line part.
//!
//! Every state that makes `record_heap_write` do something is folded into a
//! single `(lo, len)` range over owner addresses:
//!
//! - **ALL** while a concurrent mark runs (the SATB log) or owner tracking is
//!   on (the dirty-owner tables): every heap write is recorded.
//! - **The dump span** when only the dump partition is active (the steady
//!   state of a session with a loaded pdump): a write by a mapped owner may
//!   add it to the remembered set.
//! - **EMPTY** otherwise.
//!
//! An owner at address `a` needs the out-of-line barrier iff
//! `a.wrapping_sub(lo) < len`, or it is a non-cons owner that is tenured and
//! not yet remembered (see `barrier_gate`). A cons is never tenured, so
//! outside the window a cons store is a plain store.
//!
//! The window is PROTOCOL STATE, like `TAGGED_HEAP_CONCURRENT_ACTIVE`: it is
//! recomputed and republished by [`TaggedHeap::publish_barrier_window`] at
//! every writer of one of its inputs (`set_write_tracking_mode`,
//! `extend_dump_span`, `launch_concurrent_mark`, `join_concurrent_mark`), and
//! re-derived whenever a heap is (re)installed on a thread
//! (`set_tagged_heap`). There is no drop guard, for the reason the
//! concurrent flag has none: its true-window spans many mutator frames.

use super::*;

/// A half-open owner-address range `[lo, lo + len)`, tested with one
/// wrapping subtraction and one unsigned compare.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct BarrierWindow {
    lo: usize,
    len: usize,
}

impl BarrierWindow {
    /// No owner is covered.
    pub(crate) const NONE: Self = Self { lo: 0, len: 0 };
    /// Every owner is covered (heap addresses are 8-aligned, so the one
    /// address `usize::MAX` this leaves out is never an owner).
    pub(crate) const ALL: Self = Self {
        lo: 0,
        len: usize::MAX,
    };

    /// The window over `[lo, hi)`; empty when `hi <= lo`.
    pub(crate) fn span(lo: usize, hi: usize) -> Self {
        if hi <= lo {
            Self::NONE
        } else {
            Self { lo, len: hi - lo }
        }
    }

    /// Whether an owner at `addr` must take the out-of-line barrier.
    #[inline(always)]
    pub(crate) fn covers(self, addr: usize) -> bool {
        addr.wrapping_sub(self.lo) < self.len
    }

    /// The window's first address (what compiled code subtracts).
    #[inline(always)]
    pub(crate) fn lo(self) -> usize {
        self.lo
    }

    /// The window's length in bytes (what compiled code compares against).
    #[inline(always)]
    pub(crate) fn len(self) -> usize {
        self.len
    }
}

impl TaggedHeap {
    /// The window this heap's current state implies (see the module doc).
    pub(crate) fn barrier_window(&self) -> BarrierWindow {
        if self.concurrent_mark_running || self.write_tracking_mode != WriteTrackingMode::Disabled {
            BarrierWindow::ALL
        } else if self.partition_dump {
            // `extend_dump_span` sets the partition only with a non-empty
            // span, so `lo < hi` here.
            debug_assert!(self.dump_addr_lo < self.dump_addr_hi);
            BarrierWindow::span(self.dump_addr_lo, self.dump_addr_hi)
        } else {
            BarrierWindow::NONE
        }
    }

    /// THE publisher of the barrier window: recompute it from this heap's
    /// state and store it where the barriers read it — the thread-local
    /// mirror the Rust stores test and the heap field compiled code tests
    /// (`JitHeapState`). Called at every writer of an input.
    pub(super) fn publish_barrier_window(&mut self) {
        let window = self.barrier_window();
        self.jit.set_barrier_window(window);
        TAGGED_HEAP_BARRIER_WINDOW.with(|w| w.set(window));
    }

    /// Test hook: arm or disarm the concurrent SATB barrier exactly as
    /// `launch_concurrent_mark` / `join_concurrent_mark` do, without a GC
    /// thread (so nothing drains `satb_shared`). Tests used to flip the heap
    /// flag and its thread-local mirror by hand, which would now leave the
    /// barrier window stale and test the old gate.
    #[cfg(test)]
    pub(crate) fn set_concurrent_active_for_test(&mut self, on: bool) {
        self.concurrent_mark_running = on;
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(on));
        self.publish_barrier_window();
    }
}

#[cfg(test)]
impl TaggedHeap {
    /// Test hook: drain the SATB buffer the barrier logs pre-images into.
    pub(crate) fn take_satb_shared_for_test(&mut self) -> Vec<TaggedValue> {
        std::mem::take(&mut *self.satb_shared.lock().unwrap())
    }

    /// Test hook: is `owner` in the dump remembered set?
    pub(crate) fn is_remembered_for_test(&self, owner: TaggedValue) -> bool {
        self.mapped_remembered.contains(&owner.bits())
    }

    /// Test hook: is `value` a tenured (old-generation) heap object?
    pub(crate) fn is_tenured_for_test(&self, value: TaggedValue) -> bool {
        self.value_is_tenured(value)
    }

    /// Test hook: the window compiled code tests (`JitHeapState`).
    pub(crate) fn jit_barrier_window_for_test(&self) -> BarrierWindow {
        self.jit.barrier_window()
    }
}

/// The barrier window this thread's stores currently test.
#[cfg(test)]
pub(crate) fn published_barrier_window() -> BarrierWindow {
    TAGGED_HEAP_BARRIER_WINDOW.with(|w| w.get())
}
