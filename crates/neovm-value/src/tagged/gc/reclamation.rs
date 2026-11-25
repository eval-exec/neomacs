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
        assert!(
            !self.has_unfinished_concurrent_mark(),
            "reclaim_intrusive_objects cannot free objects while a concurrent mark is unfinished"
        );
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

impl Drop for TaggedHeap {
    fn drop(&mut self) {
        // Retire the current (possibly moved) inline scalar home before any
        // abandonment or field destruction can change its backing lifetime.
        self.detach_store_protocol();
        // Explicit finish is the only blocking completion handoff. Drop
        // cannot establish exclusive ownership while a marker or facade
        // reader obligation remains, so its fallback retains the backing
        // allocations and returns without waiting for either kind of reader.
        if self.has_unfinished_concurrent_mark()
            || self.facade_mark_is_excluded()
            || self._satb_layout_reserve.has_outstanding_young_loans()
            || self.has_owner_cons_reserves()
        {
            // A rejected consuming shutdown still reaches this Drop. Young
            // loans may outlive the owner even without an active marker or
            // facade. An owner reserve's closed or forgotten book likewise
            // retains its blocks until explicit return, so retain every
            // backing instead of freeing it here.
            self.abandon_concurrent_mark();
            if let Some(collector) = retire_collector(self.identity) {
                collector.detach();
            }
            // Storage is retained and the heap is being destroyed; release its
            // cold side-table entry so abandoned heaps do not leak HeapIdentity.
            // SAFETY: `abandon_concurrent_mark()` + `collector.detach()` above
            // retained every backing allocation and `Drop` returns immediately
            // without freeing storage reachable by any live worker.
            unsafe { mark_channel::retire_heap(self.identity) };
            // SAFETY: `TaggedHeap::drop` is tearing down this heap; no active
            // `Context` can evaluate on it after drop begins.
            unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(self) };
            return;
        }
        // No job is in flight, so the heap's collector is idle in `recv`:
        // joining it returns at once, and no collector outlives its heap.
        // SAFETY: `!self.has_unfinished_concurrent_mark()` held at entry above,
        // so no concurrent mark is active on `self.identity`.
        unsafe { mark_channel::retire_heap(self.identity) };
        if let Some(collector) = retire_collector(self.identity) {
            collector.join();
        }
        self._satb_layout_reserve.abandon();
        // No marker is active, so Tier-H's snapshot and retired originals are
        // freed with the claims state when the census carrier drops below;
        // Drop takes none of its locks.
        // Leave no dangling thread-local pointer behind: a heap installed with
        // `set_tagged_heap` and dropped by anything but a `Context` (a failed
        // pdump load drops the half-built one on its error path) used to stay
        // installed, and the next allocation on this thread wrote into freed
        // memory. The identity check makes this a no-op when another heap is
        // installed -- and when a `Context` drop already uninstalled this one,
        // which is also why the frees below run with no heap installed either
        // way.
        // SAFETY: `TaggedHeap::drop` is tearing down this heap; no active
        // `Context` can evaluate on it after drop begins.
        unsafe { crate::tagged::gc::clear_tagged_heap_if_installed(self) };
        // Automatic destruction cannot invoke module finalizers or SQLite
        // teardown. Explicit shutdown reclaims those resources beforehand;
        // the fallback retains native payloads and frees inert Rust storage.
        self.reclaim_intrusive_objects(ReclamationMode::DropFallback);
        // ConsBlocks are dropped automatically (they implement Drop).
        // Object arena pages likewise: page floats/strings/vectors/bytecode/
        // lambdas/macros/records/symbols-with-pos are on NONE of the lists
        // above (so the walk cannot hand a page pointer to `free_gc_object`'s
        // `Box::from_raw`), and the arena fields drop after this body, freeing
        // every page via `ObjectPage::drop`, which walks the allocated slots
        // and `drop_in_place`s each live object (strings free their byte
        // storage + interval tables, vectors their element `Vec`, bytecode its
        // ops/constants vectors + params + GNU byte maps + docstring,
        // lambdas/macros/records their slot `Vec` + cached params; floats and
        // symbols-with-pos are POD and the walk compiles out) before releasing
        // the page storage —
        // retired pages included. This path only runs without an active mark,
        // so the GC thread cannot still be reading a page.
    }
}
