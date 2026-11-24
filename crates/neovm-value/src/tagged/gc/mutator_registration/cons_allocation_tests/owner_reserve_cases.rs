//! Cons reserve proofs using the actual stopped World fixtures.

use super::*;
use crate::tagged::gc::CONS_BLOCK_BYTES;
use crate::tagged::gc::alloc_region::owner_era_reserve::tests as reserve_tests;
use crate::tagged::gc::{MeasuredInterval, OwnerAllocRegion, OwnerEraReserve, ReserveError};
use crate::tagged::header::ConsCell;

/// Borrow the current collector designator, restoring and republishing the
/// actual keeper image. Only entry zero is absent during `body`; every stopped
/// peer publication remains intact and no registry/entry lock crosses `body`.
fn with_stopped_legacy_heap<K, R>(
    era: &ParallelHeapEra<'_, K>,
    stopped: &AllStopped<'_>,
    body: impl FnOnce(&mut TaggedHeap) -> R,
) -> R
where
    K: EraKeeper + std::fmt::Debug,
{
    stopped.check_world_roots(era.heap_identity()).unwrap();
    let legacy_entry = {
        let gate = era.core.registry().lock().unwrap();
        Arc::clone(
            gate.entries
                .iter()
                .find(|entry| entry.key.owner == 0)
                .unwrap(),
        )
    };
    let legacy = entry_state(&legacy_entry).unwrap().image.take().unwrap();
    let mut collector = era.core.collector.borrow_mut();
    let heap = collector.as_mut().unwrap().registration_heap_mut();
    legacy.restore_legacy(heap).unwrap();
    let result = body(heap);
    heap.close_alloc_regions();
    let legacy = PublishedGcImage::from_legacy(heap).unwrap();
    let mut state = entry_state(&legacy_entry).unwrap();
    assert!(state.image.replace(legacy).is_none());
    result
}

pub(super) fn owner_cons_reserve_refuses_use_after_heap_drop<K>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
) where
    K: EraKeeper + std::fmt::Debug,
{
    let mut reserve = OwnerEraReserve::new();
    let mut first_heap = None;
    let mut granted = None;
    roots_era(&mut |era, _inputs| {
        first_heap = Some(era.heap_identity());
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        granted = Some(with_stopped_legacy_heap(&era, &stopped, |heap| {
            reserve.grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
        }));
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
    // This call has returned through the core fixture, so A's actual Context
    // has dropped before the second Context/World can be constructed.
    assert!(
        granted.unwrap().is_ok(),
        "the first real heap granted storage"
    );
    assert!(reserve.cons_blocks_held() > 0);
    let mut refused = None;
    roots_era(&mut |era, _inputs| {
        assert_ne!(Some(era.heap_identity()), first_heap);
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        refused = Some(with_stopped_legacy_heap(&era, &stopped, |heap| {
            let mut region = OwnerAllocRegion::new();
            // On the OLD implementation this only installs dangling scalar
            // cursors and charges B. Neither type has a destructor that reads
            // A's storage. Never take, close, refund or return this bad run.
            reserve.carve_cons(heap, &mut region)
        }));
        // Restore B's actual keeper image and finish the epoch before testing
        // the refusal, so even the intended RED follows ordinary teardown.
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
    assert_eq!(
        refused.unwrap(),
        Err(ReserveError::WrongHeap),
        "a reserve cannot install a run after its granting heap has dropped"
    );
    // The granting heap is gone. Drop must not follow its ranges. Debug builds
    // are loud about the nonempty book and leave the lease unreclaimed.
    #[cfg(debug_assertions)]
    {
        let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(reserve)));
        assert!(
            dropped.is_err(),
            "dropping a nonempty book is loud in debug"
        );
    }
    #[cfg(not(debug_assertions))]
    drop(reserve);
}

pub(super) fn owner_cons_reserve_grants_only_cons_storage<K>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
) where
    K: EraKeeper + std::fmt::Debug,
{
    let mut observed = None;
    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        observed = Some(with_stopped_legacy_heap(&era, &stopped, |heap| {
            // gc_object_counts reads the last cold capture. Request and
            // publish through its public interface so both observations
            // describe the actual initialized storage, without running GC.
            let symbols = heap.gc_object_counts().symbols_used;
            heap.request_gc_object_counts();
            heap.publish_gc_object_counts_if_requested(symbols);
            let before = heap.gc_object_counts();
            let mut reserve = OwnerEraReserve::new();
            let granted = reserve.grant_at_stop(&stopped, heap, true, MeasuredInterval::zero());
            heap.request_gc_object_counts();
            heap.publish_gc_object_counts_if_requested(symbols);
            let after = heap.gc_object_counts();
            let returned = reserve.return_unused_before_sweep(&stopped, heap);
            (before, after, granted, returned, reserve.holds_reserve())
        }));
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
    let (before, after, granted, returned, held) = observed.unwrap();
    assert!(granted.is_ok(), "the real stopped heap granted its reserve");
    assert_eq!(returned, Ok(()));
    assert!(!held, "return releases the entire real reserve");
    assert_eq!(
        (
            after.floats_used,
            after.vectors_used,
            after.vector_slots_used
        ),
        (
            before.floats_used,
            before.vectors_used,
            before.vector_slots_used
        ),
        "a cons reserve must not allocate float or bignum storage"
    );
}

pub(super) fn owner_cons_reserve_existing_cons_case<K, W>(
    case: ConsAllocationCase,
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    if case
        == ConsAllocationCase::OwnerConsReserveAnOldStoreKeepsTheOwnerConsAndTheUnrootedConsIsReclaimed
    {
        owner_cons_reserve_old_store(roots_era, world_roots);
        return;
    }
    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            match case {
            ConsAllocationCase::OwnerConsReserveTwoIdleStopsHoldNoReserve => {
                reserve_tests::two_idle_stops_hold_no_reserve(&stopped, heap)
            }
            ConsAllocationCase::OwnerConsReservePacingChargesCarvedCellsAndSubtractsTheOpenTail => {
                reserve_tests::pacing_charges_carved_cells_and_subtracts_the_open_tail(&stopped, heap)
            }
            ConsAllocationCase::OwnerConsReserveTheCapIsAReserveMiss => {
                reserve_tests::the_cap_is_a_reserve_miss(&stopped, heap)
            }
            ConsAllocationCase::OwnerConsReserveTwoOwnersCarveDisjointRunsWithoutMovingTheHeapWindow => {
                reserve_tests::two_owners_carve_disjoint_runs_without_moving_the_heap_window(
                    &stopped, heap,
                )
            }
            ConsAllocationCase::OwnerConsReserveAMovedBlockTailIsNotRewound => {
                reserve_tests::a_moved_block_tail_is_not_rewound(&stopped, heap)
            }
            _ => unreachable!("not a preserved reserve case"),
        }
        });
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn owner_cons_reserve_old_store<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use std::mem::size_of;

    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let old = with_stopped_legacy_heap(&era, &stopped, |heap| {
            assert!(heap.generational.enabled);
            heap.generational.verify = true;
            heap.partition_dump = false;
            heap.dump_blackened = false;
            heap.gc_threshold = usize::MAX / 4;
            let old = heap.alloc_cons(Value::fixnum(1), Value::NIL);
            heap.close_alloc_regions();
            // SAFETY: the current stopped keeper owns this newly initialized
            // cons and registers its root before the next collection.
            unsafe { SharedRoot::new(heap, old) }
        });
        stopped.trace(|_, _| Ok(())).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);

        let mut promoted = false;
        for _ in 0..3 {
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped_minor(&mut stopped, world_roots)
                .unwrap();
            promoted = with_stopped_legacy_heap(&era, &stopped, |heap| {
                let value = old.materialize(heap).unwrap().value();
                heap.cons_age_is_old_for_test(value)
            });
            era.resume_cons_storage(&stopped).unwrap();
            complete(stopped);
            if promoted {
                break;
            }
        }
        assert!(promoted, "the genuine World minors promoted the old cons");

        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let (kept, control, tail_word) = with_stopped_legacy_heap(&era, &stopped, |heap| {
            let mut a = OwnerEraReserve::new();
            let mut b = OwnerEraReserve::new();
            let mut region_a = OwnerAllocRegion::new();
            let mut region_b = OwnerAllocRegion::new();
            let young = heap
                ._satb_layout_reserve
                .current_young_log_mut()
                .unwrap()
                .blocks()
                .len();
            a.grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            b.grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            assert_eq!(
                heap._satb_layout_reserve
                    .current_young_log_mut()
                    .unwrap()
                    .blocks()
                    .len(),
                young + 2
            );
            a.carve_cons(heap, &mut region_a).unwrap();
            b.carve_cons(heap, &mut region_b).unwrap();
            let addr_a = region_a.take_cons_cell().unwrap();
            let addr_b = region_b.take_cons_cell().unwrap();
            let kept_value = reserve_tests::handed_cons(addr_a);
            let control_value = reserve_tests::handed_cons(addr_b);
            a.note_cons_taken(1);
            b.note_cons_taken(1);
            assert!(addr_a.abs_diff(addr_b) >= CONS_BLOCK_BYTES);
            // SAFETY: this is an aligned cell inside A's live block. Only
            // the ownership predicate inspects its tag/address, never payload.
            let tail = unsafe {
                Value::from_cons_ptr((addr_a + size_of::<ConsCell>()) as *const ConsCell)
            };
            assert_eq!(
                a.return_unused_before_sweep(&stopped, heap),
                Err(ReserveError::RegionOpen)
            );
            assert_eq!(
                b.return_unused_before_sweep(&stopped, heap),
                Err(ReserveError::RegionOpen)
            );
            assert!(heap.owns_heap_value_for_test(kept_value));
            assert!(heap.owns_heap_value_for_test(control_value));
            a.close_for_stop(heap, &mut region_a).unwrap();
            b.close_for_stop(heap, &mut region_b).unwrap();
            a.return_unused_before_sweep(&stopped, heap).unwrap();
            b.return_unused_before_sweep(&stopped, heap).unwrap();
            assert!(!a.holds_reserve());
            assert!(!b.holds_reserve());
            assert!(region_a.is_closed());
            assert!(region_b.is_closed());
            assert!(heap.owns_heap_value_for_test(kept_value));
            assert!(heap.owns_heap_value_for_test(control_value));
            assert!(!heap.owns_heap_value_for_test(tail));
            assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
            assert_eq!(heap.jit_float_window_for_test(), (0, 0));
            // SAFETY: both cells are initialized and live on this keeper's
            // heap. Temporary roots cover resume and the owner's store.
            unsafe {
                (
                    SharedRoot::new(heap, kept_value),
                    SharedRoot::new(heap, control_value),
                    tail.bits(),
                )
            }
        });
        stopped.trace(|_, _| Ok(())).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);

        let kept_word = kept.parallel_word();
        let control_word = control.parallel_word();
        let mut owner = activate(era.register(1927, 2).unwrap(), era.heap_identity(), 1927);
        with_run(&mut owner, |run| {
            let old_home = run.root_slot(&old).unwrap();
            let kept_home = run.root_slot(&kept).unwrap();
            run.with_slice(|slice| {
                let old = slice.bind(&old_home).unwrap();
                let mut kept = slice.bind(&kept_home).unwrap();
                slice.store_car(&old, &kept).unwrap();
                // The next minor must reach kept only through the old card.
                slice.store_immediate(&mut kept, Value::NIL.bits()).unwrap();
            })
            .unwrap();
        });
        drop(kept);
        drop(control);
        owner.enter_native(std::slice::from_ref(&old)).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            let old_value = old.materialize(heap).unwrap().value();
            assert!(heap.owner_card_is_dirty_for_test(old_value));
        });
        era.collect_stopped_minor(&mut stopped, world_roots)
            .unwrap();
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            // SAFETY: these words identify cells of this same heap and are
            // used first as ownership probes. Only the retained kept cell is
            // read through old's initialized, traced field after validation.
            let kept = unsafe { Value::from_bits(kept_word) };
            let control = unsafe { Value::from_bits(control_word) };
            let tail = unsafe { Value::from_bits(tail_word) };
            assert!(heap.owns_heap_value_for_test(kept));
            assert!(!heap.owns_heap_value_for_test(control));
            assert!(!heap.owns_heap_value_for_test(tail));
            let old_value = old.materialize(heap).unwrap().value();
            assert_eq!(old_value.cons_car(), kept);
            assert_eq!(heap.jit_cons_window_for_test(), (0, 0));
            assert_eq!(heap.jit_float_window_for_test(), (0, 0));
        });
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        // The two persistent execution homes outlive the slice that created
        // them. Their handles are gone, so close that root frame explicitly
        // before retirement publishes its final image.
        with_run(&mut owner, |run| run.clear_root_slots().unwrap());
        retire(owner);
        drop(old);
        // The final minor resumed the catalog so the owner could consume
        // its response and retire. Return that live catalog at one last real
        // stop before asking finish to restore the serial heap.
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped, world_roots).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// A fully consumed block stays reserved until the stopped session returns it.
/// The session returns the closed book and then collects. An ordinary
/// collector entry still refuses a live lease at `assert_no_owner_cons_reserves`.
pub(super) fn owner_cons_reserve_session_returns_closed_book_then_collects<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let mut reserve = OwnerEraReserve::new();
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            reserve
                .grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            let mut region = OwnerAllocRegion::new();
            loop {
                match reserve.carve_cons(heap, &mut region) {
                    Ok(()) => {}
                    Err(ReserveError::ReserveMiss) => break,
                    Err(error) => panic!("carve granted block: {error:?}"),
                }
                let mut handed = 0;
                while let Some(cell) = region.take_cons_cell() {
                    reserve_tests::handed_cons(cell);
                    handed += 1;
                }
                reserve.note_cons_taken(handed);
                reserve.close_for_stop(heap, &mut region).unwrap();
            }
            assert!(region.is_closed());
        });
        assert!(
            reserve.holds_reserve(),
            "exhaustion does not return its whole block"
        );
        assert!(
            matches!(
                era.resume_cons_storage(&stopped),
                Err(RegistrationError::Busy)
            ),
            "catalog refusal must preserve the stop for explicit return and GC retry"
        );
        let world_calls = std::cell::Cell::new(0usize);
        let result = era.collect_stopped(&mut stopped, |keeper, guard, roots| {
            world_calls.set(world_calls.get() + 1);
            world_roots(keeper, guard, roots)
        });
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        assert!(
            result.is_ok(),
            "the session returns the reserved block and then collects"
        );
        assert!(!reserve.holds_reserve());
        assert_eq!(
            world_calls.get(),
            1,
            "the session returns the book before the World callback, then collects once"
        );
    });
}

pub(super) fn owner_cons_reserve_refuses_foreign_book_without_mutation<K>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
) where
    K: EraKeeper + std::fmt::Debug,
{
    use crate::tagged::header::{ConsCdrWord, ConsWord};
    use std::mem::size_of;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let (refused, before, after) = with_stopped_legacy_heap(&era, &stopped, |heap| {
            let mut a = OwnerEraReserve::new();
            let mut b = OwnerEraReserve::new();
            let mut region_a = OwnerAllocRegion::new();
            let mut region_b = OwnerAllocRegion::new();
            a.grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            b.grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            a.carve_cons(heap, &mut region_a).unwrap();
            b.carve_cons(heap, &mut region_b).unwrap();
            for (book, region) in [(&mut a, &mut region_a), (&mut b, &mut region_b)] {
                let first = region.take_cons_cell().unwrap();
                book.note_cons_taken(1);
                let cells = 1 + book.open_cons_tail_bytes() / size_of::<ConsCell>();
                // SAFETY: `first` came from this actual exclusive carved run.
                // The book reports its exact remaining tail. Initialize the
                // COMPLETE run without reading stale bytes or advancing those
                // tail cursors. Even the old partial-close cleanup then keeps
                // only initialized cons prefixes. No bit/accounting is changed.
                unsafe {
                    for index in 0..cells {
                        (first as *mut ConsCell).add(index).write(ConsCell {
                            car: ConsWord::new(Value::NIL),
                            cdr_or_next: ConsCdrWord::new(Value::NIL),
                        });
                    }
                }
            }
            let before = (
                region_a.is_closed(),
                region_b.is_closed(),
                a.open_cons_tail_bytes(),
                b.open_cons_tail_bytes(),
                a.cons_blocks_held(),
                b.cons_blocks_held(),
            );
            // Catch inside the restored keeper-image scope so even the old
            // mutation-before-debug-assert path is republished and torn down.
            let outcome = catch_unwind(AssertUnwindSafe(|| b.close_for_stop(heap, &mut region_a)));
            let refused = matches!(outcome, Ok(Err(ReserveError::ForeignBook)));
            let after = (
                region_a.is_closed(),
                region_b.is_closed(),
                a.open_cons_tail_bytes(),
                b.open_cons_tail_bytes(),
                a.cons_blocks_held(),
                b.cons_blocks_held(),
            );
            // On old code A's region is already closed and B lost its open
            // record. Each old close still completes safely; every prefix it
            // retains was initialized above. With refusal, both normal own
            // closes refund their unhanded tails instead.
            a.close_for_stop(heap, &mut region_a).unwrap();
            b.close_for_stop(heap, &mut region_b).unwrap();
            a.return_unused_before_sweep(&stopped, heap).unwrap();
            b.return_unused_before_sweep(&stopped, heap).unwrap();
            (refused, before, after)
        });
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
        assert!(refused, "a reserve must refuse a different book's run");
        assert_eq!(
            after, before,
            "refusal must preserve BOTH books and regions"
        );
    });
}

pub(super) fn owner_cons_reserve_refuses_every_foreign_heap_operation<K>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    nested_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
) where
    K: EraKeeper + std::fmt::Debug,
{
    roots_era(&mut |outer, _inputs| {
        outer.start_cons_storage().unwrap();
        let mut outer_stop = outer.stop().unwrap();
        outer.return_cons_storage(&outer_stop).unwrap();
        let mut book = OwnerEraReserve::new();
        with_stopped_legacy_heap(&outer, &outer_stop, |heap| {
            book.grant_at_stop(&outer_stop, heap, true, MeasuredInterval::zero())
                .unwrap();
        });
        let mut observed = None;
        nested_era(&mut |inner, _inputs| {
            assert_ne!(inner.heap_identity(), outer.heap_identity());
            inner.start_cons_storage().unwrap();
            let mut inner_stop = inner.stop().unwrap();
            inner.return_cons_storage(&inner_stop).unwrap();
            observed = Some(with_stopped_legacy_heap(&inner, &inner_stop, |heap| {
                let mut region = OwnerAllocRegion::new();
                let bytes = heap.bytes_since_gc();
                let grant = book.grant_at_stop(&inner_stop, heap, true, MeasuredInterval::zero());
                let returned = book.return_unused_before_sweep(&inner_stop, heap);
                let closed = book.close_for_stop(heap, &mut region);
                // Carve last: under the missing-binding negative control this
                // installs scalar cursors only. Never take/dereference them.
                let carved = book.carve_cons(heap, &mut region);
                (
                    matches!(grant, Err(ReserveError::WrongHeap)),
                    matches!(returned, Err(ReserveError::WrongHeap)),
                    matches!(closed, Err(ReserveError::WrongHeap)),
                    matches!(carved, Err(ReserveError::WrongHeap)),
                    bytes == heap.bytes_since_gc()
                        && region.is_closed()
                        && book.open_cons_tail_bytes() == 0
                        && book.cons_blocks_held() == 1,
                )
            }));
            inner_stop.trace(|_, _| Ok(())).unwrap();
            complete(inner_stop);
            let (collector, keeper) = inner.finish().unwrap();
            drop(collector);
            drop(keeper);
        });
        let observed = observed.unwrap();
        if observed == (true, true, true, true, true) {
            with_stopped_legacy_heap(&outer, &outer_stop, |heap| {
                book.return_unused_before_sweep(&outer_stop, heap).unwrap();
            });
        }
        // A failed negative control must not refund or return a corrupted
        // book. Its outstanding block keeps backing alive through heap Drop.
        outer_stop.trace(|_, _| Ok(())).unwrap();
        complete(outer_stop);
        let (collector, keeper) = outer.finish().unwrap();
        drop(collector);
        drop(keeper);
        assert_eq!(
            observed,
            (true, true, true, true, true),
            "every foreign-heap operation refuses before changing either heap/book"
        );
    });
}

/// Abandoning a nonempty book leaves the lease outstanding. The next
/// `collect_stopped` returns that lease and then collects. An ordinary
/// collector entry still hits `assert_no_owner_cons_reserves`. Dropping the
/// book is loud in debug and does not itself discharge the count.
/// `PanicWhileHolding` is a same-thread `catch_unwind` stand-in for an
/// owner thread that panics.
pub(super) fn owner_cons_reserve_drop_and_forget_keep_collection_excluded<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    #[derive(Clone, Copy, Debug)]
    enum Abandon {
        DropActive,
        DropClosed,
        ForgetActive,
        ForgetClosed,
        PanicWhileHolding,
    }
    for abandon in [
        Abandon::DropActive,
        Abandon::DropClosed,
        Abandon::ForgetActive,
        Abandon::ForgetClosed,
        Abandon::PanicWhileHolding,
    ] {
        roots_era(&mut |era, _inputs| {
            era.start_cons_storage().unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            let mut book = OwnerEraReserve::new();
            with_stopped_legacy_heap(&era, &stopped, |heap| {
                book.grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                    .unwrap();
            });
            match abandon {
                Abandon::DropActive => {
                    let dropped =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(book)));
                    #[cfg(debug_assertions)]
                    assert!(
                        dropped.is_err(),
                        "{abandon:?}: dropping a nonempty book is loud"
                    );
                    #[cfg(not(debug_assertions))]
                    assert!(
                        dropped.is_ok(),
                        "{abandon:?}: release drop leaves the lease"
                    );
                }
                Abandon::DropClosed => {
                    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        drop(book.into_closed().unwrap())
                    }));
                    #[cfg(debug_assertions)]
                    assert!(
                        dropped.is_err(),
                        "{abandon:?}: dropping a nonempty book is loud"
                    );
                    #[cfg(not(debug_assertions))]
                    assert!(
                        dropped.is_ok(),
                        "{abandon:?}: release drop leaves the lease"
                    );
                }
                Abandon::ForgetActive => std::mem::forget(book),
                Abandon::ForgetClosed => std::mem::forget(book.into_closed().unwrap()),
                Abandon::PanicWhileHolding => {
                    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let _hold = book;
                        panic!("owner panicked while holding a cons reserve");
                    }));
                    assert!(
                        panicked.is_err(),
                        "{abandon:?}: the owner panic must surface"
                    );
                }
            }
            assert!(
                matches!(
                    era.resume_cons_storage(&stopped),
                    Err(RegistrationError::Busy)
                ),
                "catalog admission must not hide the obligation in its detached allocator"
            );
            assert!(
                matches!(era.shutdown_admission_stopped_for_test(&stopped).unwrap(),
                Err(crate::tagged::gc::MarkFinishError::OwnerReserveHeld(heap)) if heap == era.heap_identity())
            );
            let world_calls = std::cell::Cell::new(0);
            assert!(
                era.collect_stopped(&mut stopped, |keeper, guard, roots| {
                    world_calls.set(world_calls.get() + 1);
                    world_roots(keeper, guard, roots)
                })
                .is_ok(),
                "{abandon:?}: the next collection returns the abandoned lease and then collects"
            );
            assert_eq!(world_calls.get(), 1);
            with_stopped_legacy_heap(&era, &stopped, |heap| {
                assert!(
                    !heap.has_owner_cons_reserves(),
                    "{abandon:?}: the lease table discharged the abandoned book"
                );
            });
            complete(stopped);
            assert!(
                era.start_cons_storage().is_ok(),
                "{abandon:?}: admission proceeds once the lease is back"
            );
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            complete(stopped);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        });
    }
}

/// L6.5 session row. An admitted era holds a cons reserve and
/// `collect_stopped` runs. The session returns that book and then collects.
/// It does not reach `assert_no_owner_cons_reserves`. An ordinary
/// `TaggedHeap::collect` while a lease is live is
/// [`owner_cons_reserve_ordinary_gc_refuses_a_live_lease`].
pub(super) fn owner_cons_reserve_main_gc_returns_the_book_then_collects<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let mut reserve = OwnerEraReserve::new();
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            reserve
                .grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
        });
        assert!(reserve.holds_reserve());
        assert!(
            era.collect_stopped(&mut stopped, world_roots).is_ok(),
            "the session returns the outstanding book and then collects"
        );
        assert!(
            !reserve.holds_reserve(),
            "the keeper return cleared the live book"
        );
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            assert!(
                !heap.has_owner_cons_reserves(),
                "the ordinary entry left no owner cons reserve"
            );
        });
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        String::from("non-string panic")
    }
}

/// Ordinary `TaggedHeap::collect` while a lease still has an open run.
///
/// The request must refuse at `assert_no_owner_cons_reserves` and leave the
/// book returnable. It must not rewind the block under the open run. Routing
/// that request through the stop session is still owed, so production grants
/// stay gated on it. This row is red on 9c6133fae3: the infallible entry
/// returns the lease, `close_for_stop` sticks on `InvalidReserve`, and the
/// owner cursor and the free list hand out the same cell.
pub(super) fn owner_cons_reserve_ordinary_gc_refuses_a_live_lease<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    _world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::header::{ConsCdrWord, ConsWord};
    use std::mem::size_of;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let refused = with_stopped_legacy_heap(&era, &stopped, |heap| {
            let mut reserve = OwnerEraReserve::new();
            let mut region = OwnerAllocRegion::new();
            reserve
                .grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            reserve.carve_cons(heap, &mut region).unwrap();
            let handed_addr = region.take_cons_cell().unwrap();
            let handed = reserve_tests::handed_cons(handed_addr);
            reserve.note_cons_taken(1);
            let tail_bytes = reserve.open_cons_tail_bytes();
            assert!(tail_bytes >= size_of::<ConsCell>(), "the run has a tail");
            let tail_addr = handed_addr + size_of::<ConsCell>();
            let tail_cells = tail_bytes / size_of::<ConsCell>();
            // SAFETY: the tail is this run's exclusive unhanded cells. Sweep
            // writes a free link into each unmarked cell.
            unsafe {
                for index in 0..tail_cells {
                    (tail_addr as *mut ConsCell).add(index).write(ConsCell {
                        car: ConsWord::new(Value::NIL),
                        cdr_or_next: ConsCdrWord::new(Value::NIL),
                    });
                }
            }
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                heap.collect(std::iter::once(handed));
            }));
            match outcome {
                Err(payload) => {
                    // `&payload` is the box. Downcast sees that wrapper, not
                    // the `&str` the assert actually panicked with.
                    let text = panic_text(&*payload);
                    assert!(
                        text.contains(
                            "collection requires explicit return of every owner cons reserve"
                        ),
                        "ordinary GC refused for another reason: {text}"
                    );
                    assert!(
                        reserve.holds_reserve(),
                        "a refusal must leave the lease in the table"
                    );
                    assert!(
                        region.take_cons_cell() == Some(tail_addr),
                        "a refusal must leave the owner cursor on its tail"
                    );
                    // The probe handed one more cell. Account it, then close.
                    reserve.note_cons_taken(1);
                    reserve.close_for_stop(heap, &mut region).unwrap();
                    reserve.return_unused_before_sweep(&stopped, heap).unwrap();
                    true
                }
                Ok(()) => {
                    let close = reserve.close_for_stop(heap, &mut region);
                    let next = if region.is_closed() {
                        None
                    } else {
                        region.take_cons_cell()
                    };
                    let fresh = heap.alloc_cons(Value::NIL, Value::NIL);
                    let fresh_addr = fresh.as_cons_ptr().map(|cell| cell as usize);
                    panic!(
                        "ordinary GC reclaimed a live open lease: close={close:?} next={next:#x?} fresh={fresh_addr:#x?} tail={tail_addr:#x} holds={}",
                        reserve.holds_reserve()
                    );
                }
            }
        });
        assert!(
            refused,
            "ordinary GC with a live lease must refuse before reclaiming it"
        );
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// A poisoned cons allocator never clears. The session must report that as
/// `Publication`, the non-transient class, so a Busy retry cannot spin.
pub(super) fn owner_cons_reserve_poisoned_allocator_is_publication<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        let mut reserve = OwnerEraReserve::new();
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            reserve
                .grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            heap.cons_allocator.poison_storage_for_test();
        });
        let result = era.collect_stopped(&mut stopped, world_roots);
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            heap.cons_allocator.clear_storage_poison_for_test();
            reserve.return_unused_before_sweep(&stopped, heap).unwrap();
        });
        assert_eq!(
            result.err(),
            Some(RendezvousError::Publication),
            "a poisoned allocator is permanent and must not be retried as Busy"
        );
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// A reserved-block count with an empty lease table is what a mid-return
/// unwind leaves behind. The count stands in for that wedge: the return
/// took the table and did not discharge every block. Two stopped
/// collections both report `Publication`, because the next walk still
/// sees an empty table and the count does not clear.
pub(super) fn owner_cons_reserve_leftover_count_is_publication<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            assert!(
                !heap.has_owner_cons_reserves(),
                "a fresh stop holds no owner cons count"
            );
            // No lease is inserted. The count alone is the wedge's leftover.
            heap.cons_allocator.credit_owner_reserved_block();
            assert!(
                heap.has_owner_cons_reserves(),
                "the leftover count is visible with an empty lease table"
            );
        });
        assert_eq!(
            era.collect_stopped(&mut stopped, world_roots).err(),
            Some(RendezvousError::Publication),
            "a leftover count is permanent and must not be retried as Busy"
        );
        assert_eq!(
            era.collect_stopped(&mut stopped, world_roots).err(),
            Some(RendezvousError::Publication),
            "a second stop still sees the leftover count"
        );
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            heap.cons_allocator.return_owner_reserved_blocks(1);
            assert!(!heap.has_owner_cons_reserves());
        });
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

/// The session, not an explicit return, gives a partly carved block back.
/// The rooted handed cell stays, the unhanded tail is rewound, and the next
/// allocation is a different cell.
pub(super) fn owner_cons_reserve_session_return_keeps_handed_cell_and_reclaims_tail<K, W>(
    roots_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ParallelHeapEra<'e, K>, RootsFixture)),
    world_roots: W,
) where
    K: EraKeeper + std::fmt::Debug,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut AllStopped<'e>,
            &'o mut Vec<Value>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            RendezvousError,
        >,
{
    use crate::tagged::transport::SharedRoot;

    roots_era(&mut |era, _inputs| {
        era.start_cons_storage().unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        // The book must stay alive until the session empties the lease.
        // Dropping it at the end of the heap closure panics in debug while
        // the range is still outstanding.
        let mut reserve = OwnerEraReserve::new();
        let (root, block_index, handed_addr) = with_stopped_legacy_heap(&era, &stopped, |heap| {
            let mut region = OwnerAllocRegion::new();
            reserve
                .grant_at_stop(&stopped, heap, true, MeasuredInterval::zero())
                .unwrap();
            reserve.carve_cons(heap, &mut region).unwrap();
            let handed_addr = region.take_cons_cell().unwrap();
            let handed = reserve_tests::handed_cons(handed_addr);
            // SAFETY: `handed_addr` is the cell just taken from this run.
            unsafe {
                (*(handed_addr as *mut ConsCell)).set_car(Value::fixnum(1701));
            }
            reserve.note_cons_taken(1);
            assert!(reserve.open_cons_tail_bytes() > 0, "a tail remains");
            let block_index = reserve.first_leased_block_for_test();
            reserve.close_for_stop(heap, &mut region).unwrap();
            // The session returns the lease. This row does not.
            // SAFETY: this cell was just taken from this run and written
            // with car 1701. The keeper of this stop registers it before
            // the session collects.
            let root = unsafe { SharedRoot::new(heap, handed) };
            (root, block_index, handed_addr)
        });
        assert!(
            reserve.holds_reserve(),
            "close leaves the lease for the session"
        );
        assert!(
            era.collect_stopped(&mut stopped, world_roots).is_ok(),
            "the session returns the closed lease and then collects"
        );
        assert!(
            !reserve.holds_reserve(),
            "the session emptied the lease before this book drops"
        );
        with_stopped_legacy_heap(&era, &stopped, |heap| {
            let value = root.materialize(heap).unwrap().value();
            assert_eq!(value.cons_car(), Value::fixnum(1701));
            let block = &heap.cons_blocks[block_index];
            assert_eq!(
                block.next_index, 1,
                "the unhanded tail was rewound to the handed cell"
            );
            let cell = std::mem::size_of::<ConsCell>();
            let tail_addr = handed_addr + cell;
            // Closing the region ends this block's bump, so the next
            // alloc_cons comes from another block. Reclaim is the cursor
            // sitting on the tail cell, which the sweep does not walk.
            assert_eq!(
                tail_addr,
                block.base_addr() + (block.next_index as usize) * cell,
                "the tail cell is the bump cursor and is not in the swept prefix"
            );
            let fresh = heap.alloc_cons(Value::NIL, Value::NIL);
            assert_ne!(
                fresh.as_cons_ptr().map(|cell| cell as usize),
                Some(handed_addr),
                "the next allocation must not hand out the surviving cell"
            );
        });
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}
