//! Actual local census slots over the admitted cons core and stopped collector.
#![cfg(not(feature = "gc-memory-telemetry"))]
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::transport::{SharedRoot, with_root_table_lock_for_test};
use crate::tagged::value::TaggedValue as Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

fn seeds(context: &mut Context) -> (SharedRoot, SharedRoot) {
    // SAFETY: these immediate values are live on this serial source heap.
    unsafe {
        (
            SharedRoot::new(&context.tagged_heap, Value::fixnum(8123)),
            SharedRoot::new(&context.tagged_heap, Value::NIL),
        )
    }
}
fn with_run<'era, R>(
    owner: &mut Mutator<'era>,
    body: impl FnOnce(&mut RunAccess<'_, 'era>) -> R,
) -> R {
    let mut run = owner.run_access().unwrap();
    let result = body(&mut run);
    run.close_cons_region().unwrap();
    result
}
fn owner<'era>(era: &'era ParallelHeapEra<'_>, id: u64, budget: usize) -> Mutator<'era> {
    era.register(id, budget)
        .unwrap()
        .activate(era.heap_identity(), id)
        .unwrap()
}
fn complete(stopped: AllStopped<'_>) {
    stopped.complete().unwrap();
}
fn finish_owners<const N: usize>(era: &ParallelHeapEra<'_>, mut owners: [Mutator<'_>; N]) {
    for owner in &mut owners {
        with_run(owner, |run| run.clear_root_slots().unwrap());
        owner.enter_native(&[]).unwrap();
    }
    let mut stopped = era.stop().unwrap();
    era.return_cons_storage(&stopped).unwrap();
    era.collect_stopped(&mut stopped).unwrap();
    complete(stopped);
    for owner in owners {
        owner.retire_rooted(&[]).unwrap();
    }
}
fn finish(era: ParallelHeapEra<'_>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    allocated: usize,
    bytes: usize,
    memory: Vec<u64>,
    region: Option<(usize, usize, usize)>,
    roots: Vec<usize>,
    capacity: usize,
    generation: u64,
}
fn snapshot(run: &mut RunAccess<'_, '_>) -> Snapshot {
    let region = run.cons_region_for_test();
    let local = run.state();
    Snapshot {
        allocated: local.hot.allocated_count,
        bytes: local.hot.bytes_since_gc,
        memory: local.hot.memory_use_counts.to_vec(),
        region,
        roots: local
            .root_scratch
            .iter()
            .map(|w| w.value().bits())
            .collect(),
        capacity: local.root_scratch.capacity(),
        generation: local.root_slot_generation,
    }
}

#[test]
fn local_chain_is_rooted_only_by_its_image_across_two_collections_then_clear_reclaims() {
    Context::with_test_collection_era_then(
        seeds,
        |era, (car, nil)| {
            era.start_cons_storage().unwrap();
            let mut owner = owner(&era, 1810, 3);
            let (car_slot, mut tail, mut scratch) = with_run(&mut owner, |run| {
                (
                    run.root_slot(&car).unwrap(),
                    run.root_slot(&nil).unwrap(),
                    run.root_slot(&nil).unwrap(),
                )
            });
            drop(car);
            drop(nil);
            let mut words = Vec::new();
            for phase in 0..2 {
                with_run(&mut owner, |run| {
                    for _ in 0..140 {
                        run.cons_into(&car_slot, &tail, &mut scratch).unwrap();
                        std::mem::swap(&mut tail, &mut scratch);
                        words.push(tail.word_for_test(run));
                    }
                });
                owner.enter_native(&[]).unwrap();
                let mut stopped = era.stop().unwrap();
                era.return_cons_storage(&stopped).unwrap();
                stopped
                    .with_owner_image(owner.key(), |image| {
                        assert_eq!(image.summary().root_words, 3);
                        let mut roots = Vec::new();
                        image.visit_logs(|word| roots.push(word.value().bits()));
                        assert!(roots.contains(words.last().unwrap()));
                    })
                    .unwrap();
                era.collect_stopped(&mut stopped).unwrap();
                era.with_stopped_heap_for_test(&stopped, |heap| {
                    for &word in &words {
                        assert!(heap.owns_heap_value_for_test(Value::from_bits(word)));
                    }
                    let mut cursor = Value::from_bits(*words.last().unwrap());
                    for _ in 0..words.len() {
                        // SAFETY: the exact stopped heap has just traced this
                        // complete graph from the owner's sole local census.
                        let cell = unsafe { &*cursor.as_cons_ptr().unwrap() };
                        assert_eq!(cell.car.as_fixnum(), Some(8123));
                        // SAFETY: the same stopped, fully initialized cell remains live.
                        cursor = unsafe { cell.cdr() };
                    }
                    assert!(cursor.is_nil());
                })
                .unwrap();
                if phase == 0 {
                    era.resume_cons_storage(&stopped).unwrap();
                }
                complete(stopped);
                owner.leave_native().unwrap();
                with_run(&mut owner, |run| {
                    assert_eq!(tail.word_for_test(run), *words.last().unwrap());
                    assert_eq!(run.state().root_scratch.len(), 3);
                });
            }
            with_run(&mut owner, |run| run.clear_root_slots().unwrap());
            owner.retire_rooted(&[]).unwrap();
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
fn local_cons_does_not_wait_for_the_shared_root_table_lock() {
    Context::with_test_collection_era(
        |context| {
            let input = seeds(context);
            let value = context
                .tagged_heap
                .alloc_cons(Value::fixnum(99), Value::NIL);
            // SAFETY: the serial setup keeps this root/table alive throughout
            // the lock witness; it is not an operand or result of local cons.
            let anchor = unsafe { SharedRoot::new(&context.tagged_heap, value) };
            (input, anchor)
        },
        |era, ((car, nil), anchor)| {
            era.start_cons_storage().unwrap();
            let mut owner = owner(&era, 1811, 3);
            with_run(&mut owner, |run| {
                let a = run.root_slot(&car).unwrap();
                let d = run.root_slot(&nil).unwrap();
                let mut out = run.root_slot(&nil).unwrap();
                let held = Arc::new(AtomicBool::new(false));
                let heap = era.heap_identity();
                thread::scope(|scope| {
                    let held_by_actor = Arc::clone(&held);
                    let (ready_tx, ready_rx) = mpsc::channel();
                    let (release_tx, release_rx) = mpsc::channel();
                    let actor = scope.spawn(move || {
                        with_root_table_lock_for_test(heap, || {
                            held_by_actor.store(true, Ordering::Release);
                            ready_tx.send(()).unwrap();
                            // A regression that tries to register is released
                            // by the watchdog, then fails the held assertion.
                            let _ = release_rx.recv_timeout(Duration::from_secs(3));
                            held_by_actor.store(false, Ordering::Release);
                        });
                    });
                    ready_rx.recv_timeout(Duration::from_secs(30)).unwrap();
                    for _ in 0..260 {
                        run.cons_into(&a, &d, &mut out).unwrap();
                    }
                    assert!(
                        held.load(Ordering::Acquire),
                        "local cons waited for root registration"
                    );
                    release_tx.send(()).unwrap();
                    actor.join().unwrap();
                });
                assert_ne!(out.word_for_test(run), nil.cons_word_for_test());
            });
            drop(anchor);
            finish_owners(&era, [owner]);
            finish(era);
        },
    );
}

#[test]
fn root_slot_budget_is_fixed_and_failure_does_not_grow_or_change_the_bank() {
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, 1812, 2);
        with_run(&mut owner, |run| {
            let a = run.root_slot(&car).unwrap();
            let d = run.root_slot(&nil).unwrap();
            let before = snapshot(run);
            assert!(matches!(
                run.root_slot(&car),
                Err(RegistrationError::Capacity)
            ));
            assert_eq!(snapshot(run), before);
            assert_eq!(a.word_for_test(run), car.cons_word_for_test());
            assert_eq!(d.word_for_test(run), nil.cons_word_for_test());
        });
        finish_owners(&era, [owner]);
        finish(era);
    });
}

#[test]
fn stale_operand_and_destination_after_index_reuse_refuse_before_any_allocation() {
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, 1813, 3);
        with_run(&mut owner, |run| {
            let old_car = run.root_slot(&car).unwrap();
            let old_cdr = run.root_slot(&nil).unwrap();
            let mut old_result = run.root_slot(&car).unwrap();
            run.clear_root_slots().unwrap();
            let a = run.root_slot(&car).unwrap();
            let d = run.root_slot(&nil).unwrap();
            let mut result = run.root_slot(&car).unwrap();
            let before = snapshot(run);
            assert_eq!(
                run.cons_into(&old_car, &d, &mut result),
                Err(RegistrationError::StaleRegistration)
            );
            assert_eq!(snapshot(run), before);
            assert_eq!(
                run.cons_into(&a, &old_cdr, &mut result),
                Err(RegistrationError::StaleRegistration)
            );
            assert_eq!(snapshot(run), before);
            assert_eq!(
                run.cons_into(&a, &d, &mut old_result),
                Err(RegistrationError::StaleRegistration)
            );
            assert_eq!(snapshot(run), before);
            run.cons_into(&a, &d, &mut result).unwrap();
            assert_ne!(result.word_for_test(run), car.cons_word_for_test());
        });
        finish_owners(&era, [owner]);
        finish(era);
    });
}

#[test]
fn slots_of_another_owner_on_the_same_heap_cannot_be_inputs_or_destination() {
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut first = owner(&era, 1814, 3);
        let mut second = owner(&era, 1815, 3);
        let (other_car, other_cdr, mut other_result) = with_run(&mut second, |run| {
            (
                run.root_slot(&car).unwrap(),
                run.root_slot(&nil).unwrap(),
                run.root_slot(&car).unwrap(),
            )
        });
        let second_before = with_run(&mut second, snapshot);
        with_run(&mut first, |run| {
            let a = run.root_slot(&car).unwrap();
            let d = run.root_slot(&nil).unwrap();
            let mut result = run.root_slot(&car).unwrap();
            let before = snapshot(run);
            assert_eq!(
                run.cons_into(&other_car, &d, &mut result),
                Err(RegistrationError::WrongOwner)
            );
            assert_eq!(snapshot(run), before);
            assert_eq!(
                run.cons_into(&a, &other_cdr, &mut result),
                Err(RegistrationError::WrongOwner)
            );
            assert_eq!(snapshot(run), before);
            assert_eq!(
                run.cons_into(&a, &d, &mut other_result),
                Err(RegistrationError::WrongOwner)
            );
            assert_eq!(snapshot(run), before);
            run.cons_into(&a, &d, &mut result).unwrap();
        });
        assert_eq!(with_run(&mut second, snapshot), second_before);
        finish_owners(&era, [first, second]);
        finish(era);
    });
}

#[test]
fn dropped_and_forgotten_slot_handles_remain_roots_until_explicit_clear() {
    Context::with_test_collection_era(
        |context| {
            let graph = context
                .tagged_heap
                .alloc_cons(Value::fixnum(567), Value::NIL);
            let word = graph.bits();
            // SAFETY: fresh live setup graph from this serial heap.
            (
                unsafe { SharedRoot::new(&context.tagged_heap, graph) },
                word,
            )
        },
        |era, (source, word)| {
            era.start_cons_storage().unwrap();
            let mut owner = owner(&era, 1816, 2);
            with_run(&mut owner, |run| {
                drop(run.root_slot(&source).unwrap());
                std::mem::forget(run.root_slot(&source).unwrap());
                assert_eq!(run.state().root_scratch.len(), 2);
            });
            drop(source);
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped(&mut stopped).unwrap();
            era.with_stopped_heap_for_test(&stopped, |heap| {
                assert!(heap.owns_heap_value_for_test(Value::from_bits(word)));
            })
            .unwrap();
            complete(stopped);
            owner.leave_native().unwrap();
            with_run(&mut owner, |run| run.clear_root_slots().unwrap());
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            era.collect_stopped(&mut stopped).unwrap();
            era.with_stopped_heap_for_test(&stopped, |heap| {
                assert!(!heap.owns_heap_value_for_test(Value::from_bits(word)));
            })
            .unwrap();
            complete(stopped);
            owner.retire_rooted(&[]).unwrap();
            finish(era);
        },
    );
}

#[test]
fn exhausted_slot_generation_preserves_the_original_census_and_refuses_finish() {
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, 1817, 2);
        with_run(&mut owner, |run| {
            // Seed the real overflow boundary before handles are issued.
            run.state().root_slot_generation = u64::MAX;
            let a = run.root_slot(&car).unwrap();
            let d = run.root_slot(&nil).unwrap();
            let before = snapshot(run);
            assert_eq!(run.clear_root_slots(), Err(RegistrationError::Exhausted));
            assert_eq!(snapshot(run), before);
            assert_eq!(a.word_for_test(run), car.cons_word_for_test());
            assert_eq!(d.word_for_test(run), nil.cons_word_for_test());
        });
        owner.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        complete(stopped);
        owner.retire_rooted(&[]).unwrap();
        let failure = match era.finish() {
            Err(failure) => failure,
            Ok(_) => panic!("generation exhaustion must not discard the retained root census"),
        };
        assert_eq!(failure.reason, RegistrationError::PendingRoots);
        drop(failure); // Conservatively retain; no artificial generation repair.
    });
}

#[test]
fn mixed_slot_bridge_supports_shared_results_and_refuses_foreign_destinations_first() {
    let foreign_heap = super::super::TaggedHeap::new();
    // SAFETY: NIL is immediate; this carries a genuine different heap brand.
    let mut foreign = unsafe { SharedRoot::new(&foreign_heap, Value::NIL) };
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, 1818, 2);
        with_run(&mut owner, |run| {
            let a = run.root_slot(&car).unwrap();
            let mut result = run.root_slot(&nil).unwrap();
            let before = snapshot(run);
            assert!(matches!(
                run.root_slot(&foreign),
                Err(RegistrationError::WrongHeap)
            ));
            assert_eq!(snapshot(run), before);
            assert_eq!(
                run.cons_into(&a, &nil, &mut foreign),
                Err(RegistrationError::WrongHeap)
            );
            assert_eq!(snapshot(run), before);
            assert_eq!(foreign.cons_word_for_test(), Value::NIL.bits());
            run.cons_into(&car, &nil, &mut result).unwrap();
            let mut shared = nil.clone();
            run.cons_into(&result, &nil, &mut shared).unwrap();
            assert_ne!(shared.cons_word_for_test(), result.word_for_test(run));
            drop(shared);
        });
        finish_owners(&era, [owner]);
        finish(era);
    });
}

/// Rust-only TSan selector for the lasting local-slot consumer. Ready/go
/// witnesses two simultaneously live run loans; no mutex surrounds either
/// owner's independent cons initialization, and no SharedRoot result is made.
#[test]
fn two_owner_local_slot_allocator_stress() {
    use std::collections::HashSet;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;
    const PHASES: usize = 2;
    const PER_OWNER: usize = 4200;
    fn retry<T>(mut operation: impl FnMut() -> Result<T, RendezvousError>) -> T {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match operation() {
                Ok(value) => return value,
                Err(RendezvousError::Busy) if Instant::now() < deadline => thread::yield_now(),
                Err(error) => panic!("local-slot rendezvous: {error:?}"),
            }
        }
    }
    fn registration<T>(mut operation: impl FnMut() -> Result<T, RegistrationError>) -> T {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match operation() {
                Ok(value) => return value,
                Err(RegistrationError::Busy) if Instant::now() < deadline => thread::yield_now(),
                Err(error) => panic!("local-slot registration: {error:?}"),
            }
        }
    }
    fn run<'era, R>(
        owner: &mut Mutator<'era>,
        body: impl FnOnce(&mut RunAccess<'_, 'era>) -> R,
    ) -> R {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match owner.run_access() {
                Ok(mut access) => {
                    let result = body(&mut access);
                    access.close_cons_region().unwrap();
                    return result;
                }
                Err(RegistrationError::Busy) if Instant::now() < deadline => thread::yield_now(),
                Err(error) => panic!("local-slot run: {error:?}"),
            }
        }
    }
    fn receive<T>(receiver: &mpsc::Receiver<T>) -> T {
        receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("local-slot actor boundary")
    }
    struct Abort<'a>(&'a Registry);
    impl Drop for Abort<'_> {
        fn drop(&mut self) {
            if thread::panicking() {
                self.0.poison();
            }
        }
    }
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let heap = era.heap_identity();
        let permits = [
            era.register(1819, 3).unwrap(),
            era.register(1820, 3).unwrap(),
        ];
        let active = Arc::new(AtomicUsize::new(0));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        thread::scope(|scope| {
            let _abort = Abort(era.registry());
            let mut commands = Vec::new();
            for (index, mut permit) in permits.into_iter().enumerate() {
                let ready = ready_tx.clone();
                let results = result_tx.clone();
                let car = car.clone();
                let nil = nil.clone();
                let active = Arc::clone(&active);
                let (go_tx, go_rx) = mpsc::channel();
                commands.push(go_tx);
                scope.spawn(move || {
                    let deadline = Instant::now() + Duration::from_secs(30);
                    let mut owner = loop {
                        match permit.activate(heap, 1819 + index as u64) {
                            Ok(owner) => break owner,
                            Err(failure)
                                if failure.reason == RegistrationError::Busy
                                    && Instant::now() < deadline =>
                            {
                                permit = failure.into_permit();
                                thread::yield_now();
                            }
                            Err(failure) => panic!("local-slot activate: {:?}", failure.reason),
                        }
                    };
                    let (car_slot, mut tail, mut scratch) = run(&mut owner, |run| {
                        (
                            run.root_slot(&car).unwrap(),
                            run.root_slot(&nil).unwrap(),
                            run.root_slot(&nil).unwrap(),
                        )
                    });
                    drop(car);
                    drop(nil);
                    for phase in 0..PHASES {
                        let head = run(&mut owner, |run| {
                            active.fetch_add(1, Ordering::SeqCst);
                            ready.send((index, phase)).unwrap();
                            if go_rx.recv().is_err() {
                                active.fetch_sub(1, Ordering::SeqCst);
                                return None;
                            }
                            for sequence in 0..PER_OWNER {
                                run.cons_into(&car_slot, &tail, &mut scratch).unwrap();
                                std::mem::swap(&mut tail, &mut scratch);
                                if sequence % 31 == 0 {
                                    thread::yield_now();
                                }
                            }
                            active.fetch_sub(1, Ordering::SeqCst);
                            Some(tail.word_for_test(run))
                        });
                        let Some(head) = head else { return };
                        results.send((index, phase, owner.key(), head)).unwrap();
                        let epoch = retry(|| owner.wait_request());
                        retry(|| owner.acknowledge(epoch, &[]));
                        let mut image = owner.begin_restore_retry(false).unwrap();
                        let deadline = Instant::now() + Duration::from_secs(30);
                        loop {
                            match image.apply() {
                                Ok(()) => break,
                                Err(failure)
                                    if failure.reason == RendezvousError::Busy
                                        && Instant::now() < deadline =>
                                {
                                    image = failure.into_image();
                                    thread::yield_now();
                                }
                                Err(failure) => panic!("local-slot restore: {:?}", failure.reason),
                            }
                        }
                        run(&mut owner, |run| assert_eq!(tail.word_for_test(run), head));
                    }
                    run(&mut owner, |run| run.clear_root_slots().unwrap());
                    let deadline = Instant::now() + Duration::from_secs(30);
                    loop {
                        match owner.retire_rooted(&[]) {
                            Ok(()) => break,
                            Err(failure)
                                if failure.reason == RendezvousError::Busy
                                    && Instant::now() < deadline =>
                            {
                                owner = failure.into_owner();
                                thread::yield_now();
                            }
                            Err(failure) => panic!("local-slot retire: {:?}", failure.reason),
                        }
                    }
                });
            }
            drop(ready_tx);
            drop(result_tx);
            for phase in 0..PHASES {
                let ready = [receive(&ready_rx), receive(&ready_rx)];
                assert_eq!((ready[0].1, ready[1].1), (phase, phase));
                assert_ne!(ready[0].0, ready[1].0);
                assert_eq!(active.load(Ordering::SeqCst), 2);
                for command in &commands {
                    command.send(()).unwrap();
                }
                let responses = [receive(&result_rx), receive(&result_rx)];
                assert_ne!(responses[0].0, responses[1].0);
                retry(|| era.rendezvous().request());
                let mut stopped = retry(|| era.stop());
                registration(|| era.return_cons_storage(&stopped));
                for &(_, observed_phase, key, _) in &responses {
                    assert_eq!(observed_phase, phase);
                    retry(|| {
                        stopped.with_owner_image(key, |image| {
                            assert_eq!(image.summary().root_words, 3)
                        })
                    });
                    retry(|| {
                        stopped.with_root_reader(key, |reader| {
                            let mut external = Vec::new();
                            reader.append_words(&mut external);
                            assert!(external.is_empty());
                            reader.finish().unwrap();
                        })
                    });
                }
                assert!(retry(|| era.collect_stopped(&mut stopped)).major);
                retry(|| {
                    era.with_stopped_heap_for_test(&stopped, |heap| {
                        let mut addresses = HashSet::new();
                        for &(_, _, _, head) in &responses {
                            let mut cursor = Value::from_bits(head);
                            for _ in 0..(phase + 1) * PER_OWNER {
                                assert!(heap.owns_heap_value_for_test(cursor));
                                assert!(
                                    addresses.insert(cursor.bits()),
                                    "overlapping owner chains"
                                );
                                // SAFETY: stopped exact images retain both complete
                                // chains; no owner has run or cell-write authority.
                                let cell = unsafe { &*cursor.as_cons_ptr().unwrap() };
                                assert_eq!(cell.car.as_fixnum(), Some(8123));
                                // SAFETY: the same stopped, fully initialized cell remains live.
                                cursor = unsafe { cell.cdr() };
                            }
                            assert!(cursor.is_nil());
                        }
                        assert_eq!(addresses.len(), 2 * (phase + 1) * PER_OWNER);
                    })
                });
                if phase + 1 < PHASES {
                    registration(|| era.resume_cons_storage(&stopped));
                }
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    match stopped.complete() {
                        Ok(()) => break,
                        Err(failure)
                            if failure.reason == RendezvousError::Busy
                                && Instant::now() < deadline =>
                        {
                            stopped = failure.into_stopped();
                            thread::yield_now();
                        }
                        Err(failure) => panic!("local-slot complete: {:?}", failure.reason),
                    }
                }
            }
        });
        finish(era);
    });
}

#[test]
fn cloned_operand_alias_loads_before_in_place_result_and_is_stale_after_clear() {
    Context::with_test_collection_era(seeds, |era, (car, nil)| {
        era.start_cons_storage().unwrap();
        let mut owner = owner(&era, 1821, 2);
        let (a, mut head) = with_run(&mut owner, |run| {
            (run.root_slot(&car).unwrap(), run.root_slot(&nil).unwrap())
        });
        let mut alias = head.clone();
        let word = with_run(&mut owner, |run| {
            let capacity = run.state().root_scratch.capacity();
            for _ in 0..260 {
                run.cons_into(&a, &alias, &mut head).unwrap();
            }
            assert_eq!(run.state().root_scratch.len(), 2);
            assert_eq!(run.state().root_scratch.capacity(), capacity);
            head.word_for_test(run)
        });
        owner.enter_native(&[]).unwrap();
        let mut stopped = era.stop().unwrap();
        era.return_cons_storage(&stopped).unwrap();
        era.collect_stopped(&mut stopped).unwrap();
        era.with_stopped_heap_for_test(&stopped, |heap| {
            let mut cursor = Value::from_bits(word);
            for _ in 0..260 {
                assert!(heap.owns_heap_value_for_test(cursor));
                // SAFETY: exact stopped authority just traced this initialized
                // chain from the one canonical in-place output home. The only
                // retained SharedRoots are untraced fixnum and NIL seeds.
                unsafe {
                    let cell = &*cursor.as_cons_ptr().unwrap();
                    assert_eq!(cell.car.as_fixnum(), Some(8123));
                    cursor = cell.cdr();
                }
            }
            assert!(cursor.is_nil(), "aliasing must not create a self-cycle");
        })
        .unwrap();
        era.resume_cons_storage(&stopped).unwrap();
        complete(stopped);
        owner.leave_native().unwrap();
        with_run(&mut owner, |run| {
            assert_eq!(alias.word_for_test(run), word);
            run.clear_root_slots().unwrap();
            let fresh_car = run.root_slot(&car).unwrap();
            let mut fresh_result = run.root_slot(&nil).unwrap();
            let before = snapshot(run);
            assert_eq!(
                run.cons_into(&alias, &nil, &mut fresh_result),
                Err(RegistrationError::StaleRegistration)
            );
            assert_eq!(snapshot(run), before);
            assert_eq!(
                run.cons_into(&fresh_car, &alias, &mut fresh_result),
                Err(RegistrationError::StaleRegistration)
            );
            assert_eq!(snapshot(run), before);
            assert_eq!(
                run.cons_into(&fresh_car, &nil, &mut alias),
                Err(RegistrationError::StaleRegistration)
            );
            assert_eq!(snapshot(run), before);
        });
        finish_owners(&era, [owner]);
        finish(era);
    });
}
