//! Reclamation policy for explicit collection and automatic heap destruction.

use super::*;

/// Native callbacks and resource close operations require an explicit caller.
/// Automatic destruction only releases Rust-owned storage with inert payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReclamationMode {
    Explicit,
    DropFallback,
}

static_assertions::assert_impl_all!(ReclamationMode: Send, Sync, Copy, std::fmt::Debug);

#[derive(Clone, Copy, Debug)]
enum ObjectList {
    Young,
    Tenured,
    SweepPending,
    Old,
    OldSweepPending,
}

static_assertions::assert_impl_all!(ObjectList: Send, Sync, Copy, std::fmt::Debug);

impl TaggedHeap {
    /// Drain residual Box ownership after the marker has finished. Each head
    /// advances before resource teardown, so an unwinding explicit shutdown
    /// cannot make automatic Drop revisit a callback or freed allocation.
    pub(super) fn reclaim_intrusive_objects(&mut self, mode: ReclamationMode) {
        for list in [
            ObjectList::Young,
            ObjectList::Tenured,
            ObjectList::SweepPending,
            ObjectList::Old,
            ObjectList::OldSweepPending,
        ] {
            loop {
                let head = match list {
                    ObjectList::Young => self.all_objects,
                    ObjectList::Tenured => self.tenured_objects,
                    ObjectList::SweepPending => self.sweep_noncons_pending,
                    ObjectList::Old => self.generational.old_objects,
                    ObjectList::OldSweepPending => self.generational.old_sweep_pending,
                };
                if head.is_null() {
                    break;
                }
                // SAFETY: all five lists contain disjoint, exclusively owned
                // residual Box allocations; the marker has finished, and the
                // list head is detached before any callback or destruction.
                let next = unsafe { (*head).gc_link() };
                match list {
                    ObjectList::Young => self.all_objects = next,
                    ObjectList::Tenured => self.tenured_objects = next,
                    ObjectList::SweepPending => self.sweep_noncons_pending = next,
                    ObjectList::Old => self.generational.old_objects = next,
                    ObjectList::OldSweepPending => self.generational.old_sweep_pending = next,
                }
                // SAFETY: the detached allocation belongs to this heap alone
                // and no marker can read it. The mode bounds external teardown.
                unsafe { self.free_gc_object(head, mode) };
            }
        }
    }
}
