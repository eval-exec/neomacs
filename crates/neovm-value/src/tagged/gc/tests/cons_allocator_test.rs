//! Focused tests of the shared allocator, without making TaggedHeap Send.
use super::super::scan_contract::SingleMutatorWorld;
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

// Unrecorded GEN0/C1 fixtures need no canonical registry. This test-only
// identity is explicit; production constructors reuse TaggedHeap's identity.
fn standalone_allocator() -> ConsAllocator {
    ConsAllocator::new(HeapIdentity::issue())
}

fn record_canonical_carve(
    world: &mut SingleMutatorWorld<'_>,
    allocator: &mut ConsAllocator,
    base: ConsBlockBase,
) {
    let id = world.current_young_registration_id();
    let mut young = world.take_young_log_loan(id).unwrap();
    allocator.note_carve(base, young.log_mut()).unwrap();
    world.return_young_log_loan(young).unwrap();
}

fn unregister_canonical_block(
    world: &mut SingleMutatorWorld<'_>,
    allocator: &mut ConsAllocator,
    base: ConsBlockBase,
) {
    world
        .with_stopped_young_logs(|_, logs, _| {
            // SAFETY: callers retain every registered block and have no worker,
            // marker or future use of an outstanding region grant.
            unsafe { allocator.stopped_world() }
                .unregister(base, logs)
                .unwrap();
        })
        .unwrap();
}

fn verify_canonical_registry(world: &mut SingleMutatorWorld<'_>, allocator: &mut ConsAllocator) {
    world
        .with_stopped_young_logs(|_, logs, _| {
            // SAFETY: fixture owns all retained storage and excludes workers,
            // regions and markers; verifier visits metadata/canonical logs only.
            unsafe { allocator.stopped_world() }
                .verify_registry(logs)
                .unwrap();
        })
        .unwrap();
}

fn free_block(allocator: &mut ConsAllocator, cells: usize) -> ConsBlock {
    let mut block = ConsBlock::new();
    block.reserve_tail(cells);
    allocator.register(&block);
    let mut head = std::ptr::null_mut();
    let live = block.sweep(&mut head);
    assert_eq!(live, 0);
    // SAFETY: freshly owned block remains alive for each test, its sweep
    // produced an exact list and there are no mutators or outstanding regions.
    unsafe { allocator.publish(&block, head, live) };
    block
}

#[test]
fn cons_allocator_detaches_nonlast_block_and_withholds_returned_cells() {
    let mut allocator = standalone_allocator();
    let blocks: Vec<_> = (0..3).map(|_| free_block(&mut allocator, 32)).collect();
    let first = ConsBlockBase::of(&blocks[0]);
    let last = ConsBlockBase::of(&blocks[2]);
    // SAFETY: no worker or region exists in this exclusively owned fixture.
    unsafe { allocator.stopped_world() }.withhold([first]);
    allocator.verify();
    assert!(!allocator.is_ready(first));
    assert!(allocator.is_ready(last));
    // SAFETY: the withheld block remains owned; withholding emptied its old
    // list, so this range is exclusive and currently absent from every list.
    unsafe { allocator.return_unused(blocks[0].cells_ptr(), 8) };
    assert!(!allocator.is_ready(first));
    allocator.verify();
    // SAFETY: the first block is withheld, so the remaining two ready lists
    // are stable, valid, and exclude the returned region above.
    let (run, count) = unsafe { allocator.reserve_run(32) }.unwrap();
    assert_ne!(ConsBlock::block_base_for_ptr(run), first.addr());
    assert_eq!(count, 32);
    allocator.unregister(first);
    allocator.verify();
    for block in &blocks[1..] {
        allocator.unregister(ConsBlockBase::of(block));
    }
}

#[test]
fn cons_allocator_three_mutators_share_free_blocks_across_rebuilds() {
    enum WorkerCommand {
        Allocate(usize),
    }
    const MUTATORS: usize = 3;
    const CYCLES: usize = 128;
    const CARVES: usize = 32;
    let mut allocator = standalone_allocator();
    let mut blocks: Vec<_> = (0..3).map(|_| free_block(&mut allocator, 256)).collect();
    let bases: Vec<_> = blocks.iter().map(ConsBlock::base_addr).collect();
    let active: Vec<_> = (0..3 * 256).map(|_| AtomicBool::new(false)).collect();
    // SAFETY: the fixture has no workers, regions or marker yet.
    unsafe { allocator.stopped_world() }.admit_threads();
    {
        let allocator = allocator.shared();
        std::thread::scope(|scope| {
            // These senders belong INSIDE the scope closure: coordinator unwind
            // drops them before scoped thread joining, waking every idle worker.
            let mut commands = Vec::new();
            let (result_tx, results) = mpsc::channel();
            for worker in 0..MUTATORS {
                let allocator = &allocator;
                let active = &active;
                let bases = &bases;
                let (command_tx, command_rx) = mpsc::channel();
                commands.push(command_tx);
                let result_tx = result_tx.clone();
                scope.spawn(move || {
                    while let Ok(WorkerCommand::Allocate(cycle)) = command_rx.recv() {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            for carve in 0..CARVES {
                                let want = 1 + (worker + cycle + carve) % 17;
                                // SAFETY: parent retains blocks through join and
                                // collects only after every worker has replied.
                                let (run, count) = unsafe { allocator.reserve_run(want) }.unwrap();
                                let base = ConsBlock::block_base_for_ptr(run);
                                let block_index = bases.iter().position(|&b| b == base).unwrap();
                                for index in 0..count {
                                    // SAFETY: this reserved run is exclusively
                                    // owned and remains in its registered block.
                                    let cell = unsafe { run.add(index) };
                                    let slot = block_index * 256 + ConsBlock::index_of_ptr(cell);
                                    assert!(
                                        !active[slot].swap(true, Ordering::SeqCst),
                                        "overlapping runs"
                                    );
                                    // SAFETY: initialize privately owned cells.
                                    // No global test lock serializes cell writes.
                                    unsafe {
                                        (*cell).car.set_plain(TaggedValue::T);
                                        (*cell).set_cdr(TaggedValue::NIL);
                                    }
                                }
                                std::thread::yield_now();
                                for index in 0..count {
                                    // SAFETY: the entire run is still owned.
                                    let cell = unsafe { run.add(index) };
                                    let slot = block_index * 256 + ConsBlock::index_of_ptr(cell);
                                    assert!(active[slot].swap(false, Ordering::SeqCst));
                                }
                                // SAFETY: unreachable region is exclusive, and
                                // coordinator has not started any sweep.
                                unsafe { allocator.return_unused(run, count) };
                            }
                        }));
                        let failed = result.is_err();
                        if result_tx.send((worker, cycle, result)).is_err() || failed {
                            break;
                        }
                    }
                });
            }
            drop(result_tx);
            for cycle in 0..CYCLES {
                for command in &commands {
                    command.send(WorkerCommand::Allocate(cycle)).unwrap();
                }
                let mut replied = std::collections::HashSet::new();
                for _ in 0..MUTATORS {
                    let (worker, returned_cycle, result) = results.recv().unwrap();
                    result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                    assert_eq!(returned_cycle, cycle);
                    assert!(replied.insert(worker));
                }
                // Every worker is now blocked on its private command receiver;
                // successful replies are the stop-all/closed-region proof.
                assert!(active.iter().all(|slot| !slot.load(Ordering::SeqCst)));
                assert_eq!(allocator.free_count(), 3 * 256);
                allocator.verify();
                // SAFETY: every scoped worker is parked with its regions closed.
                unsafe { allocator.stopped_world() }.withhold(blocks.iter().map(ConsBlockBase::of));
                assert!(allocator.free_heads().is_empty());
                // SAFETY: every worker replied with all regions returned.
                assert!(unsafe { allocator.reserve_run(1) }.is_none());
                for block in &mut blocks {
                    block.clear_marks();
                    let mut head = std::ptr::null_mut();
                    let live = block.sweep(&mut head);
                    assert_eq!(live, 0);
                    // SAFETY: workers remain parked; this exact rebuilt list
                    // names storage retained by the coordinator until scope join.
                    unsafe { allocator.publish(block, head, live) };
                }
                allocator.verify();
            }
            // Normal completion and unwinding both disconnect idle receivers.
            drop(commands);
        });
    }
    for block in &blocks {
        allocator.unregister(ConsBlockBase::of(block));
    }
}

#[test]
fn cons_allocator_poison_prevents_collector_panic_containment() {
    let mut heap = TaggedHeap::new();
    assert!(!heap.gc_locks_poisoned());
    // SAFETY: no workers, regions or marker exist in this fresh fixture.
    unsafe { heap.cons_allocator.stopped_world() }.admit_threads();
    let shared = heap.cons_allocator.shared();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = shared.state.lock().unwrap();
        panic!("inject allocator protocol failure");
    }));
    assert!(result.is_err());
    assert!(heap.gc_locks_poisoned());
}

#[test]
fn cons_allocator_dense_slots_preserve_fragmented_run_and_return_order() {
    let mut allocator = standalone_allocator();
    let mut blocks = [free_block(&mut allocator, 8), free_block(&mut allocator, 7)];
    let bases = blocks.each_ref().map(ConsBlockBase::of);
    // SAFETY: no workers or outstanding reservations exist in this fixture.
    unsafe { allocator.stopped_world() }.withhold(bases);
    for (block, live_indices) in blocks.iter_mut().zip([&[2, 6][..], &[1, 5][..]]) {
        for &index in live_indices {
            ConsBlock::mark_run_at(block.base_addr(), index, 1, true);
        }
        let mut head = std::ptr::null_mut();
        let live = block.sweep(&mut head);
        // SAFETY: a stopped fixture rebuilt this exact list; blocks remain owned.
        unsafe { allocator.publish(block, head, live) };
    }
    // The newest published block's isolated first cell is a short run. Giving
    // it back must restore that same first run, rather than reorder blocks.
    // SAFETY: all free nodes remain in these registered, owned blocks.
    let (first, count) = unsafe { allocator.reserve_run(3) }.unwrap();
    assert_eq!(ConsBlockBase::of_cell(first), bases[1]);
    assert_eq!(ConsBlock::index_of_ptr(first), 0);
    assert_eq!(count, 1);
    // SAFETY: this uninitialized, unmarked run remains exclusively ours.
    unsafe { allocator.return_unused(first, count) };
    for (base, index, expected_count) in [
        (bases[1], 0, 1),
        (bases[1], 2, 3),
        (bases[1], 6, 1),
        (bases[0], 0, 2),
        (bases[0], 3, 3),
        (bases[0], 7, 1),
    ] {
        // SAFETY: fixture retains every block and owns every preceding grant.
        let (run, count) = unsafe { allocator.reserve_run(3) }.unwrap();
        assert_eq!(ConsBlockBase::of_cell(run), base);
        assert_eq!(ConsBlock::index_of_ptr(run), index);
        assert_eq!(count, expected_count);
        allocator.verify();
    }
    // SAFETY: storage is still retained; all free runs were reserved above.
    assert!(unsafe { allocator.reserve_run(1) }.is_none());
    for base in bases {
        allocator.unregister(base);
    }
    allocator.verify();
}

#[test]
fn cons_allocator_dense_slots_reuse_identity_after_unlink_and_withhold() {
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocator::new(heap.heap_identity());
    // SAFETY: this fixture owns the unused coordinator heap; no worker,
    // marker, Lisp callback or allocation safepoint reaches it.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let blocks = [
        free_block(&mut allocator, 9),
        free_block(&mut allocator, 5),
        free_block(&mut allocator, 7),
    ];
    let bases = blocks.each_ref().map(ConsBlockBase::of);
    let removed_slot = allocator.with_state(|state, _| state.slot(bases[1]));
    allocator.unregister(bases[1]);
    allocator.unregister(bases[1]); // Already absent: never enqueue a vacant slot twice.
    assert!(!allocator.is_ready(bases[1]));
    allocator.verify();
    // Keep the removed block owned, ensuring a different base takes its slot.
    let mut replacement = free_block(&mut allocator, 11);
    let replacement_base = ConsBlockBase::of(&replacement);
    assert_ne!(replacement_base, bases[1]);
    record_canonical_carve(&mut world, &mut allocator, replacement_base);
    assert_eq!(
        allocator.with_state(|state, _| state.slot(replacement_base)),
        removed_slot
    );
    assert_eq!(allocator.with_state(|state, _| state.slots.len()), 3);
    allocator.verify();
    // SAFETY: no active workers/regions; nonlast removals exercise the moved
    // ready entry's direct metadata backpointer before the slot is reused.
    unsafe { allocator.stopped_world() }.withhold([bases[0], replacement_base]);
    // SAFETY: withholding removed every replacement node; this is an exclusive
    // returned range in retained storage and must remain unavailable.
    unsafe { allocator.return_unused(replacement.cells_ptr(), 3) };
    assert!(!allocator.is_ready(replacement_base));
    // SAFETY: only the final original block remains available.
    let (run, count) = unsafe { allocator.reserve_run(64) }.unwrap();
    assert_eq!(ConsBlockBase::of_cell(run), bases[2]);
    assert_eq!(count, 7);
    // SAFETY: all available nodes were granted; the replacement is withheld.
    assert!(unsafe { allocator.reserve_run(1) }.is_none());
    for _ in 0..64 {
        unregister_canonical_block(&mut world, &mut allocator, replacement_base);
        allocator.register(&replacement);
        allocator.with_state(|state, _| {
            assert_eq!(state.slot(replacement_base), removed_slot);
            assert_eq!(
                state.slots.len(),
                3,
                "slot storage grew with registration churn"
            );
            let entry = state.block(removed_slot);
            assert!(!entry.young.load(Ordering::Relaxed));
            assert_eq!(entry.young_log_slot, None);
            assert_eq!(entry.old_count, 0);
            assert_eq!(entry.free_before_sweep, None);
            assert_eq!(entry.head, 0);
            assert_eq!(entry.count, 0);
            assert_eq!(entry.available_slot, None);
            assert_eq!(entry.availability, BlockAvailability::Ready);
        });
        let mut head = std::ptr::null_mut();
        let live = replacement.sweep(&mut head);
        assert_eq!(live, 0);
        // SAFETY: no replacement grant remains active; its exact list was rebuilt.
        unsafe { allocator.publish(&replacement, head, live) };
        allocator.verify();
        // SAFETY: the replacement is the only available block and remains owned.
        let (run, count) = unsafe { allocator.reserve_run(64) }.unwrap();
        assert_eq!(ConsBlockBase::of_cell(run), replacement_base);
        assert_eq!(count, 11);
        record_canonical_carve(&mut world, &mut allocator, replacement_base);
        verify_canonical_registry(&mut world, &mut allocator);
    }
    unregister_canonical_block(&mut world, &mut allocator, replacement_base);
    allocator.unregister(bases[0]);
    allocator.unregister(bases[2]);
    allocator.verify();
}

#[test]
fn sole_allocator_admission_preserves_free_run_inventory() {
    let mut allocator = standalone_allocator();
    let block = free_block(&mut allocator, 8);
    let base = ConsBlockBase::of(&block);
    assert!(!allocator.threads_admitted());
    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = allocator.shared();
    }));
    assert!(refused.is_err());
    assert!(!allocator.threads_admitted());
    // SAFETY: the fixture retains its registered block and consumes this
    // complete grant, leaving no open reservation at the admission boundary.
    let (first, count) = unsafe { allocator.reserve_run(3) }.unwrap();
    assert_eq!(count, 3);
    for index in 0..count {
        // SAFETY: all cells in the private run are initialized by its owner.
        unsafe {
            (*first.add(index)).car.set_plain(TaggedValue::T);
            (*first.add(index)).set_cdr(TaggedValue::NIL);
        }
    }
    let slot = allocator.with_state(|state, _| state.slot(base));
    assert_eq!(allocator.free_count(), 5);
    assert!(allocator.sweep_work_snapshot().is_empty());
    // SAFETY: a stopped fixture owns every cell and has no marker or worker.
    unsafe { allocator.stopped_world() }.admit_threads();
    assert!(allocator.threads_admitted());
    {
        let shared = allocator.shared();
        assert_eq!(shared.free_count(), 5);
        shared.with_state(|state, _| assert_eq!(state.slot(base), slot));
        // SAFETY: all registered storage is still retained.
        let (rest, count) = unsafe { shared.reserve_run(8) }.unwrap();
        assert_eq!(ConsBlock::index_of_ptr(rest), 3);
        assert_eq!(count, 5);
        // SAFETY: this entire second grant remains unhanded and unmarked.
        unsafe { shared.return_unused(rest, count) };
        shared.verify();
    }
    assert!(allocator.threads_admitted());
    allocator.unregister(base);
}

#[test]
fn sole_allocator_stop_all_unwind_is_poisoned() {
    let mut heap = TaggedHeap::new();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: the fresh fixture has no worker, region or marker.
        let mut stopped = unsafe { heap.cons_allocator.stopped_world() };
        stopped.state.blocks_with_free.reserve(8);
        panic!("inject sole collector metadata failure");
    }));
    assert!(failure.is_err());
    assert!(heap.gc_locks_poisoned());
    assert!(!heap.cons_allocator.threads_admitted());
}

// Additional C2 canonical-registry regressions. This is an UNCOMPILED scratch
// extension projected onto actual C3 with complete sweep obligations; production
// and all previously delivered C2 patches remain unchanged.
fn canonical_fixture_mode(allocator: &mut ConsAllocator, mode: AllocatorAccessMode) {
    match mode {
        AllocatorAccessMode::Sole => {}
        AllocatorAccessMode::Admitted => {
            // SAFETY: fresh fixture is stopped with no worker, marker or grant.
            unsafe { allocator.stopped_world() }.admit_threads();
        }
    }
}

fn record_canonical_owner_carve(
    world: &mut SingleMutatorWorld<'_>,
    allocator: &mut ConsAllocator,
    registration: YoungRegistrationId,
    base: ConsBlockBase,
) {
    let mut young = world.take_young_log_loan(registration).unwrap();
    allocator.note_carve(base, young.log_mut()).unwrap();
    world.return_young_log_loan(young).unwrap();
}

#[test]
fn cons_allocator_canonical_wrong_heap_carve_preserves_flags_and_logs() {
    for mode in [AllocatorAccessMode::Sole, AllocatorAccessMode::Admitted] {
        let owner_heap = TaggedHeap::new();
        let mut foreign_heap = TaggedHeap::new();
        let mut allocator = ConsAllocator::new(owner_heap.heap_identity());
        let block = free_block(&mut allocator, 8);
        let base = ConsBlockBase::of(&block);
        canonical_fixture_mode(&mut allocator, mode);
        // SAFETY: real grant remains exclusive/unhanded through the refusal;
        // this fixture retains the block and no collector or worker runs.
        let (run, count) = unsafe { allocator.reserve_run(3) }.unwrap();
        assert_eq!(count, 3);
        let snapshot = |allocator: &ConsAllocator| {
            allocator.with_state(|state, _| {
                let block = state.block_for_base(base);
                (
                    state.identity,
                    (
                        block.young.load(Ordering::Relaxed),
                        block.young_log_slot,
                        block.head,
                        block.count,
                        block.available_slot,
                        block.availability,
                    ),
                    state.pending_mutators.clone(),
                    state.mark_work.clone(),
                    state.blocks_with_free.clone(),
                    state.vacant_slots.clone(),
                )
            })
        };
        let before = snapshot(&allocator);
        // SAFETY: foreign heap is exclusively owned and unused; only its
        // young payload is loaned locally, without Lisp/TLS aliases or marker.
        let mut foreign = unsafe { SingleMutatorWorld::from_heap(&mut foreign_heap) };
        let registration = foreign.current_young_registration_id();
        let mut young = foreign.take_young_log_loan(registration).unwrap();
        let before_log = {
            let log = young.log_mut();
            (log.id(), log.blocks().to_vec(), log.pending_index())
        };
        assert_ne!(registration.heap_identity(), owner_heap.heap_identity());
        assert_eq!(
            allocator.note_carve(base, young.log_mut()),
            Err(YoungRegistryError::WrongHeap)
        );
        assert_eq!(snapshot(&allocator), before);
        let after_log = {
            let log = young.log_mut();
            (log.id(), log.blocks().to_vec(), log.pending_index())
        };
        assert_eq!(after_log, before_log);
        assert!(
            !allocator.is_poisoned(),
            "typed refusal is before mutation/unwind"
        );
        foreign.return_young_log_loan(young).unwrap();
        foreign.ensure_young_logs_stopped().unwrap();
        // SAFETY: refusal handed out/initialized nothing; refund the exact
        // still-exclusive run before unrecorded cleanup.
        unsafe { allocator.return_unused(run, count) };
        // A wrong-domain call must not record a young block at all; the original
        // unrecorded cleanup path remains valid and retains storage until done.
        allocator.unregister(base);
        allocator.verify();
    }
}

#[test]
fn cons_allocator_canonical_wrong_heap_empty_capture_preserves_metadata() {
    for mode in [AllocatorAccessMode::Sole, AllocatorAccessMode::Admitted] {
        let owner_heap = TaggedHeap::new();
        let mut foreign_heap = TaggedHeap::new();
        let mut allocator = ConsAllocator::new(owner_heap.heap_identity());
        let block = free_block(&mut allocator, 8);
        let base = ConsBlockBase::of(&block);
        canonical_fixture_mode(&mut allocator, mode);
        let snapshot = |allocator: &ConsAllocator| {
            allocator.with_state(|state, _| {
                let block = state.block_for_base(base);
                (
                    state.identity,
                    (
                        block.young.load(Ordering::Relaxed),
                        block.young_log_slot,
                        block.head,
                        block.count,
                        block.available_slot,
                        block.availability,
                    ),
                    state.pending_mutators.clone(),
                    state.mark_work.clone(),
                    state.blocks_with_free.clone(),
                    state.vacant_slots.clone(),
                )
            })
        };
        let before = snapshot(&allocator);
        assert!(before.2.is_empty());
        assert!(before.3.is_empty());
        // SAFETY: foreign heap is unused and has no outstanding loan, marker,
        // worker, callback or TLS alias. Its valid resolver has the WRONG domain.
        let mut foreign = unsafe { SingleMutatorWorld::from_heap(&mut foreign_heap) };
        let result = foreign
            .with_stopped_young_logs(|_, logs, _| {
                // SAFETY: external storage is retained; no workers or regions exist.
                unsafe { allocator.stopped_world() }.capture_minor(logs)
            })
            .unwrap();
        assert_eq!(result, Err(YoungRegistryError::WrongHeap));
        assert_eq!(snapshot(&allocator), before);
        assert!(!allocator.is_poisoned());
        allocator.unregister(base);
        allocator.verify();
    }
}

#[test]
fn cons_allocator_canonical_unlinks_swapped_owner_positions_before_base_reuse() {
    for mode in [AllocatorAccessMode::Sole, AllocatorAccessMode::Admitted] {
        let mut heap = TaggedHeap::new();
        let mut allocator = ConsAllocator::new(heap.heap_identity());
        let mut blocks: Vec<_> = (0..4).map(|_| free_block(&mut allocator, 8)).collect();
        let bases: Vec<_> = blocks.iter().map(ConsBlockBase::of).collect();
        canonical_fixture_mode(&mut allocator, mode);
        // SAFETY: coordinator owns every paired registration and retained block;
        // no Lisp allocation, marker, callback or TLS alias enters this fixture.
        let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
        let registrations: Vec<_> = (0..3)
            .map(|_| {
                let physical = world.register_mutator_gc_for_test();
                world.registered_young_id_for_test(physical).unwrap()
            })
            .collect();
        let [a, b, c] = registrations.as_slice() else {
            panic!("three owners")
        };
        let [a, b, c] = [*a, *b, *c];
        // Reserve real free runs first, then record/refund them in canonical
        // owner order A0,A1,B2,C3. These are actual grants, not synthetic nodes.
        let grants: Vec<_> = (0..4)
            .map(|_| {
                // SAFETY: all four owned blocks remain registered and retained;
                // there is no worker/collector and each grant is disjoint.
                unsafe { allocator.reserve_run(8) }.unwrap()
            })
            .collect();
        for (registration, block_index) in [(a, 0), (a, 1), (b, 2), (c, 3)] {
            let base = bases[block_index];
            let &(run, count) = grants
                .iter()
                .find(|(run, _)| ConsBlock::block_base_for_ptr(*run) == base.addr())
                .unwrap();
            assert_eq!(count, 8);
            record_canonical_owner_carve(&mut world, &mut allocator, registration, base);
            // SAFETY: this entire grant stayed unhanded/unmarked and exclusive.
            // It is no longer used after refund; all physical storage stays owned.
            unsafe { allocator.return_unused(run, count) };
        }
        world
            .with_stopped_young_logs(|_, logs, _| {
                // SAFETY: all loans/grants returned; no worker or marker exists.
                let mut stopped = unsafe { allocator.stopped_world() };
                assert_eq!(stopped.state.pending_mutators, vec![a, b, c]);
                assert_eq!(logs.resolve_mut(a).unwrap().blocks(), &[bases[0], bases[1]]);
                assert_eq!(
                    logs.resolve_mut(a)
                        .unwrap()
                        .pending_index()
                        .unwrap()
                        .index(),
                    0
                );
                assert_eq!(
                    logs.resolve_mut(b)
                        .unwrap()
                        .pending_index()
                        .unwrap()
                        .index(),
                    1
                );
                assert_eq!(
                    logs.resolve_mut(c)
                        .unwrap()
                        .pending_index()
                        .unwrap()
                        .index(),
                    2
                );
                stopped.verify_registry(logs).unwrap();

                // Nonlast block removal moves A1 into A's index0.
                stopped.unregister(bases[0], logs).unwrap();
                assert_eq!(logs.resolve_mut(a).unwrap().blocks(), &[bases[1]]);
                let moved_block = stopped
                    .state
                    .block_for_base(bases[1])
                    .young_log_slot
                    .unwrap();
                assert_eq!(moved_block.registration, a);
                assert_eq!(moved_block.index.index(), 0);
                stopped.verify_registry(logs).unwrap();

                // Emptying A removes nonlast pending index0 and moves C from2->0.
                stopped.unregister(bases[1], logs).unwrap();
                assert!(logs.resolve_mut(a).unwrap().blocks().is_empty());
                assert_eq!(logs.resolve_mut(a).unwrap().pending_index(), None);
                assert_eq!(stopped.state.pending_mutators, vec![c, b]);
                assert_eq!(
                    logs.resolve_mut(c)
                        .unwrap()
                        .pending_index()
                        .unwrap()
                        .index(),
                    0
                );
                assert_eq!(
                    logs.resolve_mut(b)
                        .unwrap()
                        .pending_index()
                        .unwrap()
                        .index(),
                    1
                );
                assert!(!stopped.state.blocks.contains_key(&bases[0]));
                assert!(!stopped.state.blocks.contains_key(&bases[1]));
                stopped.verify_registry(logs).unwrap();
            })
            .unwrap();

        // Re-register EXACTLY the same still-owned base A0 after canonical
        // unlink, proving that no stale A0 log/backlink can survive address reuse.
        allocator.register(&blocks[0]);
        let mut head = std::ptr::null_mut();
        let live = blocks[0].sweep(&mut head);
        assert_eq!(live, 0);
        // SAFETY: A0 remains owned, no grant/worker/marker exists, and this exact
        // rebuilt list contains all eight free cells before publication.
        unsafe { allocator.publish(&blocks[0], head, live) };
        // SAFETY: newly published A0 is the newest ready block; run is exclusive.
        let (run, count) = unsafe { allocator.reserve_run(8) }.unwrap();
        assert_eq!(ConsBlock::block_base_for_ptr(run), bases[0].addr());
        assert_eq!(count, 8);
        record_canonical_owner_carve(&mut world, &mut allocator, a, bases[0]);
        // SAFETY: fully unused, unmarked, exclusive grant is returned before stop.
        unsafe { allocator.return_unused(run, count) };
        world
            .with_stopped_young_logs(|_, logs, _| {
                // SAFETY: all real grants/loans are closed and storage remains owned.
                let mut stopped = unsafe { allocator.stopped_world() };
                assert_eq!(stopped.state.pending_mutators, vec![c, b, a]);
                assert_eq!(logs.resolve_mut(a).unwrap().blocks(), &[bases[0]]);
                stopped.verify_registry(logs).unwrap();
                let work = stopped.capture_minor(logs).unwrap();
                assert_eq!(work.len(), 3);
                assert_eq!(
                    work.iter()
                        .copied()
                        .collect::<std::collections::HashSet<_>>(),
                    [bases[2], bases[3], bases[0]]
                        .into_iter()
                        .collect::<std::collections::HashSet<_>>()
                );
                assert!(!work.contains(&bases[1]));
                // Every run was completely refunded: actual promotion has no live
                // young cell. C3 additionally hands off and withholds sweep work.
                for base in &work {
                    let block = blocks
                        .iter_mut()
                        .find(|block| ConsBlockBase::of(block) == *base)
                        .unwrap();
                    assert_eq!(block.promote_marked_world_stopped(), 0);
                }
                stopped
                    .finish_promotion(
                        super::super::generational::GenerationCycle::Minor,
                        &blocks,
                        0,
                        logs,
                    )
                    .unwrap();
                assert!(stopped.state.mark_work.is_empty());
                assert!(stopped.state.pending_mutators.is_empty());
                stopped.verify_registry(logs).unwrap();
                let sweep_work = stopped.state.sweep_work.clone();
                assert_eq!(sweep_work, work);
                // Release metadata access before publication reacquires it.
                // The outer canonical world still excludes every mutator.
                drop(stopped);
                for base in &sweep_work {
                    let block = blocks
                        .iter_mut()
                        .find(|block| ConsBlockBase::of(block) == *base)
                        .unwrap();
                    let mut head = std::ptr::null_mut();
                    let live = block.sweep_generational(&mut head);
                    assert_eq!(live, 0);
                    // SAFETY: every grant was fully refunded, no worker runs,
                    // storage is owned and this withheld list was rebuilt exactly.
                    unsafe { allocator.publish_generational_verified(block, head, live) };
                }
                assert_eq!(allocator.take_completed_sweep_freed(), 0);
                let candidates = allocator.take_empty_candidates();
                assert_eq!(
                    candidates
                        .iter()
                        .copied()
                        .collect::<std::collections::HashSet<_>>(),
                    sweep_work
                        .iter()
                        .copied()
                        .collect::<std::collections::HashSet<_>>()
                );
                // SAFETY: SAME physical stop-all, all sweep visits completed,
                // metadata accounting consumed, no region/worker/marker exists.
                let mut stopped = unsafe { allocator.stopped_world() };
                stopped.verify_registry(logs).unwrap();
                for base in &bases {
                    // A1 is already absent; all remaining canonical links must be
                    // gone BEFORE any owned ConsBlock can be dropped below.
                    stopped.unregister(*base, logs).unwrap();
                }
                stopped.verify_registry(logs).unwrap();
                assert!(stopped.state.blocks.is_empty());
            })
            .unwrap();
        allocator.verify();
        for registration in registrations {
            world
                .retire_young_registration_for_test(registration)
                .unwrap()
                .acknowledge();
        }
    }
}
