//! Sealed stopped-world proof token for collector-protocol entry points
//! (value-gc C12, G1).
//!
//! Collector-protocol entry points (`collect`, `collect_exact`,
//! `begin_stw_collection`, `complete_collection`, `begin_minor_collection`,
//! `complete_minor_collection`, `incremental_finish`,
//! `incremental_sweep_slice`, `finish_incremental_sweep_now`) and their
//! internal collector-phase helpers (`finalize_collection`,
//! `begin_collection_with`, `begin_parallel_minor_collection`,
//! `finish_incremental_sweep`, `begin_old_sweep_world_stopped`,
//! `sweep_old_objects_slice`) take `&StoppedWorldProof<'_>`.
//!
//! The proof's authority is a borrow held for the whole closure, never a raw
//! pointer:
//! - [`StoppedWorldProof::with_stopped_world`] (serial owner) borrows the heap
//!   `&mut` and hands that same borrow to `body`, so no `&`/`&mut` pair to the
//!   heap coexists. Entry refuses an unfinished concurrent mark (C2's sealed
//!   side-table, not the bare flag), owner cons reserves and young log loans.
//! - [`StoppedWorldProof::with_all_stopped`] (parallel) additionally borrows
//!   `&'stop ParallelStoppedMutatorsProof<'stop>`, the proof `AllStopped::trace`
//!   hands its callback; the token's `'stop` is that borrow. The parallel proof
//!   is minted only from a `TraceWitness`, which only `trace` creates after its
//!   validation, so the safe parallel mint is reachable only inside `trace`.
//! - Both refuse inside a `with_*_mut` mutation closure and suspend the TLS
//!   heap slot for the body (`StoppedWorldEntry`).
//!
//! Fields are private to this leaf module, the token is invariant in `'stop`
//! and `!Send + !Sync + !Clone + !Copy`, and every consumer calls
//! [`StoppedWorldProof::assert_stopped`] first: identity, no unfinished
//! concurrent mark (side-table), and no marker launched since the mint
//! (launch epoch). The AST guard
//! `c12_collector_protocol_entries_require_stopped_world_proof` denies any
//! other constructor and allowlists every production mint site.

use super::scan_contract::ParallelStoppedMutatorsProof;
use super::{HeapIdentity, StoppedWorldEntry, TaggedHeap};
use std::marker::PhantomData;
use std::rc::Rc;

/// Thread-confined, non-escaping, lifetime-invariant proof that all mutators
/// of the identified [`TaggedHeap`] are stopped (no concurrent or TLS-aliased
/// writer, no unfinished concurrent marker, none launched since the mint) for
/// the lifetime `'stop`.
#[derive(Debug)]
#[doc(hidden)]
pub struct StoppedWorldProof<'stop> {
    heap_identity: HeapIdentity,
    /// The heap's `mark_launch_epoch` at mint: a marker launched (and perhaps
    /// already finished) inside `body` fails the next `assert_stopped`.
    mark_launch_epoch: u64,
    /// `cfg(test)` only: minted by `with_unchecked_admission_for_test`, so
    /// `assert_stopped` checks identity alone and rows reach each entry's own
    /// refusal.
    #[cfg(test)]
    unchecked_for_test: bool,
    // Invariant in `'stop` (two distinct stop lifetimes cannot unify via
    // subtyping) and `!Send + !Sync` via `Rc<()>`.
    _scope: PhantomData<(fn(&'stop ()) -> &'stop (), Rc<()>)>,
}

static_assertions::assert_not_impl_any!(StoppedWorldProof<'static>: Send, Sync, Clone, Copy);

impl<'stop> StoppedWorldProof<'stop> {
    /// Serial-owner mint: run `body` with the same `&mut` heap borrow and a
    /// non-escaping proof bound to its [`HeapIdentity`] and launch epoch.
    ///
    /// Release-asserts, through `heap` itself: no `with_*_mut` mutation
    /// closure is live; no concurrent mark is unfinished
    /// (`has_unfinished_concurrent_mark`: flag, exit channel or the sealed
    /// `mark_channel` side-table); no owner cons reserve is outstanding; no
    /// young log is on loan to a parallel owner. For the duration of `body`
    /// the thread's TLS heap slot is null when it named `heap`, so a
    /// `with_tagged_heap` re-entry inside `body` cannot alias it.
    ///
    /// # Safety
    /// `&mut TaggedHeap` excludes every safe Rust alias, and the TLS slot is
    /// suspended above; the caller still guarantees that, for all of `body`:
    /// - no other raw-pointer writer of this heap runs (a `*mut TaggedHeap`
    ///   obtained before the mint);
    /// - no Lisp callback, allocating safepoint or mutator resumption runs.
    #[inline]
    pub unsafe fn with_stopped_world<R>(
        heap: &mut TaggedHeap,
        body: impl for<'s> FnOnce(&mut TaggedHeap, &StoppedWorldProof<'s>) -> R,
    ) -> R {
        let _entry = StoppedWorldEntry::enter(heap);
        assert!(
            !heap.has_unfinished_concurrent_mark(),
            "StoppedWorldProof::with_stopped_world: a concurrent mark is unfinished"
        );
        assert!(
            !heap.has_owner_cons_reserves(),
            "StoppedWorldProof::with_stopped_world: owner cons reserves are outstanding"
        );
        assert!(
            !heap._satb_layout_reserve.has_outstanding_young_loans(),
            "StoppedWorldProof::with_stopped_world: young logs are on loan"
        );
        let proof = StoppedWorldProof {
            heap_identity: heap.heap_identity(),
            mark_launch_epoch: heap.mark_launch_epoch(),
            #[cfg(test)]
            unchecked_for_test: false,
            _scope: PhantomData,
        };
        body(heap, &proof)
    }

    /// Parallel mint: the stop authority is the borrowed
    /// [`ParallelStoppedMutatorsProof`]. Peers may legitimately hold parked
    /// reserves and loans here; the mutation-closure, TLS, identity and
    /// unfinished-mark checks still apply.
    #[inline]
    pub(super) fn with_all_stopped<R>(
        heap: &mut TaggedHeap,
        parallel: &'stop ParallelStoppedMutatorsProof<'stop>,
        body: impl for<'a> FnOnce(&mut TaggedHeap, &'a StoppedWorldProof<'stop>) -> R,
    ) -> R {
        let _entry = StoppedWorldEntry::enter(heap);
        assert_eq!(
            parallel.heap_identity(),
            heap.heap_identity(),
            "StoppedWorldProof::with_all_stopped: parallel proof belongs to a different TaggedHeap"
        );
        assert!(
            !heap.has_unfinished_concurrent_mark(),
            "StoppedWorldProof::with_all_stopped: a concurrent mark is unfinished"
        );
        let proof = StoppedWorldProof {
            heap_identity: heap.heap_identity(),
            mark_launch_epoch: heap.mark_launch_epoch(),
            #[cfg(test)]
            unchecked_for_test: false,
            _scope: PhantomData,
        };
        body(heap, &proof)
    }

    /// Test-only mint WITHOUT the entry refusals, so rows can reach each
    /// collector entry's own fail-closed check behind the mint check.
    /// `cfg(test)` only: absent from production and from `test-support`.
    ///
    /// # Safety
    /// Same contract as [`Self::with_stopped_world`].
    #[cfg(test)]
    pub(super) unsafe fn with_unchecked_admission_for_test<R>(
        heap: &mut TaggedHeap,
        body: impl for<'s> FnOnce(&mut TaggedHeap, &StoppedWorldProof<'s>) -> R,
    ) -> R {
        let proof = StoppedWorldProof {
            heap_identity: heap.heap_identity(),
            mark_launch_epoch: heap.mark_launch_epoch(),
            unchecked_for_test: true,
            _scope: PhantomData,
        };
        body(heap, &proof)
    }

    /// The heap identity this proof authorizes.
    #[inline]
    pub fn heap_identity(&self) -> HeapIdentity {
        self.heap_identity
    }

    /// First statement of every collector-protocol consumer. Fails closed in
    /// debug and release if the proof was minted for another heap, if a
    /// concurrent mark is unfinished now (the same side-table predicate as the
    /// mint), or if a marker was launched since the mint even when it has
    /// already finished (`mark_launch_epoch`, bumped at the single production
    /// launch). Cost: one mutex-guarded side-table lookup and one `u64`
    /// compare per collector-entry call, off the allocation path.
    #[inline]
    pub(super) fn assert_stopped(&self, heap: &TaggedHeap, entry: &'static str) {
        assert!(
            self.heap_identity == heap.heap_identity(),
            "StoppedWorldProof belongs to a different TaggedHeap ({entry})"
        );
        #[cfg(test)]
        if self.unchecked_for_test {
            return;
        }
        assert!(
            !heap.has_unfinished_concurrent_mark(),
            "StoppedWorldProof used while a concurrent mark is unfinished ({entry})"
        );
        assert!(
            self.mark_launch_epoch == heap.mark_launch_epoch(),
            "StoppedWorldProof used after a concurrent marker launched since its mint ({entry})"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagged::value::TaggedValue;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    type Entry = fn(&mut TaggedHeap, &StoppedWorldProof<'_>);

    /// Every gated entry, called with arguments that are otherwise valid on
    /// a fresh heap. Each row's message names its own entry, so deleting one
    /// entry's `assert_stopped` changes the message (a delegate's name or
    /// none) and fails that row.
    fn gated_entries() -> [(&'static str, Entry); 16] {
        [
            // SAFETY (all `unsafe` calls here): serial fixture; the caller
            // owns the only heap view, with no TLS writer or callback.
            ("collect", |h, s| unsafe {
                h.collect(s, std::iter::empty())
            }),
            ("collect_exact", |h, s| unsafe {
                h.collect_exact(s, std::iter::empty())
            }),
            ("begin_stw_collection", |h, s| unsafe {
                h.begin_stw_collection(s)
            }),
            ("complete_collection", |h, s| unsafe {
                h.complete_collection(s)
            }),
            ("begin_minor_collection", |h, s| {
                let _ = unsafe { h.begin_minor_collection(s) };
            }),
            ("complete_minor_collection", |h, s| unsafe {
                h.complete_minor_collection(s)
            }),
            ("incremental_finish", |h, s| unsafe {
                h.incremental_finish(s, 0, std::time::Instant::now())
            }),
            ("incremental_sweep_slice", |h, s| {
                let _ = unsafe { h.incremental_sweep_slice(s, 1) };
            }),
            ("finish_incremental_sweep_now", |h, s| unsafe {
                h.finish_incremental_sweep_now(s)
            }),
            ("finish_first_partition_cycle", |h, s| unsafe {
                h.finish_first_partition_cycle(s)
            }),
            ("finish_incremental_sweep", |h, s| {
                h.finish_incremental_sweep(s)
            }),
            ("begin_collection_with", |h, s| {
                h.begin_collection_with(s, false)
            }),
            ("finalize_collection", |h, s| {
                h.finalize_collection(s, 0, 0, std::time::Instant::now())
            }),
            ("begin_old_sweep_world_stopped", |h, s| {
                h.begin_old_sweep_world_stopped(s)
            }),
            ("sweep_old_objects_slice", |h, s| {
                let _ = h.sweep_old_objects_slice(s, 1);
            }),
            ("begin_parallel_minor_collection", |h, s| {
                let id = h.heap_identity();
                let parallel = ParallelStoppedMutatorsProof::for_heap_identity_test(id);
                let capture =
                    super::super::cons_allocator::CapturedParallelYoung::for_heap_test(id);
                let _ = h.begin_parallel_minor_collection(s, capture, &parallel);
            }),
        ]
    }

    fn panic_message(f: impl FnOnce()) -> String {
        let payload = catch_unwind(AssertUnwindSafe(f)).expect_err("expected a panic");
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    #[test]
    fn every_gated_entry_rejects_a_foreign_heap_proof_by_name() {
        for (name, entry) in gated_entries() {
            let mut heap_a = TaggedHeap::new();
            let mut heap_b = TaggedHeap::new();
            // SAFETY: serial fixture; both heaps are owned by this test.
            let message = unsafe {
                StoppedWorldProof::with_stopped_world(&mut heap_a, |_, foreign| {
                    panic_message(|| entry(&mut heap_b, foreign))
                })
            };
            assert_eq!(
                message,
                format!("StoppedWorldProof belongs to a different TaggedHeap ({name})")
            );
        }
    }

    #[test]
    fn every_gated_entry_rejects_a_proof_after_a_marker_started_by_name() {
        for (name, entry) in gated_entries() {
            let mut heap = TaggedHeap::new();
            // SAFETY: serial fixture. The flag write stands in for the
            // production launch (`launch_concurrent_mark` sets it) inside
            // `body`; no worker exists, and it is cleared before drop.
            let message = unsafe {
                StoppedWorldProof::with_stopped_world(&mut heap, |heap, stopped| {
                    heap.concurrent_mark_running.set(true);
                    let message = panic_message(|| entry(heap, stopped));
                    heap.concurrent_mark_running.set(false);
                    message
                })
            };
            assert_eq!(
                message,
                format!("StoppedWorldProof used while a concurrent mark is unfinished ({name})")
            );
        }
    }

    /// The v2 aliasing shape: the body mutates the very heap the constructor
    /// admitted. v2 (`*const TaggedHeap`, `&*heap` while `body` held `&mut`)
    /// failed this row under Miri Stacked and Tree Borrows.
    #[test]
    fn with_stopped_world_body_mutates_the_admitted_heap() {
        let mut heap = TaggedHeap::new();
        let identity = heap.heap_identity();
        // SAFETY: serial fixture; the test owns the only heap view.
        unsafe {
            StoppedWorldProof::with_stopped_world(&mut heap, |heap, stopped| {
                assert_eq!(stopped.heap_identity(), identity);
                heap.collect_exact(stopped, std::iter::empty());
                heap.collect_exact(stopped, std::iter::empty());
            });
        }
    }

    /// Leave an unfinished mark recorded ONLY in the sealed `mark_channel`
    /// side-table: install a synthetic exit channel, then apply the C2 safe
    /// overwrites of both the plain flag and the exit slot.
    fn record_mark_only_in_side_table(heap: &mut TaggedHeap) {
        let (_tx, rx) = std::sync::mpsc::channel();
        let id = heap.heap_identity();
        // SAFETY: inert synthetic mark channel; no worker exists. Drop sees
        // the side-table entry and retains storage.
        unsafe {
            heap.concurrent_mark_running.set(true);
            heap.gc_exited.install_for_heap(id, rx);
        }
        heap.concurrent_mark_running = super::super::ConcurrentMarkFlag::new();
        heap.gc_exited = super::super::mark_channel::MarkExitSlot::empty();
        assert!(!heap.concurrent_mark_running() && !heap.gc_exited.is_some());
        assert!(
            heap.has_unfinished_concurrent_mark(),
            "side-table entry must remain"
        );
    }

    #[test]
    fn with_stopped_world_refuses_an_unfinished_mark_recorded_only_in_the_side_table() {
        let mut heap = TaggedHeap::new();
        record_mark_only_in_side_table(&mut heap);
        let message = panic_message(|| unsafe {
            StoppedWorldProof::with_stopped_world(&mut heap, |_, _| {});
        });
        assert_eq!(
            message,
            "StoppedWorldProof::with_stopped_world: a concurrent mark is unfinished"
        );
    }

    #[test]
    fn every_gated_entry_rejects_a_side_table_only_mark_recorded_after_the_mint_by_name() {
        for (name, entry) in gated_entries() {
            let mut heap = TaggedHeap::new();
            // SAFETY: serial fixture; the side-table state is synthetic.
            let message = unsafe {
                StoppedWorldProof::with_stopped_world(&mut heap, |heap, stopped| {
                    record_mark_only_in_side_table(heap);
                    panic_message(|| entry(heap, stopped))
                })
            };
            assert_eq!(
                message,
                format!("StoppedWorldProof used while a concurrent mark is unfinished ({name})")
            );
        }
    }

    #[test]
    fn every_gated_entry_rejects_a_proof_after_a_marker_launched_and_finished_by_name() {
        for (name, entry) in gated_entries() {
            let mut heap = TaggedHeap::new();
            // SAFETY: serial fixture. The epoch bump stands in for a launch
            // (`launch_concurrent_mark` bumps it) that also finished inside
            // `body`, so the flag and side-table are clear again (ABA).
            let message = unsafe {
                StoppedWorldProof::with_stopped_world(&mut heap, |heap, stopped| {
                    heap.mark_launch_epoch += 1;
                    assert!(!heap.has_unfinished_concurrent_mark());
                    panic_message(|| entry(heap, stopped))
                })
            };
            assert_eq!(
                message,
                format!(
                    "StoppedWorldProof used after a concurrent marker launched since its mint ({name})"
                )
            );
        }
    }

    /// #859: `promote_and_blacken` relinks `all_objects` on the premise that
    /// the sweep completed, so the entry refuses a pending mark or sweep even
    /// with a valid proof.
    #[test]
    fn finish_first_partition_cycle_refuses_a_pending_mark_or_sweep() {
        for pending in ["sweep", "mark"] {
            let mut heap = TaggedHeap::new();
            // SAFETY: serial fixture; the flags are restored before drop.
            let message = unsafe {
                StoppedWorldProof::with_stopped_world(&mut heap, |heap, stopped| {
                    match pending {
                        "sweep" => heap.sweep_in_progress = true,
                        _ => heap.mark_in_progress = true,
                    }
                    let message = panic_message(|| heap.finish_first_partition_cycle(stopped));
                    heap.sweep_in_progress = false;
                    heap.mark_in_progress = false;
                    message
                })
            };
            assert_eq!(
                message, "finish_first_partition_cycle: the cycle's mark or sweep has not drained",
                "pending {pending}"
            );
        }
    }

    #[test]
    fn both_mints_refuse_inside_a_heap_mutation_closure() {
        let mut fixture = crate::tagged::gc::TestHeap::new();
        let v = fixture.alloc_vector(vec![TaggedValue::fixnum(7)]);
        let other = TaggedHeap::new();
        let parallel = ParallelStoppedMutatorsProof::for_heap_identity_test(other.heap_identity());
        crate::tagged::mutate::with_vector_data_mut(v, |_| {
            for mint in [0, 1] {
                // SAFETY: the mint must refuse before touching the heap.
                let message = panic_message(|| unsafe {
                    crate::tagged::gc::with_tagged_heap(|heap| match mint {
                        0 => StoppedWorldProof::with_stopped_world(heap, |_, _| {}),
                        _ => StoppedWorldProof::with_all_stopped(heap, &parallel, |_, _| {}),
                    })
                });
                assert_eq!(
                    message,
                    "GC safe point inside a with_*_mut heap mutation closure (GEN-5)"
                );
            }
        })
        .unwrap();
    }

    #[test]
    fn with_stopped_world_suspends_the_tls_heap_slot_for_its_body() {
        let _fixture = crate::tagged::gc::TestHeap::new();
        // SAFETY: the fixture installed itself as this thread's heap; the
        // body only observes the slot.
        unsafe {
            crate::tagged::gc::with_tagged_heap(|heap| {
                assert!(crate::tagged::gc::tagged_heap_is_current(heap));
                StoppedWorldProof::with_stopped_world(heap, |heap, _| {
                    assert!(
                        !crate::tagged::gc::tagged_heap_is_current(heap),
                        "the TLS slot must not name the heap inside the proof body"
                    );
                });
                assert!(
                    crate::tagged::gc::tagged_heap_is_current(heap),
                    "restored after body"
                );
                let _ = panic_message(|| {
                    StoppedWorldProof::with_stopped_world(heap, |_, _| panic!("body unwinds"));
                });
                assert!(
                    crate::tagged::gc::tagged_heap_is_current(heap),
                    "restored on unwind"
                );
            });
        }
    }

    #[test]
    fn with_stopped_world_refuses_outstanding_owner_cons_reserves() {
        let mut heap = TaggedHeap::new();
        heap.cons_allocator.credit_owner_reserved_block();
        let message = panic_message(|| unsafe {
            StoppedWorldProof::with_stopped_world(&mut heap, |_, _| {});
        });
        heap.cons_allocator.return_owner_reserved_blocks(1);
        assert_eq!(
            message,
            "StoppedWorldProof::with_stopped_world: owner cons reserves are outstanding"
        );
    }

    #[test]
    fn with_stopped_world_refuses_young_logs_on_loan() {
        let mut heap = TaggedHeap::new();
        // SAFETY: serial fixture; sole owner, no marker, no callback.
        let loan = {
            let mut world =
                unsafe { super::super::scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
            let id = world.current_young_registration_id();
            world.take_young_log_loan(id).unwrap()
        };
        let message = panic_message(|| unsafe {
            StoppedWorldProof::with_stopped_world(&mut heap, |_, _| {});
        });
        // SAFETY: same confined owner.
        let mut world =
            unsafe { super::super::scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
        world.return_young_log_loan(loan).unwrap();
        assert_eq!(
            message,
            "StoppedWorldProof::with_stopped_world: young logs are on loan"
        );
    }

    #[test]
    fn with_all_stopped_refuses_a_parallel_proof_for_another_heap() {
        let mut heap = TaggedHeap::new();
        let other = TaggedHeap::new();
        let parallel = ParallelStoppedMutatorsProof::for_heap_identity_test(other.heap_identity());
        let message = panic_message(|| {
            StoppedWorldProof::with_all_stopped(&mut heap, &parallel, |_, _| {});
        });
        assert!(
            message.starts_with(
                "assertion `left == right` failed: StoppedWorldProof::with_all_stopped: \
                 parallel proof belongs to a different TaggedHeap"
            ),
            "{message}"
        );
    }

    #[test]
    fn with_all_stopped_refuses_an_unfinished_mark() {
        let mut heap = TaggedHeap::new();
        let parallel = ParallelStoppedMutatorsProof::for_heap_identity_test(heap.heap_identity());
        record_mark_only_in_side_table(&mut heap);
        let message = panic_message(|| {
            StoppedWorldProof::with_all_stopped(&mut heap, &parallel, |_, _| {});
        });
        assert_eq!(
            message,
            "StoppedWorldProof::with_all_stopped: a concurrent mark is unfinished"
        );
    }
}
