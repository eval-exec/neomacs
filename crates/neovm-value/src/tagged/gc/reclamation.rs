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

impl Drop for TaggedHeap {
    fn drop(&mut self) {
        // Retire the current (possibly moved) inline scalar home before any
        // abandonment or field destruction can change its backing lifetime.
        self.detach_store_protocol();
        // Explicit finish is the only blocking completion handoff. Drop
        // cannot establish exclusive ownership while a marker or facade
        // reader obligation remains, so its fallback retains the backing
        // allocations and returns without waiting for either kind of reader.
        if self.concurrent_mark_running
            || self.facade_mark_is_excluded()
            || self._satb_layout_reserve.has_outstanding_young_loans()
        {
            // A rejected consuming shutdown still reaches this Drop. Young
            // loans may outlive the owner even without an active marker or
            // facade, so retain every backing instead of freeing it here.
            self.abandon_concurrent_mark();
            // The collector may still be running the abandoned job: detach
            // it rather than wait.
            //
            // SAFETY (ordering): the detached thread cannot touch this heap's
            // freed memory.
            // - The job never dereferences the heap (`ConcurrentMarkJob`; the
            //   mapped-veclike snapshot keeps a heap pointer for identity
            //   comparison only). It reads its own snapshots and queues, and
            //   through them the blocks, pages, vector backings and hash
            //   storage that `abandon_concurrent_mark` just retained
            //   (forgot) for it. Leased obarray chunks outlive their owner
            //   the same way.
            // - `stop` was stored (Release) above, so the job exits at its
            //   next check. Its final `exited` send fails harmlessly once
            //   `gc_exited` drops with this heap.
            // - Retiring closes the request channel, so after the job `recv`
            //   fails and the thread ends. No other request can be queued:
            //   `MarkAll` is synchronous (its sender is blocked until done,
            //   so none is pending while the owner runs Drop), and only one
            //   concurrent job runs per heap.
            if let Some(collector) = retire_collector(self.identity) {
                collector.detach();
            }
            crate::tagged::gc::clear_tagged_heap_if_installed(self);
            return;
        }
        // No job is in flight, so the heap's collector is idle in `recv`:
        // joining it returns at once, and no collector outlives its heap.
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
        crate::tagged::gc::clear_tagged_heap_if_installed(self);
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
