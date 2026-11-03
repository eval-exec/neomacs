//! Real admitted cons storage: Rust-only owners, rooted results, and A5 stops.
#![cfg(not(feature = "gc-memory-telemetry"))]
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::gc::card_table::ChunkSlot;
use crate::tagged::gc::chunk_map::{ChunkClass, ChunkEntry};
use crate::tagged::gc::cons_allocator::ConsBlockBase;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

fn retry<T>(mut f: impl FnMut() -> Result<T, RendezvousError>) -> T {
    loop {
        match f() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) => thread::yield_now(),
            Err(error) => panic!("cons protocol: {error:?}"),
        }
    }
}
fn retry_registration<T>(mut f: impl FnMut() -> Result<T, RegistrationError>) -> T {
    loop {
        match f() {
            Ok(value) => return value,
            Err(RegistrationError::Busy) => thread::yield_now(),
            Err(error) => panic!("cons registration: {error:?}"),
        }
    }
}
fn activate<'era>(mut permit: MutatorPermit<'era>, heap: HeapIdentity, id: u64) -> Mutator<'era> {
    loop {
        match permit.activate(heap, id) {
            Ok(owner) => return owner,
            Err(failure) if failure.reason == RegistrationError::Busy => {
                permit = failure.into_permit();
                thread::yield_now();
            }
            Err(failure) => panic!("activate cons owner: {:?}", failure.reason),
        }
    }
}
fn with_run<R>(owner: &mut Mutator<'_>, body: impl FnOnce(&mut RunAccess<'_, '_>) -> R) -> R {
    loop {
        match owner.run_access() {
            Ok(mut run) => {
                let result = body(&mut run);
                retry_registration(|| run.close_cons_region());
                return result;
            }
            Err(RegistrationError::Busy) => thread::yield_now(),
            Err(error) => panic!("run cons owner: {error:?}"),
        }
    }
}
fn apply(mut image: ResumeGcImage<'_, '_>) {
    loop {
        match image.apply() {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                image = failure.into_image();
                thread::yield_now();
            }
            Err(failure) => panic!("restore cons owner: {:?}", failure.reason),
        }
    }
}
fn complete(mut stopped: AllStopped<'_>) {
    loop {
        match stopped.complete() {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                stopped = failure.into_stopped();
                thread::yield_now();
            }
            Err(failure) => panic!("complete cons stop: {:?}", failure.reason),
        }
    }
}
fn retire(mut owner: Mutator<'_>) {
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("retire cons owner: {:?}", failure.reason),
        }
    }
}
fn receive<T>(rx: &mpsc::Receiver<T>) -> T {
    rx.recv_timeout(Duration::from_secs(30))
        .expect("cons actor reaches bounded boundary")
}
fn roots(context: &mut Context) -> ([SharedRoot; 2], SharedRoot) {
    // SAFETY: immediate values are live on this serial heap; no heap object
    // or raw Value is transported to an owner.
    unsafe {
        (
            [
                SharedRoot::new(&context.tagged_heap, Value::fixnum(1701)),
                SharedRoot::new(&context.tagged_heap, Value::fixnum(1702)),
            ],
            SharedRoot::new(&context.tagged_heap, Value::NIL),
        )
    }
}
fn check_catalog(heap: &TaggedHeap) {
    heap.cons_allocator.verify();
    for (index, block) in heap.cons_blocks.iter().enumerate() {
        let base = block.base_addr();
        assert_eq!(heap.cons_block_index_by_base.get(&base), Some(&index));
        assert!(heap.cons_allocator.is_ready(ConsBlockBase::of(block)));
        if let Some(map) = heap.chunk_map.as_deref() {
            assert_eq!(map.get(base), ChunkEntry::new(ChunkClass::Cons, index));
        }
        if heap.generational_enabled() {
            assert!(
                heap.card_slots()
                    .unwrap()
                    .blocks(ChunkSlot::of(base))
                    .contains(&base)
            );
            assert!(block.trailer().ages_match_old_bits());
        }
    }
}
fn check_pair(heap: &TaggedHeap, root: &SharedRoot, car: i64) {
    let local = root.materialize(heap).unwrap();
    let value = local.value();
    assert!(heap.owns_heap_value_for_test(value));
    // SAFETY: the matching stopped heap retains this rooted initialized cell;
    // owners hold no run/grant and cannot write it during inspection.
    unsafe {
        let cell = &*value.as_cons_ptr().unwrap();
        assert_eq!(cell.car.as_fixnum(), Some(car));
        assert!(cell.cdr().is_nil());
    }
}
struct PoisonOnUnwind<'a>(&'a Registry);
impl Drop for PoisonOnUnwind<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.poison();
        }
    }
}

/// This exact test is the Rust-only two-owner TSan selector. It does not use
/// external fixture blocks, install heap TLS, execute Lisp, or lock around
/// independent cell initialization. The ready/go phase witnesses two live run
/// authorities; the stress itself does not claim a specific CPU instruction
/// from each actor executes at precisely the same instant.
/// Production correspondence of the admission scan controls: an IN_SLICE
/// admission word refuses the catalog transition even when every gated
/// precheck passes. A legally admitted slice can never reach the transition
/// (Running entries are refused earlier), so the scan is verified by the
/// exact condition it exists to catch: an admission word left IN_SLICE with
/// the entry otherwise Starting. Clearing the word lets the same transition
/// proceed.
#[test]
pub(super) fn in_slice_admission_word_refuses_catalog_transition() {
    super::admission_probe::in_slice_admission_word_refuses_catalog_transition();
}

#[test]
fn two_owner_cons_allocator_stress() {
    const PHASES: usize = 2;
    const PER_OWNER: usize = 4200; // exceeds a real cons block per owner
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let heap = era.heap_identity();
        let permits = [
            era.register(1701, 0).unwrap(),
            era.register(1702, 0).unwrap(),
        ];
        let active = Arc::new(AtomicUsize::new(0));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let (acked_tx, acked_rx) = mpsc::channel();
        thread::scope(|scope| {
            let _abort = PoisonOnUnwind(era.registry());
            // Senders are inside the scope closure: coordinator unwind
            // disconnects actors before thread::scope attempts to join.
            let mut commands = Vec::new();
            for (index, (permit, car)) in permits.into_iter().zip(cars).enumerate() {
                let (go_tx, go_rx) = mpsc::channel();
                commands.push(go_tx);
                let ready = ready_tx.clone();
                let results = result_tx.clone();
                let acked = acked_tx.clone();
                let active = Arc::clone(&active);
                let nil = nil.clone();
                scope.spawn(move || {
                    let mut owner = activate(permit, heap, 1701 + index as u64);
                    let mut all_roots = Vec::new();
                    for phase in 0..PHASES {
                        let allocated = with_run(&mut owner, |run| {
                            active.fetch_add(1, Ordering::SeqCst);
                            ready.send((index, phase)).unwrap();
                            if go_rx.recv().is_err() {
                                active.fetch_sub(1, Ordering::SeqCst);
                                return None;
                            }
                            let mut allocated = Vec::with_capacity(PER_OWNER);
                            for sequence in 0..PER_OWNER {
                                allocated.push(retry_registration(|| run.cons(&car, &nil)));
                                if sequence % 31 == 0 {
                                    thread::yield_now();
                                }
                            }
                            active.fetch_sub(1, Ordering::SeqCst);
                            Some(allocated)
                        });
                        let Some(allocated) = allocated else { return };
                        all_roots.extend(allocated.iter().cloned());
                        results.send((index, phase, allocated)).unwrap();
                        let epoch = retry(|| owner.wait_request());
                        retry(|| owner.acknowledge(epoch, &all_roots));
                        // Owner-exclusive tails return at the ACK, not at the
                        // run close: signal only once the catalog is coherent.
                        acked.send(()).unwrap();
                        apply(owner.begin_restore_retry(false).unwrap());
                    }
                    drop(all_roots);
                    retire(owner);
                });
            }
            drop(ready_tx);
            drop(result_tx);
            drop(acked_tx);
            let mut addresses = HashSet::new();
            for phase in 0..PHASES {
                let first = receive(&ready_rx);
                let second = receive(&ready_rx);
                assert_eq!((first.1, second.1), (phase, phase));
                assert_ne!(first.0, second.0);
                assert_eq!(active.load(Ordering::SeqCst), 2);
                for go in &commands {
                    go.send(()).unwrap();
                }
                let responses = [receive(&result_rx), receive(&result_rx)];
                // Owners park in wait_request until the keeper requests the
                // epoch; the ACK barrier can only drain after that request.
                retry(|| era.rendezvous().request());
                // Both ACKs publish every held tail suffix before this
                // snapshot can observe a coherent zero-charge catalog.
                for _ in 0..2 {
                    receive(&acked_rx);
                }
                let catalog = era.cons_catalog_snapshot_for_test().unwrap();
                assert!(catalog.installed && catalog.metadata_coherent);
                assert_eq!(catalog.active_regions, 0);
                assert!(
                    catalog.source_grants[2] > 0,
                    "stress installs real fresh blocks"
                );
                let mut stopped = retry(|| era.stop());
                retry_registration(|| era.return_cons_storage(&stopped));
                retry(|| {
                    era.with_stopped_heap_for_test(&stopped, |heap| {
                        check_catalog(heap);
                        for (index, returned_phase, roots) in &responses {
                            assert_eq!(*returned_phase, phase);
                            for root in roots {
                                assert!(
                                    addresses.insert(root.cons_word_for_test()),
                                    "overlapping live cells"
                                );
                                check_pair(heap, root, 1701 + *index as i64);
                            }
                        }
                    })
                });
                let report = retry(|| era.collect_stopped(&mut stopped));
                assert!(report.major);
                retry(|| {
                    era.with_stopped_heap_for_test(&stopped, |heap| {
                        check_catalog(heap);
                        for (index, _, roots) in &responses {
                            for root in roots {
                                check_pair(heap, root, 1701 + *index as i64);
                            }
                        }
                    })
                });
                if phase + 1 != PHASES {
                    retry_registration(|| era.resume_cons_storage(&stopped));
                }
                complete(stopped);
            }
        });
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn cons_result_survives_from_published_head_then_reclaims_after_serial_restore() {
    Context::with_test_collection_era_then(
        roots,
        |era, (cars, nil)| {
            era.start_cons_storage().unwrap();
            let mut owner = activate(era.register(1710, 1).unwrap(), era.heap_identity(), 1710);
            let (head, words) = with_run(&mut owner, |run| {
                let child = run.cons(&cars[0], &nil).unwrap();
                let head = run.cons(&child, &nil).unwrap();
                let words = [head.cons_word_for_test(), child.cons_word_for_test()];
                drop(child); // only the head's car retains the child
                (head, words)
            });
            owner.enter_native(std::slice::from_ref(&head)).unwrap();
            drop(head); // only the authoritative publication retains the graph
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped(&mut stopped).unwrap();
            era.with_stopped_heap_for_test(&stopped, |heap| {
                for word in words {
                    assert!(heap.owns_heap_value_for_test(Value::from_bits(word)));
                }
                // SAFETY: exact stopped publication retains the complete graph.
                unsafe {
                    let head = &*Value::from_bits(words[0]).as_cons_ptr().unwrap();
                    assert_eq!(head.car.bits(), words[1]);
                    let child = &*head.car.as_cons_ptr().unwrap();
                    assert_eq!(child.car.as_fixnum(), Some(1701));
                }
            })
            .unwrap();
            complete(stopped);
            owner.leave_native().unwrap();
            with_run(&mut owner, |run| run.state().root_scratch.clear());
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            (collector, keeper, words)
        },
        |context, words| {
            context.gc_collect_exact();
            for word in words {
                assert!(
                    !context
                        .tagged_heap
                        .owns_heap_value_for_test(Value::from_bits(word))
                );
            }
        },
    );
}

#[test]
fn cons_without_catalog_refuses_before_charging_or_producing_a_root() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        let mut owner = activate(era.register(1711, 0).unwrap(), era.heap_identity(), 1711);
        with_run(&mut owner, |run| {
            let before = (
                run.state().hot.allocated_count,
                run.state().hot.bytes_since_gc,
            );
            assert_eq!(
                run.cons(&cars[0], &nil).unwrap_err(),
                RegistrationError::Publication
            );
            assert_eq!(
                (
                    run.state().hot.allocated_count,
                    run.state().hot.bytes_since_gc
                ),
                before
            );
            assert!(run.cons_region_for_test().is_none());
        });
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn foreign_cons_operand_refuses_before_any_grant_and_correct_inputs_still_work() {
    let foreign_heap = TaggedHeap::new();
    // SAFETY: NIL is immediate and live; this root carries a real foreign brand.
    let foreign = unsafe { SharedRoot::new(&foreign_heap, Value::NIL) };
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1712, 0).unwrap(), era.heap_identity(), 1712);
        let root = with_run(&mut owner, |run| {
            let before = (
                run.state().hot.allocated_count,
                run.state().hot.bytes_since_gc,
            );
            for (car, cdr) in [(&foreign, &nil), (&cars[0], &foreign)] {
                assert_eq!(
                    run.cons(car, cdr).unwrap_err(),
                    RegistrationError::WrongHeap
                );
                assert_eq!(
                    (
                        run.state().hot.allocated_count,
                        run.state().hot.bytes_since_gc
                    ),
                    before
                );
                assert!(run.cons_region_for_test().is_none());
            }
            run.cons(&cars[0], &nil).unwrap()
        });
        owner.enter_native(std::slice::from_ref(&root)).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| check_pair(heap, &root, 1701))
            .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop(root);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn dropping_open_cons_region_poison_retains_storage_and_refuses_publication() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1713, 0).unwrap(), era.heap_identity(), 1713);
        let root = {
            let mut run = owner.run_access().unwrap();
            let root = run.cons(&cars[0], &nil).unwrap();
            assert!(run.cons_region_for_test().is_some());
            drop(run); // deliberately omit explicit close; Drop cannot block
            root
        };
        assert_eq!(
            owner.enter_native(std::slice::from_ref(&root)),
            Err(RendezvousError::Poisoned)
        );
        assert_eq!(era.start_cons_storage(), Err(RegistrationError::Poisoned));
        drop((owner, root));
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::OpenRegion
        );
        // The fixture intentionally does not resume the suspended Context.
        // Abandoned era/owner retain backing instead of accepting a fake stop.
    });
}

#[test]
fn collection_requires_returned_catalog_and_resume_requires_current_stop() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1714, 0).unwrap(), era.heap_identity(), 1714);
        let root = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        owner.enter_native(std::slice::from_ref(&root)).unwrap();
        let mut stopped = era.stop().unwrap();
        assert_eq!(
            era.collect_stopped(&mut stopped).unwrap_err(),
            RendezvousError::Publication
        );
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        let second = with_run(&mut owner, |run| run.cons(&cars[1], &nil).unwrap());
        owner.enter_native(&[root.clone(), second.clone()]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            check_pair(heap, &root, 1701);
            check_pair(heap, &second, 1702);
        })
        .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop((root, second));
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn earlier_tail_close_never_rewinds_through_another_owners_live_region() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let heap = era.heap_identity();
        let mut first = activate(era.register(1715, 0).unwrap(), heap, 1715);
        let mut second = activate(era.register(1716, 0).unwrap(), heap, 1716);
        // Serial bootstrap GC may leave ready free nodes. Consume them through
        // the REAL lasting API until a fresh-source witness, rather than
        // assuming the first grant happens to be a bump tail.
        let seeds = with_run(&mut first, |run| {
            let mut seeds = Vec::new();
            for _ in 0..100_000 {
                seeds.push(run.cons(&cars[0], &nil).unwrap());
                if era.cons_catalog_snapshot_for_test().unwrap().source_grants[2] != 0 {
                    return seeds;
                }
            }
            panic!("bounded bootstrap free storage never reached a fresh block")
        });
        let before = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(before.metadata_coherent);
        // The seeding owner still holds its exclusive tail credit: closed
        // regions no longer return storage until the owner ACKs.
        assert!(before.active_regions >= 1);
        let mut a = first.run_access().unwrap();
        let a_root = a.cons(&cars[0], &nil).unwrap();
        let a_bounds = a.cons_region_for_test().unwrap();
        let mut b = second.run_access().unwrap();
        let b_root = b.cons(&cars[1], &nil).unwrap();
        let b_bounds = b.cons_region_for_test().unwrap();
        let during = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(during.active_regions >= 2, "each owner holds a tail");
        assert!(
            (during.source_grants[0] + during.source_grants[1] + during.source_grants[2])
                >= (before.source_grants[0] + before.source_grants[1] + before.source_grants[2])
                    + 2,
            "both owners carved from newly lent tails"
        );
        // Owners never share a tail any more: their exclusive ranges are
        // disjoint by construction instead of by grant ordering.
        assert!(
            a_bounds.1 <= b_bounds.0 || b_bounds.1 <= a_bounds.0,
            "owner tails are disjoint"
        );
        // A bootstrap free fragment may hand a whole one-cell run, so an
        // open region can already be empty after its first cons; only the
        // handed ranges and their disjointness are protocol invariants.
        a.close_cons_region().unwrap();
        let after_a = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(after_a.active_regions >= 1);
        // The local rewind returns nothing through the shared gate.
        assert_eq!(after_a.free_count, during.free_count);
        assert!(a.cons_region_for_test().is_none());
        // The earlier suffix is reusable while B still retains its own live
        // region: A's next cell is the immediate tail successor of its own
        // initialized prefix, never a cell of B or the free list.
        let reused = a.cons(&cars[0], &nil).unwrap();
        let reused_word = reused.cons_word_for_test();
        if a_bounds.2 >= 1 {
            assert_eq!(
                reused_word,
                a_root.cons_word_for_test() + std::mem::size_of::<crate::tagged::gc::ConsCell>(),
                "the closed region's suffix serves the next local carve"
            );
        }
        // Whatever the fragment geometry, the next local carve never serves
        // a cell inside B's live exclusive region.
        assert!(reused_word < b_bounds.0 || reused_word >= b_bounds.1);
        a.close_cons_region().unwrap();
        b.close_cons_region().unwrap();
        assert!(a.cons_region_for_test().is_none() && b.cons_region_for_test().is_none());
        drop((a, b));
        let mut first_roots = seeds;
        first_roots.extend([a_root.clone(), reused.clone()]);
        first.enter_native(&first_roots).unwrap();
        second.enter_native(std::slice::from_ref(&b_root)).unwrap();
        let acked = era.cons_catalog_snapshot_for_test().unwrap();
        assert_eq!(acked.active_regions, 0, "both ACKs published every tail");
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            check_catalog(heap);
            check_pair(heap, &a_root, 1701);
            check_pair(heap, &b_root, 1702);
            check_pair(heap, &reused, 1701);
        })
        .unwrap();
        complete(stopped);
        first.leave_native().unwrap();
        second.leave_native().unwrap();
        drop((first_roots, a_root, b_root, reused));
        retire(first);
        retire(second);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn foreign_stop_cannot_return_or_resume_a_cons_catalog() {
    // Construct/capture the catalog World LAST: current serial Context TLS
    // caches are not restored by dropping a separately constructed Context.
    // The foreign World needs only a metadata stop; do not collect its old
    // frozen census after the inner Context has changed those caches.
    Context::with_test_collection_era(
        |_| (),
        |foreign, ()| {
            let mut foreign_stop = foreign.stop().unwrap();
            Context::with_test_collection_era(roots, |era, (cars, nil)| {
                era.start_cons_storage().unwrap();
                let before = era.cons_catalog_snapshot_for_test().unwrap();
                assert_eq!(
                    era.return_cons_storage(&foreign_stop),
                    Err(RegistrationError::WrongHeap)
                );
                assert_eq!(
                    era.resume_cons_storage(&foreign_stop),
                    Err(RegistrationError::WrongHeap)
                );
                let after = era.cons_catalog_snapshot_for_test().unwrap();
                assert_eq!(after.blocks, before.blocks);
                assert_eq!(after.live_count, before.live_count);
                assert_eq!(after.free_count, before.free_count);
                assert!(after.installed && after.metadata_coherent);
                // A real allocation and matching stop still succeed after
                // refusal, on the World whose frozen census was captured last.
                let mut owner = activate(era.register(1717, 0).unwrap(), era.heap_identity(), 1717);
                let root = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
                owner.enter_native(std::slice::from_ref(&root)).unwrap();
                let mut stopped = era.stop().unwrap();
                era.return_cons_storage(&stopped).unwrap();
                era.collect_stopped(&mut stopped).unwrap();
                era.with_stopped_heap_for_test(&stopped, |heap| {
                    check_catalog(heap);
                    check_pair(heap, &root, 1701);
                })
                .unwrap();
                complete(stopped);
                owner.leave_native().unwrap();
                drop(root);
                retire(owner);
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
            });
            foreign_stop.trace(|_, _| Ok(())).unwrap();
            complete(foreign_stop);
            let (collector, keeper) = foreign.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}

#[test]
fn return_cons_storage_preserves_busy_stop_validation_for_retry() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1718, 0).unwrap(), era.heap_identity(), 1718);
        let root = with_run(&mut owner, |run| run.cons(&cars[0], &nil).unwrap());
        owner.enter_native(std::slice::from_ref(&root)).unwrap();
        let mut stopped = era.stop().unwrap();
        thread::scope(|scope| {
            let registry = era.registry();
            let (held_tx, held_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let actor = scope.spawn(move || {
                let _gate = registry.gate.state.lock().unwrap();
                held_tx.send(()).unwrap();
                receive(&release_rx);
            });
            receive(&held_rx);
            let refusal = era.return_cons_storage(&stopped);
            // Release BEFORE assertions so a failure never strands the join.
            release_tx.send(()).unwrap();
            actor.join().unwrap();
            assert_eq!(refusal, Err(RegistrationError::Busy));
            assert!(!registry.park.poisoned.load(Ordering::Acquire));
        });
        retry_registration(|| era.return_cons_storage(&stopped));
        era.collect_stopped(&mut stopped).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| check_pair(heap, &root, 1701))
            .unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        drop(root);
        retire(owner);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn forgotten_open_cons_run_refuses_publication_native_and_finish_without_waiting() {
    Context::with_test_collection_era(roots, |era, (cars, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = activate(era.register(1719, 0).unwrap(), era.heap_identity(), 1719);
        let mut run = owner.run_access().unwrap();
        let root = run.cons(&cars[0], &nil).unwrap();
        assert!(run.cons_region_for_test().is_some());
        // Unlike the Drop regression, no destructor can publish poison or
        // return this grant. Only the canonical live-loan ledger remains.
        std::mem::forget(run);
        assert!(owner.entry.admitted());
        assert!(!era.registry().park.poisoned.load(Ordering::Acquire));
        let before = era.cons_catalog_snapshot_for_test().unwrap();
        assert!(before.installed && before.metadata_coherent);
        // One lent tail (or a multi-run batch) stays charged until the owner
        // publishes; the forgotten run loan blocks that publication.
        assert!(before.active_regions >= 1);
        let epoch = std::num::NonZeroU64::MIN;
        assert_eq!(
            owner.begin_publication(epoch).unwrap().publish(),
            Err(RegistrationError::OutstandingLoan)
        );
        assert_eq!(
            owner.enter_native(std::slice::from_ref(&root)),
            Err(RendezvousError::Loan)
        );
        let failure = owner.finish(epoch).unwrap_err();
        assert_eq!(failure.reason, RegistrationError::OutstandingLoan);
        let owner = failure.into_owner();
        let after = era.cons_catalog_snapshot_for_test().unwrap();
        assert_eq!(
            after, before,
            "refusals preserve the sole grant and backing"
        );
        // Never call stop: this forgotten owner can no longer acknowledge.
        // Forget its wrapper, then exercise the actual era failure retention.
        std::mem::forget(owner);
        let failure = era.finish().unwrap_err();
        assert_eq!(failure.reason, RegistrationError::OpenRegion);
        drop(failure);
        drop(root);
    });
}

#[test]
fn partial_cons_grant_refunds_owner_and_global_credits_then_merges_history_once() {
    use crate::tagged::gc::{ConsCell, MemoryUseCountSlot};
    use std::mem::size_of;

    Context::with_test_collection_era_then(
        |context| {
            let inputs = roots(context);
            let heap = &context.tagged_heap;
            (
                inputs,
                heap.cons_live_count,
                heap.memory_use_counts_snapshot(),
                heap.total_allocated_bytes(),
            )
        },
        |era, ((cars, nil), base_live, base_memory, base_bytes)| {
            let table_charge = era.symbol_table_admission_charge_for_test();
            assert!(
                table_charge.get() > 0,
                "admission retains the published table"
            );
            era.start_cons_storage().unwrap();
            let before = era.cons_catalog_snapshot_for_test().unwrap();
            assert_eq!(before.live_count, base_live);
            let mut owner = activate(era.register(1720, 0).unwrap(), era.heap_identity(), 1720);
            let mut run = owner.run_access().unwrap();
            assert_eq!(
                table_charge,
                run.initial_table_statistics_for_test()
                    .unwrap()
                    .retained_bytes,
                "admission charged exactly the published table's retained storage"
            );
            let mut allocated = Vec::new();
            // Bootstrap free lists may offer one-cell runs. Keep actual cells
            // rooted until a genuine partial region is observed; never inject
            // a fake charge or assume an allocator source/layout.
            let unused = loop {
                assert!(allocated.len() < 100_000, "bounded real partial grant");
                allocated.push(run.cons(&cars[0], &nil).unwrap());
                let unused = run.cons_region_for_test().unwrap().2;
                if unused != 0 {
                    break unused;
                }
            };
            let handed = allocated.len();
            let charged = handed + unused;
            let slot = MemoryUseCountSlot::ConsCells.index();
            {
                let hot = &run.state().hot;
                assert_eq!(hot.allocated_count, charged);
                assert_eq!(hot.bytes_since_gc, charged * size_of::<ConsCell>());
                assert_eq!(hot.memory_use_counts[slot], charged as u64);
                assert_eq!(hot.bytes_banked_at_resets, 0);
                assert_eq!(hot.pacing, Default::default());
            }
            let during = era.cons_catalog_snapshot_for_test().unwrap();
            // The whole lent tail batch is charged globally until the ACK.
            assert!(during.active_regions >= 1);
            assert!(during.live_count >= base_live + charged);
            run.close_cons_region().unwrap();
            {
                let hot = &run.state().hot;
                assert_eq!(hot.allocated_count, handed);
                assert_eq!(hot.bytes_since_gc, handed * size_of::<ConsCell>());
                assert_eq!(hot.memory_use_counts[slot], handed as u64);
                assert_eq!(hot.bytes_banked_at_resets, 0);
                assert_eq!(hot.pacing, Default::default());
            }
            assert!(run.cons_region_for_test().is_none());
            let closed = era.cons_catalog_snapshot_for_test().unwrap();
            // A local close moves no global credit: the suffix stays in the
            // owner's exclusive tail until publication.
            assert!(closed.active_regions >= 1);
            assert_eq!(closed.live_count, during.live_count);
            drop(run);
            let mut expected_memory = base_memory;
            expected_memory[slot] = expected_memory[slot].wrapping_add(handed as u64);
            // The cons owner refunds only its unused region. Owner 0 also
            // retains admission's separately charged immutable pointer table;
            // both histories merge into the stopped cumulative byte total.
            let expected_bytes = base_bytes
                + (handed * size_of::<ConsCell>()) as u64
                + u64::try_from(table_charge.get()).unwrap();
            owner.enter_native(&allocated).unwrap();
            let acked = era.cons_catalog_snapshot_for_test().unwrap();
            assert_eq!(acked.active_regions, 0, "the ACK published the tail");
            assert_eq!(
                acked.live_count,
                base_live + handed,
                "publication refunds exactly the unhanded suffix"
            );
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            let report = era.collect_stopped(&mut stopped).unwrap();
            assert_eq!(report.memory_use_counts, expected_memory);
            assert_eq!(report.total_allocated_bytes, expected_bytes);
            complete(stopped);
            owner.leave_native().unwrap();
            with_run(&mut owner, |run| {
                let hot = &run.state().hot;
                assert_eq!(hot.allocated_count, 0, "GC rebases live balance to owner 0");
                assert_eq!(hot.bytes_since_gc, 0);
                assert_eq!(
                    hot.bytes_banked_at_resets,
                    (handed * size_of::<ConsCell>()) as u64
                );
                assert_eq!(hot.memory_use_counts[slot], handed as u64);
            });
            drop(allocated);
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            (
                collector,
                keeper,
                (report.allocated_count, expected_memory, expected_bytes),
            )
        },
        |context, (live, memory, bytes)| {
            // Inspect BEFORE another GC: serial reunion itself must preserve
            // exactly the post-sweep balance and cumulative owner histories.
            assert_eq!(context.tagged_heap.allocated_count(), live);
            assert_eq!(context.tagged_heap.memory_use_counts_snapshot(), memory);
            assert_eq!(context.tagged_heap.total_allocated_bytes(), bytes);
        },
    );
}
