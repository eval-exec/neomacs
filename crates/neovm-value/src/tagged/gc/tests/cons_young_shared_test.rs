//! Shared GEN1 fixture using the SAME canonical hot/cold SATB registrations.
//! The coordinator heap never allocates Lisp cons
//! cells here; its canonical owner logs bind exclusively to the external fixture
//! allocator. Only the shared allocator view and owned young-only loans travel
//! to workers. TaggedHeap, SATB buffers and registry control never cross threads.

use super::cons_allocator::{ConsAllocator, ConsBlockBase, SharedConsAllocator};
use super::scan_contract::SingleMutatorWorld;
use super::young_registry::YoungLogLoan;
use super::*;
use std::collections::HashSet;
use std::sync::mpsc;

#[derive(Clone, Copy, Debug)]
struct HandedCell {
    address: usize,
    rooted: bool,
}

fn allocator_young_fixture_block(allocator: &mut ConsAllocator, cells: usize) -> ConsBlock {
    let mut block = ConsBlock::new();
    block.reserve_tail(cells);
    allocator.register(&block);
    let mut head = std::ptr::null_mut();
    let live = block.sweep(&mut head);
    assert_eq!(live, 0);
    // SAFETY: the coordinator retains this storage until every worker joins;
    // this initial sweep rebuilt exactly the block's allocated-capacity nodes.
    unsafe { allocator.publish(&block, head, live) };
    block
}

fn allocator_young_fixture_owner(blocks: &[ConsBlock], address: usize) -> usize {
    let base = ConsBlock::block_base_for_ptr(address as *const ConsCell);
    blocks
        .iter()
        .position(|block| block.base_addr() == base)
        .unwrap()
}

/// The coordinator has received ALL workers' replies and returned ALL their
/// actual owned payloads. Workers are blocked on command receivers with every
/// region suffix refunded. No marker or residual major exists. Real ConsBlocks
/// are retained here; no test-wide lock serializes their independent writes.
fn allocator_young_fixture_minor(
    world: &mut SingleMutatorWorld<'_>,
    allocator: &SharedConsAllocator<'_>,
    blocks: &mut [ConsBlock],
    handed: &[HandedCell],
) {
    world.ensure_young_logs_stopped().unwrap();
    world
        .with_stopped_young_logs(|_unused_heap_allocator, logs, _unused_heap_blocks| {
            let free_before = allocator.free_count();
            // SAFETY: all owned loans have returned and all workers are parked;
            // blocks are retained and closed through this entire collection.
            let mut stopped = unsafe { allocator.stopped_world() };
            stopped.verify_young_at_begin(blocks, logs).unwrap();
            let mark_work = stopped.capture_minor(logs).unwrap();
            let expected_bases: HashSet<_> = handed
                .iter()
                .map(|cell| {
                    ConsBlockBase::of(&blocks[allocator_young_fixture_owner(blocks, cell.address)])
                })
                .collect();
            assert_eq!(
                mark_work.iter().copied().collect::<HashSet<_>>(),
                expected_bases
            );
            assert_eq!(
                mark_work.len(),
                expected_bases.len(),
                "duplicate young work"
            );
            for base in &mark_work {
                let index = blocks
                    .iter()
                    .position(|b| b.base_addr() == base.addr())
                    .unwrap();
                blocks[index].clear_marks();
            }
            for cell in handed.iter().filter(|cell| cell.rooted) {
                let index = allocator_young_fixture_owner(blocks, cell.address);
                let offset = cell.address - blocks[index].base_addr();
                assert!(blocks[index].mark_cell_offset(offset));
            }
            let mut promoted = 0;
            for base in &mark_work {
                let index = blocks
                    .iter()
                    .position(|b| b.base_addr() == base.addr())
                    .unwrap();
                let newly_old = blocks[index].promote_marked_world_stopped();
                promoted += newly_old;
                stopped.record_minor_promoted(*base, newly_old);
            }
            assert_eq!(promoted, handed.iter().filter(|cell| cell.rooted).count());
            let total_old = blocks.iter().map(ConsBlock::count_old).sum();
            // Same physical stop through promotion/flag clear/sweep withholding.
            // No command is sent until this helper has completed its whole sweep.
            stopped
                .finish_promotion(
                    generational::GenerationCycle::Minor,
                    blocks,
                    total_old,
                    logs,
                )
                .unwrap();
            stopped.verify_after_promotion(blocks, logs).unwrap();
            drop(stopped); // Release the metadata mutex before publish acquires it.
            let sweep_work = allocator.sweep_work_snapshot();
            assert_eq!(sweep_work, mark_work);
            for base in sweep_work {
                let index = blocks
                    .iter()
                    .position(|b| b.base_addr() == base.addr())
                    .unwrap();
                let block = &mut blocks[index];
                let mut head = std::ptr::null_mut();
                let live = block.sweep_generational(&mut head);
                // SAFETY: workers remain parked; the exact rebuilt list excludes
                // old/live cells, and all registered storage remains retained.
                unsafe { allocator.publish_generational_verified(block, head, live) };
            }
            let expected_freed = handed.iter().filter(|cell| !cell.rooted).count();
            assert_eq!(allocator.take_completed_sweep_freed(), expected_freed);
            assert_eq!(allocator.free_count(), free_before + expected_freed);
            // Fixture retains its fixed blocks instead of releasing empty candidates.
            let _retained_candidates = allocator.take_empty_candidates();
            allocator.verify();
            for cell in handed {
                let owner = &blocks[allocator_young_fixture_owner(blocks, cell.address)];
                let index = ConsBlock::index_of_ptr(cell.address as *const ConsCell);
                assert_eq!(owner.trailer().is_old(index), cell.rooted);
                // SAFETY: owned storage and completed sweep determine this cell;
                // every worker remains blocked and cannot write it concurrently.
                let car = unsafe { (*(cell.address as *const ConsCell)).car.get() };
                assert_eq!(
                    car.bits(),
                    if cell.rooted {
                        TaggedValue::T.bits()
                    } else {
                        TaggedValue::DEAD.bits()
                    }
                );
            }
        })
        .unwrap();
}

fn allocator_young_fixture_unregister(
    world: &mut SingleMutatorWorld<'_>,
    allocator: &SharedConsAllocator<'_>,
    blocks: &[ConsBlock],
) {
    world
        .with_stopped_young_logs(|_, logs, _| {
            // SAFETY: workers have joined and no region or marker exists. Every
            // block stays owned until all canonical backlinks are unlinked here.
            let mut stopped = unsafe { allocator.stopped_world() };
            for block in blocks {
                stopped.unregister(ConsBlockBase::of(block), logs).unwrap();
            }
        })
        .unwrap();
}

#[test]
fn young_three_mutators_allocate_shared_blocks_across_eighty_minors() {
    enum WorkerCommand {
        Allocate { cycle: usize, young: YoungLogLoan },
    }
    const MUTATORS: usize = 3;
    const CYCLES: usize = 80;
    const RUNS_PER_MUTATOR: usize = 4;
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocator::new(heap.heap_identity());
    // 480 old roots accumulate; fixed 1536-cell capacity suffices.
    let mut blocks: Vec<_> = (0..3)
        .map(|_| allocator_young_fixture_block(&mut allocator, 512))
        .collect();
    // SAFETY: no worker or region exists before external allocator admission.
    unsafe { allocator.stopped_world() }.admit_threads();
    assert!(allocator.threads_admitted());
    let allocator = allocator.shared();
    let bases: Vec<_> = blocks.iter().map(ConsBlockBase::of).collect();
    let mut old_roots = HashSet::new();
    std::thread::scope(|scope| {
        // This closure executes on the coordinator, not a worker. Heap/SATB
        // remain confined; only young payloads move through command/result T.
        // SAFETY: fixture owns the whole coordinator heap with no aliases,
        // marker, Lisp callbacks, allocation safepoints or heap cons regions.
        let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
        let registrations: Vec<_> = (0..MUTATORS)
            .map(|_| {
                let physical = world.register_mutator_gc_for_test();
                world.registered_young_id_for_test(physical).unwrap()
            })
            .collect();
        // Senders MUST live inside scope: coordinator unwind disconnects idle
        // receivers before scoped joining, so an assertion cannot deadlock it.
        let mut commands = Vec::new();
        let (result_tx, results) = mpsc::channel();
        for worker in 0..MUTATORS {
            let allocator = &allocator;
            let bases = &bases;
            let (command_tx, command_rx) = mpsc::channel();
            commands.push(command_tx);
            let result_tx = result_tx.clone();
            scope.spawn(move || {
                while let Ok(WorkerCommand::Allocate { cycle, mut young }) = command_rx.recv() {
                    // Borrow, rather than move, young into the panic boundary:
                    // a test assertion still returns its actual payload.
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut handed = Vec::new();
                        for run_index in 0..RUNS_PER_MUTATOR {
                            let want = 3 + (worker + cycle + run_index) % 15;
                            // SAFETY: coordinator retains blocks through join
                            // and collects only after all actual loans return.
                            let (run, count) = unsafe { allocator.reserve_run(want) }.unwrap();
                            let kept = count.min(2);
                            let base_addr = ConsBlock::block_base_for_ptr(run);
                            let base = *bases.iter().find(|base| base.addr() == base_addr).unwrap();
                            allocator.note_carve(base, young.log_mut()).unwrap();
                            for index in 0..kept {
                                // SAFETY: this reserved run is exclusive; no
                                // collector runs in the command phase.
                                let cell = unsafe { run.add(index) };
                                // SAFETY: initialize privately before channel
                                // publication, without any global test lock.
                                unsafe {
                                    (*cell).car.set_plain(TaggedValue::T);
                                    (*cell).set_cdr(TaggedValue::NIL);
                                }
                                handed.push(HandedCell {
                                    address: cell as usize,
                                    rooted: index == 0 && run_index % 2 == 0,
                                });
                            }
                            // SAFETY: unused suffix is exclusive/unmarked,
                            // unhanded and in the same retained block.
                            unsafe { allocator.return_unused(run.add(kept), count - kept) };
                            std::thread::yield_now();
                        }
                        handed
                    }));
                    let failed = result.is_err();
                    // The registry/SATB never cross this channel, only the
                    // unique young-only loan, addresses and panic payload.
                    if result_tx.send((worker, cycle, young, result)).is_err() || failed {
                        break;
                    }
                }
            });
        }
        drop(result_tx);
        for cycle in 0..CYCLES {
            for (worker, command) in commands.iter().enumerate() {
                let young = world.take_young_log_loan(registrations[worker]).unwrap();
                command
                    .send(WorkerCommand::Allocate { cycle, young })
                    .unwrap();
            }
            let mut responses = Vec::new();
            for _ in 0..MUTATORS {
                let (worker, returned_cycle, young, result) = results.recv().unwrap();
                // Return EVERY actual loan before result assertions/resuming a
                // worker panic. Parking alone never grants a capture admission.
                world.return_young_log_loan(young).unwrap();
                responses.push((worker, returned_cycle, result));
            }
            world.ensure_young_logs_stopped().unwrap();
            let mut replied = HashSet::new();
            let mut handed = Vec::new();
            for (worker, returned_cycle, result) in responses {
                let cells = result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                assert_eq!(returned_cycle, cycle);
                assert!(replied.insert(worker));
                handed.extend(cells);
            }
            let mut addresses = HashSet::new();
            for cell in &handed {
                assert!(addresses.insert(cell.address), "overlapping worker runs");
                assert!(
                    !old_roots.contains(&cell.address),
                    "allocated an old live cell"
                );
            }
            allocator_young_fixture_minor(&mut world, &allocator, &mut blocks, &handed);
            old_roots.extend(
                handed
                    .iter()
                    .filter(|cell| cell.rooted)
                    .map(|cell| cell.address),
            );
        }
        for registration in registrations {
            world
                .retire_young_registration_for_test(registration)
                .unwrap()
                .acknowledge();
        }
        drop(commands); // Normal completion also disconnects idle workers.
    });
    // SAFETY: every worker joined; coordinator still owns the unused heap and
    // external storage. No log loan, worker, marker or region remains.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    allocator_young_fixture_unregister(&mut world, &allocator, &blocks);
}

#[test]
fn young_log_from_exited_worker_survives_until_minor_capture() {
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocator::new(heap.heap_identity());
    let mut blocks = vec![allocator_young_fixture_block(&mut allocator, 32)];
    let base = ConsBlockBase::of(&blocks[0]);
    // SAFETY: sole coordinator owns closed storage before starting the worker.
    unsafe { allocator.stopped_world() }.admit_threads();
    let allocator = allocator.shared();
    // SAFETY: coordinator owns this unused heap with no marker/TLS aliases.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let physical = world.register_mutator_gc_for_test();
    let registration = world.registered_young_id_for_test(physical).unwrap();
    let young = world.take_young_log_loan(registration).unwrap();
    let (young, handed) = std::thread::scope(|scope| {
        let allocator = &allocator;
        scope
            .spawn(move || {
                let mut young = young;
                // SAFETY: parent retains registered storage through join/capture;
                // no collector runs before the worker has exited and returned loan.
                let (run, count) = unsafe { allocator.reserve_run(3) }.unwrap();
                assert_eq!(count, 3);
                allocator.note_carve(base, young.log_mut()).unwrap();
                for index in 0..2 {
                    // SAFETY: worker privately owns this reserved run.
                    unsafe {
                        (*run.add(index)).car.set_plain(TaggedValue::T);
                        (*run.add(index)).set_cdr(TaggedValue::NIL);
                    }
                }
                // SAFETY: third cell is an exclusive unhanded suffix.
                unsafe { allocator.return_unused(run.add(2), 1) };
                (
                    young,
                    [
                        HandedCell {
                            address: run as usize,
                            rooted: true,
                        },
                        HandedCell {
                            // SAFETY: second cell lies in the retained three-cell region.
                            address: unsafe { run.add(1) } as usize,
                            rooted: false,
                        },
                    ],
                )
            })
            .join()
            .unwrap()
    });
    // Physical exit alone is insufficient: explicitly return the actual payload,
    // then retire via the SAME canonical hot/cold SATB owner path before capture.
    world.return_young_log_loan(young).unwrap();
    world
        .retire_young_registration_for_test(registration)
        .unwrap()
        .acknowledge();
    world
        .with_stopped_young_logs(|_, logs, _| {
            let log = logs.resolve_mut(registration).unwrap();
            assert_eq!(log.blocks(), &[base]);
            assert!(log.pending_index().is_some());
        })
        .unwrap();
    allocator_young_fixture_minor(&mut world, &allocator, &mut blocks, &handed);
    allocator_young_fixture_unregister(&mut world, &allocator, &blocks);
}

#[test]
fn young_admission_retains_preexisting_carve_for_next_minor() {
    let mut heap = TaggedHeap::new();
    let mut allocator = ConsAllocator::new(heap.heap_identity());
    let mut blocks = vec![allocator_young_fixture_block(&mut allocator, 32)];
    let base = ConsBlockBase::of(&blocks[0]);
    // SAFETY: fixture exclusively owns the unused heap and retained blocks.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let registration = world.current_young_registration_id();
    let mut young = world.take_young_log_loan(registration).unwrap();
    // SAFETY: sole owner retains all registered storage; no worker exists.
    let (run, count) = unsafe { allocator.reserve_run(3) }.unwrap();
    assert_eq!(count, 3);
    allocator.note_carve(base, young.log_mut()).unwrap(); // actual Sole/plain path
    for index in 0..2 {
        // SAFETY: initialize both handed cells before closing the grant.
        unsafe {
            (*run.add(index)).car.set_plain(TaggedValue::T);
            (*run.add(index)).set_cdr(TaggedValue::NIL);
        }
    }
    // SAFETY: exclusive unhanded suffix is refunded before admission.
    unsafe { allocator.return_unused(run.add(2), 1) };
    world.return_young_log_loan(young).unwrap();
    assert!(!allocator.threads_admitted());
    assert!(allocator.sweep_work_snapshot().is_empty());
    world
        .with_stopped_young_logs(|_, logs, _| {
            assert_eq!(logs.resolve_mut(registration).unwrap().blocks(), &[base]);
        })
        .unwrap();
    // SAFETY: stopped sole owner has all regions closed and all loans returned.
    unsafe { allocator.stopped_world() }.admit_threads();
    assert!(allocator.threads_admitted());
    let shared = allocator.shared();
    let handed = [
        HandedCell {
            address: run as usize,
            rooted: true,
        },
        HandedCell {
            // SAFETY: second handed cell remains in the owned retained block.
            address: unsafe { run.add(1) } as usize,
            rooted: false,
        },
    ];
    allocator_young_fixture_minor(&mut world, &shared, &mut blocks, &handed);
    allocator_young_fixture_unregister(&mut world, &shared, &blocks);
}
