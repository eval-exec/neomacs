//! Sealed concurrent-mark completion channel and active flag (value-gc C2, G1).
//!
//! [`TaggedHeap::gc_exited`] and [`TaggedHeap::concurrent_mark_running`] sit
//! on the unsafe trust boundary of concurrent marking. Because `TaggedHeap` is
//! declared in parent module `gc.rs` (with pinned layout offsets), descendant
//! modules of `tagged::gc` can still perform whole-field assignment or
//! `std::mem::swap` on those fields. What prevents UAF at consumption time is:
//! - sealing direct wrapper construction/mutation behind private fields and
//!   `unsafe fn` transitions (`install_for_heap`, `set`);
//! - maintaining a private cold side-table (`ACTIVE_MARK_HEAPS`, keyed by
//!   `HeapIdentity`) updated at launch/finish/retirement so every finish,
//!   sweep, collection-start, and teardown consumer detects a cleared or
//!   swapped flag/channel and fails closed;
//! - checking `result.heap == expected_heap` in [`MarkExitSlot::take_for_finish`]
//!   and failing closed with [`MarkFinishError::PoisonedCollectorState`] before
//!   clearing `concurrent_mark_running` or materializing any deferred `MarkWord`.
//!
//! Both `#[repr(transparent)]` wrappers preserve `TaggedHeap`'s exact field
//! offsets (`gc_exited == 912`, `concurrent_mark_running == 4522`) and size
//! byte-for-byte.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::mpsc::Receiver;

/// Cold side-table of heaps whose concurrent mark has been installed via
/// [`MarkExitSlot::install_for_heap`] and not yet proven finished via
/// [`complete_for_heap`] or retired at teardown via [`retire_heap`].
///
/// Private to this leaf module so no descendant of `tagged::gc` can clear or
/// swap it by whole-field assignment on `TaggedHeap`.
static ACTIVE_MARK_HEAPS: Mutex<Option<HashSet<super::HeapIdentity>>> = Mutex::new(None);

fn with_active_heaps<R>(f: impl FnOnce(&mut HashSet<super::HeapIdentity>) -> R) -> R {
    let mut guard = ACTIVE_MARK_HEAPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let set = guard.get_or_insert_with(HashSet::new);
    f(set)
}

/// True if `heap` has an active concurrent mark recorded in the sealed
/// side-table (not yet finished or retired).
#[inline]
pub(super) fn is_active_for_heap(heap: super::HeapIdentity) -> bool {
    let guard = ACTIVE_MARK_HEAPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.as_ref().is_some_and(|set| set.contains(&heap))
}

/// Mark `heap`'s concurrent mark as completed after `finish_concurrent_mark`
/// has verified the worker handoff and all collector invariants.
///
/// # Safety
/// The worker for `heap` must have exited and its `ConcurrentMarkResult`
/// (matching `heap`), SATB handoff, and Tier-H logs must all have been
/// verified unpoisoned before clearing active status.
#[inline]
pub(super) unsafe fn complete_for_heap(heap: super::HeapIdentity) {
    with_active_heaps(|set| {
        set.remove(&heap);
    });
}

/// Remove `heap` from the cold side-table at `TaggedHeap::drop` *after*
/// storage abandonment and collector detachment have completed, or after
/// confirming no concurrent mark is unfinished.
///
/// # Safety
/// Either no concurrent mark is unfinished on `heap`, or `TaggedHeap::drop` has
/// already abandoned the concurrent mark, detached its collector, and retained
/// every backing allocation so no storage reachable by a live worker will be
/// freed.
#[inline]
pub(super) unsafe fn retire_heap(heap: super::HeapIdentity) {
    with_active_heaps(|set| {
        set.remove(&heap);
    });
}

/// Sealed `#[repr(transparent)]` wrapper around `TaggedHeap::gc_exited`.
#[repr(transparent)]
pub(super) struct MarkExitSlot(Option<Receiver<super::ConcurrentMarkResult>>);

/// Sealed `#[repr(transparent)]` wrapper around `TaggedHeap::concurrent_mark_running`.
#[repr(transparent)]
pub(super) struct ConcurrentMarkFlag(bool);

const _: () = {
    assert!(
        core::mem::size_of::<MarkExitSlot>()
            == core::mem::size_of::<Option<Receiver<super::ConcurrentMarkResult>>>()
    );
    assert!(
        core::mem::align_of::<MarkExitSlot>()
            == core::mem::align_of::<Option<Receiver<super::ConcurrentMarkResult>>>()
    );
    assert!(core::mem::size_of::<ConcurrentMarkFlag>() == core::mem::size_of::<bool>());
    assert!(core::mem::align_of::<ConcurrentMarkFlag>() == core::mem::align_of::<bool>());
};

impl MarkExitSlot {
    #[inline(always)]
    pub(super) const fn empty() -> Self {
        Self(None)
    }

    #[inline(always)]
    pub(super) fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Install the completion receiver for a concurrent mark launched on `heap`
    /// and record `heap` in the sealed active-mark side-table.
    ///
    /// # Safety
    /// `rx` must be paired with a concurrent mark job launched for `heap` whose
    /// worker sends a [`ConcurrentMarkResult`] built exclusively from live
    /// objects belonging to `heap` and retained until join.
    #[inline]
    pub(super) unsafe fn install_for_heap(
        &mut self,
        heap: super::HeapIdentity,
        rx: Receiver<super::ConcurrentMarkResult>,
    ) {
        with_active_heaps(|set| {
            set.insert(heap);
        });
        self.0 = Some(rx);
    }

    /// Take the installed receiver, block for the worker result, and verify
    /// that its [`HeapIdentity`] matches `expected_heap`.
    ///
    /// Fails closed with [`MarkFinishError::PoisonedCollectorState`] on a
    /// heap-identity mismatch so no foreign [`MarkWord`] can reach the
    /// mutator's `gray_queue`, leaving `expected_heap` active in the sealed
    /// side-table.
    pub(super) fn take_for_finish(
        &mut self,
        expected_heap: super::HeapIdentity,
    ) -> Result<(Vec<super::MarkWord>, Vec<usize>, Vec<super::SymId>), super::MarkFinishError> {
        let rx = self
            .0
            .take()
            .ok_or(super::MarkFinishError::MissingCompletion)?;
        let result = rx.recv().map_err(super::MarkFinishError::Disconnected)?;
        result
            .into_parts_for_heap(expected_heap)
            .map_err(|_| super::MarkFinishError::PoisonedCollectorState)
    }

    /// Test-only: clear the installed receiver without clearing the active
    /// side-table (for `MissingCompletion` failure fixtures).
    ///
    /// Not gated on `#[cfg(test)]` because `tagged/gc/tests/` fixtures are also
    /// compiled under `feature = "test-support"`, and trybuild mounts this file
    /// directly via `#[path]` where `test-support` is not declared. Requiring
    /// `unsafe` prevents safe code in `tagged::gc` from invoking it.
    ///
    /// # Safety
    /// No real concurrent marker may be using this slot (serial synthetic
    /// fixture only).
    #[doc(hidden)]
    pub(super) unsafe fn clear_for_test(&mut self) {
        self.0 = None;
    }

    /// Test-only: tear down an inert sentinel receiver AND remove `heap` from
    /// the active side-table (when no real worker ever ran).
    ///
    /// Same `#[doc(hidden)]` + `unsafe fn` rationale as [`Self::clear_for_test`].
    ///
    /// # Safety
    /// No real concurrent marker may have been launched on `heap`.
    #[doc(hidden)]
    pub(super) unsafe fn clear_sentinel_for_test(&mut self, heap: super::HeapIdentity) {
        self.0 = None;
        with_active_heaps(|set| {
            set.remove(&heap);
        });
    }

    /// Test-only: detach the installed receiver for abandonment/inspection
    /// fixtures.
    ///
    /// # Safety
    /// The caller takes responsibility for either relaying the result back via
    /// [`Self::install_for_heap`] or abandoning/retaining storage without sweeping.
    #[cfg(test)]
    pub(super) unsafe fn take_receiver_for_test(
        &mut self,
    ) -> Option<Receiver<super::ConcurrentMarkResult>> {
        self.0.take()
    }
}

impl ConcurrentMarkFlag {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self(false)
    }

    #[inline(always)]
    pub(super) const fn get(&self) -> bool {
        self.0
    }

    /// Transition the concurrent-mark active flag.
    ///
    /// # Safety
    /// Must be updated in lockstep with `gc_exited`, `close_alloc_regions()`,
    /// the thread-local barrier mirrors, and `publish_barrier_window()`.
    /// Clearing while a worker runs disables the SATB barrier and can free
    /// live objects; setting without an active/paired completion channel breaks
    /// join and shutdown invariants.
    #[inline(always)]
    pub(super) unsafe fn set(&mut self, running: bool) {
        self.0 = running;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagged::gc::{MarkFinishError, MarkWord, TaggedHeap};
    use crate::tagged::value::TaggedValue;

    #[test]
    fn take_for_finish_rejects_mismatched_heap_identity() {
        let mut heap_a = TaggedHeap::new();
        let heap_b = TaggedHeap::new();
        assert_ne!(heap_a.heap_identity(), heap_b.heap_identity());

        let mut slot = MarkExitSlot::empty();
        let (tx, rx) = std::sync::mpsc::channel();
        // SAFETY: test-only synthetic channel exercising the mismatch guard.
        unsafe { slot.install_for_heap(heap_a.heap_identity(), rx) };
        let forged = unsafe {
            super::super::ConcurrentMarkResult::from_parts(
                heap_b.heap_identity(),
                vec![MarkWord::nil()],
                vec![0x1000],
                vec![],
            )
        };
        tx.send(forged).unwrap();

        let err = slot.take_for_finish(heap_a.heap_identity()).unwrap_err();
        assert!(matches!(err, MarkFinishError::PoisonedCollectorState));
        assert!(!slot.is_some());
        assert!(is_active_for_heap(heap_a.heap_identity()));
        // SAFETY: synthetic channel fixture in unit test; no real worker ran.
        unsafe { retire_heap(heap_a.heap_identity()) };

        // End-to-end on TaggedHeap::finish_concurrent_mark: fail-closed leaves
        // `concurrent_mark_running == true` and `gray_queue` untouched.
        let (tx2, rx2) = std::sync::mpsc::channel();
        // SAFETY: synthetic serial fixture, no live worker.
        unsafe {
            heap_a.concurrent_mark_running.set(true);
            heap_a
                .gc_exited
                .install_for_heap(heap_a.heap_identity(), rx2);
        }
        let forged2 = unsafe {
            super::super::ConcurrentMarkResult::from_parts(
                heap_b.heap_identity(),
                vec![MarkWord::of(TaggedValue::fixnum(42))],
                vec![],
                vec![],
            )
        };
        tx2.send(forged2).unwrap();

        let finish_err = heap_a.finish_concurrent_mark().unwrap_err();
        assert!(matches!(
            finish_err,
            MarkFinishError::PoisonedCollectorState
        ));
        assert!(
            heap_a.concurrent_mark_running(),
            "fail-closed mismatch must keep concurrent_mark_running active"
        );
        assert!(
            heap_a.gray_queue.is_empty(),
            "fail-closed mismatch must not materialize foreign deferred words into gray_queue"
        );
    }

    #[test]
    fn take_for_finish_accepts_matching_heap_identity() {
        let heap = TaggedHeap::new();
        let expected = heap.heap_identity();
        let mut slot = MarkExitSlot::empty();
        let (tx, rx) = std::sync::mpsc::channel();
        // SAFETY: test-only synthetic channel with matching heap identity and immediate word.
        unsafe { slot.install_for_heap(expected, rx) };
        let result = unsafe {
            super::super::ConcurrentMarkResult::from_parts(
                expected,
                vec![MarkWord::nil()],
                vec![],
                vec![],
            )
        };
        tx.send(result).unwrap();

        let (deferred, promo, symbols) = slot.take_for_finish(expected).unwrap();
        assert_eq!(deferred.len(), 1);
        assert_eq!(deferred[0].bits(), MarkWord::nil().bits());
        assert!(promo.is_empty());
        assert!(symbols.is_empty());
        // SAFETY: synthetic channel completed with matching heap identity.
        unsafe { complete_for_heap(expected) };
        assert!(!is_active_for_heap(expected));
    }

    /// Post-swap negative test (Round-4 Blocker #2): safe whole-field overwrite
    /// or `std::mem::swap` of `concurrent_mark_running` (alone OR together with
    /// `gc_exited`) must fail closed at `finish_concurrent_mark`, `sweep_objects`,
    /// `sweep_cons`, and `finalize_collection` without joining falsely or freeing
    /// live worker storage.
    #[test]
    fn overwritten_or_swapped_mark_channel_fails_closed_at_finish_and_sweep() {
        // Case 1: Overwrite ONLY `concurrent_mark_running = ConcurrentMarkFlag::new()`
        // while `gc_exited` + active side-table record an unfinished mark.
        {
            let mut heap = TaggedHeap::new();
            let (tx, rx) = std::sync::mpsc::channel();
            let heap_id = heap.heap_identity();
            // SAFETY: inert synthetic mark channel for post-overwrite consumer check.
            unsafe {
                heap.concurrent_mark_running.set(true);
                heap.gc_exited.install_for_heap(heap_id, rx);
            }
            // Exact 1-line safe bypass from Blocker #2:
            heap.concurrent_mark_running = ConcurrentMarkFlag::new();
            assert!(!heap.concurrent_mark_running());
            assert!(heap.has_unfinished_concurrent_mark());
            assert!(matches!(
                heap.finish_concurrent_mark(),
                Err(MarkFinishError::PoisonedCollectorState)
            ));
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| heap.sweep_objects()))
                    .is_err(),
                "sweep_objects must refuse while concurrent mark is unfinished"
            );
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| heap.sweep_cons()))
                    .is_err(),
                "sweep_cons must refuse while concurrent mark is unfinished"
            );
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    heap.finalize_collection(0, 0, std::time::Instant::now())
                }))
                .is_err(),
                "finalize_collection must refuse while concurrent mark is unfinished"
            );
            drop(tx);
            // Drop sees `has_unfinished_concurrent_mark() == true` and retains storage safely.
        }

        // Case 2: Swap BOTH `concurrent_mark_running` AND `gc_exited` between
        // active `heap_a` and idle `heap_b` so `heap_a` has `running == false`
        // AND `gc_exited == Empty`, while `heap_b` has `running == true` AND
        // `gc_exited == Some(rx_a)`.
        {
            let mut heap_a = TaggedHeap::new();
            let mut heap_b = TaggedHeap::new();
            let (tx_a, rx_a) = std::sync::mpsc::channel();
            let heap_a_id = heap_a.heap_identity();
            // SAFETY: inert synthetic mark channel bound to `heap_a`.
            unsafe {
                heap_a.concurrent_mark_running.set(true);
                heap_a.gc_exited.install_for_heap(heap_a_id, rx_a);
            }
            tx_a.send(unsafe {
                super::super::ConcurrentMarkResult::from_parts(
                    heap_a.heap_identity(),
                    vec![MarkWord::of(TaggedValue::fixnum(99))],
                    vec![],
                    vec![],
                )
            })
            .unwrap();

            // Exact safe double-swap bypass:
            std::mem::swap(
                &mut heap_a.concurrent_mark_running,
                &mut heap_b.concurrent_mark_running,
            );
            std::mem::swap(&mut heap_a.gc_exited, &mut heap_b.gc_exited);

            // On heap_a: both fields look idle, yet ACTIVE_MARK_HEAPS catches it!
            assert!(!heap_a.concurrent_mark_running());
            assert!(!heap_a.gc_exited.is_some());
            assert!(heap_a.has_unfinished_concurrent_mark());
            assert!(matches!(
                heap_a.finish_concurrent_mark(),
                Err(MarkFinishError::PoisonedCollectorState)
            ));
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| heap_a.sweep_objects()))
                    .is_err()
            );

            // On heap_b: fields look active with heap_a's channel, yet take_for_finish
            // catches `result.heap != heap_b.heap_identity()` and fails closed!
            assert!(matches!(
                heap_b.finish_concurrent_mark(),
                Err(MarkFinishError::PoisonedCollectorState)
            ));
            assert!(heap_b.gray_queue.is_empty());
            assert!(heap_b.has_unfinished_concurrent_mark());
        }
    }

    /// Post-overwrite negative test (Round-5 Blocker #2): after safe whole-field
    /// overwrite of BOTH `concurrent_mark_running = ConcurrentMarkFlag::new()`
    /// AND `gc_exited = MarkExitSlot::empty()`, ONLY the private cold side-table
    /// `ACTIVE_MARK_HEAPS` records the unfinished mark. Every reclaimer, SATB
    /// fold, and admission consumer must still fail closed; each assertion below
    /// fails if its `has_unfinished_concurrent_mark()` check is removed.
    #[test]
    fn reclaimers_and_consumers_fail_closed_after_mark_flag_and_slot_overwrite() {
        use super::super::arena_pages::ArenaSweepRanges;
        use super::super::facade_mark::{
            CollectorQuiescence, CollectorQuiescenceError, ConcurrentMarkAdmissionError,
            ConcurrentMarkCapture,
        };
        use super::super::generational::MinorCollectionAdmissionError;
        use super::super::reclamation::ReclamationMode;
        use super::super::scan_contract::SingleMutatorWorld;

        let mut heap = TaggedHeap::new();
        let (_tx, rx) = std::sync::mpsc::channel();
        let heap_id = heap.heap_identity();
        // SAFETY: inert synthetic mark channel recording `heap_id` in ACTIVE_MARK_HEAPS.
        unsafe {
            heap.concurrent_mark_running.set(true);
            heap.gc_exited.install_for_heap(heap_id, rx);
        }
        // Safe 2-line overwrite clearing both TaggedHeap fields:
        heap.concurrent_mark_running = ConcurrentMarkFlag::new();
        heap.gc_exited = MarkExitSlot::empty();
        assert!(!heap.concurrent_mark_running());
        assert!(!heap.gc_exited.is_some());
        assert!(heap.has_unfinished_concurrent_mark());

        // 1. Safe reclaimers / sweepers / adapters:
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.release_concurrent_hash_storage()
            }))
            .is_err(),
            "release_concurrent_hash_storage must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.reclaim_intrusive_objects(ReclamationMode::Explicit)
            }))
            .is_err(),
            "reclaim_intrusive_objects must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.sweep_arena_pages_ranges(ArenaSweepRanges::default())
            }))
            .is_err(),
            "sweep_arena_pages_ranges must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.release_empty_cons_blocks()
            }))
            .is_err(),
            "release_empty_cons_blocks must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.release_empty_object_pages()
            }))
            .is_err(),
            "release_empty_object_pages must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.finish_incremental_sweep()
            }))
            .is_err(),
            "finish_incremental_sweep must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.begin_old_sweep_world_stopped()
            }))
            .is_err(),
            "begin_old_sweep_world_stopped must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.sweep_old_objects_slice(1)
            }))
            .is_err(),
            "sweep_old_objects_slice must fail closed"
        );
        heap.mark_in_progress = true;
        heap.generational.major_in_progress = true;
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                heap.trace_black_born_children_world_stopped()
            }))
            .is_err(),
            "trace_black_born_children_world_stopped must fail closed"
        );

        // 2. check_concurrent_capture (mark_in_progress == true, sweep_in_progress == false):
        assert!(matches!(
            // SAFETY: inert serial fixture probing check_concurrent_capture refusal.
            unsafe { ConcurrentMarkCapture::resume_for_test(&mut heap) },
            Err(ConcurrentMarkAdmissionError::NotStarted)
        ));
        heap.mark_in_progress = false;
        heap.generational.major_in_progress = false;

        // 3. SATB & stopped-world scan_contract / scan_contract_bridge consumers:
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // SAFETY: serial fixture probing SATB fold refusal.
                let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
                let _ = world.fold_satb();
            }))
            .is_err(),
            "fold_satb_counts must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
                let _ = world.begin_satb_cycle();
            }))
            .is_err(),
            "begin_satb_cycle must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
                let _ = world.take_satb_written_owners();
            }))
            .is_err(),
            "take_satb_written_owners must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
                let _ = world.with_stopped_heap(|_, _| ());
            }))
            .is_err(),
            "with_stopped_heap must fail closed"
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
                let _ = world.with_stopped_young_logs(|_, _, _| ());
            }))
            .is_err(),
            "with_stopped_young_logs must fail closed"
        );

        // 4. Admission consumers (CollectorQuiescence, minor collection, drain_to_quiescent,
        //    cons_storage attach, owner_era_reserve grant):
        assert!(matches!(
            CollectorQuiescence::check(&mut heap),
            Err(CollectorQuiescenceError::CollectorBusy)
        ));
        assert!(matches!(
            // SAFETY: serial fixture probing minor admission refusal.
            unsafe { heap.begin_minor_collection() },
            Err(MinorCollectionAdmissionError::CollectorBusy)
        ));
        assert!(matches!(
            // SAFETY: serial fixture probing drain_to_quiescent refusal.
            unsafe { heap.drain_to_quiescent(|_| {}) },
            Err(CollectorQuiescenceError::Marker(
                MarkFinishError::PoisonedCollectorState
            ))
        ));
        let cons_storage = super::super::cons_storage::ConsCatalog::new(heap_id);
        assert!(matches!(
            // SAFETY: serial fixture probing ConsCatalog::install refusal.
            unsafe { cons_storage.install(&mut heap) },
            Err(super::super::RegistrationError::Busy)
        ));
        assert!(!heap.rows_observation_idle());
    }
}
