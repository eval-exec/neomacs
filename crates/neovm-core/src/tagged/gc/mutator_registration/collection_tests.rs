//! Real STW collection through the canonical synthetic-owner protocol.
#![cfg(not(feature = "gc-memory-telemetry"))]
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::Duration;

fn retry<T>(mut f: impl FnMut() -> Result<T, RendezvousError>) -> T {
    loop {
        match f() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) => thread::yield_now(),
            Err(error) => panic!("unexpected collection refusal: {error:?}"),
        }
    }
}
fn activate<'era>(
    mut permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    owner: u64,
) -> Mutator<'era> {
    loop {
        match permit.activate(heap, owner) {
            Ok(mutator) => return mutator,
            Err(failure) if failure.reason == RegistrationError::Busy => {
                permit = failure.into_permit();
                thread::yield_now();
            }
            Err(failure) => panic!("activation: {:?}", failure.reason),
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
            Err(failure) => panic!("restore: {:?}", failure.reason),
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
            Err(failure) => panic!("retire: {:?}", failure.reason),
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
            Err(failure) => panic!("complete: {:?}", failure.reason),
        }
    }
}
fn receive<T>(rx: &mpsc::Receiver<T>) -> T {
    rx.recv_timeout(Duration::from_secs(30))
        .expect("actor reaches collection boundary")
}
fn graph(context: &mut Context, n: i64) -> (SharedRoot, [usize; 2], MarkWord) {
    let child = context.tagged_heap.alloc_cons(Value::fixnum(n), Value::NIL);
    let head = context.tagged_heap.alloc_cons(child, Value::NIL);
    // SAFETY: setup is the sole serial owner; no collection or callback occurs
    // between allocation and admission to the matching root table.
    let root = unsafe { SharedRoot::new(&context.tagged_heap, head) };
    (root, [head.bits(), child.bits()], MarkWord::of(head))
}
fn assert_owned(era: &ParallelHeapEra<'_>, stop: &AllStopped<'_>, words: &[usize], expected: bool) {
    retry(|| {
        era.with_stopped_heap_for_test(stop, |heap| {
            for &word in words {
                assert_eq!(
                    heap.owns_heap_value_for_test(Value::from_bits(word)),
                    expected,
                    "ownership of opaque observation {word:#x}"
                );
            }
        })
    });
}

#[test]
fn two_requesters_collect_transitive_world_private_and_image_roots() {
    Context::with_test_collection_era(
        |context| {
            let a = graph(context, 101);
            let b = graph(context, 102);
            let world = graph(context, 103);
            context.set_variable("p1rv-a5-world-root", Value::from_bits(world.1[0]));
            drop(world.0);
            let legacy = graph(context, 104);
            context.lexenv = Value::from_bits(legacy.1[0]);
            drop(legacy.0);
            let logs = [
                graph(context, 105),
                graph(context, 106),
                graph(context, 107),
            ];
            let shared_only = graph(context, 108);
            let internal = context
                .tagged_heap
                .alloc_buffer(crate::buffer::BufferId(900001));
            context.tagged_heap.buffer_registry.push(Some(internal));
            let dead = context
                .tagged_heap
                .alloc_cons(Value::fixnum(999), Value::NIL)
                .bits();
            (
                a,
                b,
                world.1,
                legacy.1,
                logs,
                dead,
                shared_only,
                internal.bits(),
                context.tagged_heap.generational_enabled(),
            )
        },
        |era, (a, b, world, legacy, logs, dead, shared_only, internal, generational)| {
            let heap = era.heap_identity();
            let rv = era.rendezvous();
            let permits = [era.register(801, 1).unwrap(), era.register(802, 1).unwrap()];
            let barrier = Arc::new(Barrier::new(3));
            let (tx, rx) = mpsc::channel();
            let observations = [a.1, b.1];
            thread::scope(|scope| {
                for (index, (permit, root)) in permits.into_iter().zip([a.0, b.0]).enumerate() {
                    let barrier = Arc::clone(&barrier);
                    let tx = tx.clone();
                    scope.spawn(move || {
                        let mut owner = activate(permit, heap, 801 + index as u64);
                        loop {
                            match owner.run_access() {
                                Ok(mut run) => {
                                    let hot = &mut run.state().hot;
                                    hot.bytes_since_gc = 64 + index;
                                    if generational {
                                        hot.pacing.promoted_since_major = 32;
                                        hot.pacing.minors_since_major = 4;
                                        hot.pacing.stress_cycles_since_major = 3;
                                    }
                                    break;
                                }
                                Err(RegistrationError::Busy) => thread::yield_now(),
                                Err(error) => panic!("initial owner access: {error:?}"),
                            }
                        }
                        barrier.wait();
                        let request = retry(|| rv.request());
                        let Request::Active(epoch) = request else {
                            panic!("unexpected debt")
                        };
                        retry(|| owner.acknowledge(epoch, std::slice::from_ref(&root)));
                        // Only the batch publication roots this graph during mark.
                        drop(root);
                        tx.send((owner.key(), epoch)).unwrap();
                        let response = owner.begin_restore_retry(false).unwrap();
                        apply(response);
                        loop {
                            match owner.run_access() {
                                Ok(mut run) => {
                                    assert_eq!(run.state().hot.bytes_since_gc, 0);
                                    assert_eq!(run.state().hot.pacing.promoted_since_major, 0);
                                    assert_eq!(run.state().hot.pacing.minors_since_major, 0);
                                    assert_eq!(run.state().hot.pacing.stress_cycles_since_major, 0);
                                    assert_eq!(
                                        run.state().hot.bytes_banked_at_resets,
                                        (64 + index) as u64
                                    );
                                    run.state().root_scratch.clear();
                                    break;
                                }
                                Err(RegistrationError::Busy) => thread::yield_now(),
                                Err(error) => panic!("run after collection: {error:?}"),
                            }
                        }
                        retire(owner);
                    });
                }
                barrier.wait();
                let first = receive(&rx);
                let second = receive(&rx);
                assert_eq!(first.1, second.1, "requesters coalesce one epoch");
                let mut stopped = retry(|| era.stop());
                let [root, remembered, cons_write] = logs;
                retry(|| {
                    stopped.with_owner_image(first.0, |image| {
                        image.seed_collection_words_for_test(root.2, remembered.2, cons_write.2);
                    })
                });
                let log_words = [root.1, remembered.1, cons_write.1].concat();
                drop((root.0, remembered.0, cons_write.0));
                let report = retry(|| era.collect_stopped(&mut stopped));
                assert!(report.major);
                assert_eq!(report.epoch, first.1.get());
                assert_eq!(report.frozen_owners, 2);
                for words in observations.iter().chain([&world, &legacy]) {
                    assert_owned(&era, &stopped, words, true);
                }
                assert_owned(&era, &stopped, &log_words, true);
                assert_owned(&era, &stopped, &shared_only.1, true);
                assert_owned(&era, &stopped, &[internal], true);
                assert_owned(&era, &stopped, &[dead], false);
                complete(stopped);
            });
            drop(shared_only.0);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}

#[test]
fn native_image_logs_fold_once_across_repeated_full_majors() {
    Context::with_test_collection_era(
        |context| (graph(context, 201), graph(context, 202)),
        |era, (graph, remembered)| {
            let mut owner = era
                .register(803, 1)
                .unwrap()
                .activate(era.heap_identity(), 803)
                .unwrap();
            let key = owner.key();
            owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
            drop(graph.0);
            let mut previous_cycle = None;
            let remembered_observations = remembered.1;
            let mut remembered = Some(remembered);
            for round in 0..2 {
                let mut stopped = retry(|| era.stop());
                let work_words = if let Some((root, words, word)) = remembered.take() {
                    stopped
                        .with_owner_image(key, |image| {
                            image.seed_collection_words_for_test(
                                MarkWord::of(Value::NIL),
                                word,
                                MarkWord::of(Value::NIL),
                            );
                        })
                        .unwrap();
                    drop(root);
                    words
                } else {
                    [0; 2]
                };
                let report = retry(|| era.collect_stopped(&mut stopped));
                assert!(report.major);
                if let Some(previous) = previous_cycle {
                    assert_eq!(report.cycle, previous + 1);
                }
                previous_cycle = Some(report.cycle);
                assert_owned(&era, &stopped, &graph.1, true);
                if round == 0 {
                    assert_owned(&era, &stopped, &work_words, true);
                } else {
                    assert_owned(&era, &stopped, &remembered_observations, false);
                }
                stopped
                    .with_owner_image(key, |image| {
                        let facts = image.summary();
                        assert_eq!(facts.remembered_words, 0);
                        assert_eq!(facts.cons_writes, 0);
                        assert_eq!(facts.bytes_since_gc, 0);
                        assert_eq!(facts.promoted_since_major, 0);
                        assert_eq!(facts.minors_since_major, 0);
                        assert_eq!(facts.stress_cycles_since_major, 0);
                        assert!(image.young_empty());
                    })
                    .unwrap();
                complete(stopped);
            }
            owner.leave_native().unwrap();
            {
                let mut run = owner.run_access().unwrap();
                run.state().root_scratch.clear();
            }
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}

#[test]
fn foreign_stop_guard_and_missing_root_admission_refuse_before_collection() {
    Context::with_test_collection_era(
        |_| (),
        |era, ()| {
            Context::with_test_collection_era(
                |_| (),
                |foreign, ()| {
                    let mut stop = foreign.stop().unwrap();
                    assert_eq!(
                        era.collect_stopped(&mut stop).unwrap_err(),
                        RendezvousError::Identity
                    );
                    assert_eq!(stop.epoch().get(), 1);
                    stop.trace(|_, _| Ok(())).unwrap();
                    complete(stop);
                    let (collector, keeper) = foreign.finish().unwrap();
                    drop(collector);
                    drop(keeper);
                },
            );
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
    Context::with_test_mutator_era(|era| {
        let mut stopped = era.stop().unwrap();
        assert_eq!(
            era.collect_stopped(&mut stopped).unwrap_err(),
            RendezvousError::Publication
        );
        stopped.trace(|_, _| Ok(())).unwrap();
        complete(stopped);
        let (collector, keeper) = era.finish().unwrap();
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn retained_reader_refuses_collection_and_abort_keeps_backing() {
    Context::with_test_collection_era(
        |context| graph(context, 301),
        |era, graph| {
            let mut owner = era
                .register(804, 1)
                .unwrap()
                .activate(era.heap_identity(), 804)
                .unwrap();
            let key = owner.key();
            owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
            drop(graph.0);
            let mut stopped = era.stop().unwrap();
            stopped
                .with_root_reader(key, |reader| std::mem::forget(reader))
                .unwrap();
            assert!(matches!(
                era.shutdown_admission_stopped_for_test(&stopped).unwrap(),
                Err(crate::tagged::gc::MarkFinishError::FacadeRetained(_))
            ));
            assert_eq!(
                era.collect_stopped(&mut stopped).unwrap_err(),
                RendezvousError::Readers
            );
            assert_owned(&era, &stopped, &graph.1, true);
            drop(stopped);
            drop(owner);
            assert_eq!(
                era.finish().unwrap_err().reason,
                RegistrationError::Poisoned
            );
        },
    );
}

#[test]
fn explicitly_finished_reader_allows_real_collection() {
    Context::with_test_collection_era(
        |context| graph(context, 302),
        |era, graph| {
            let mut owner = era
                .register(805, 1)
                .unwrap()
                .activate(era.heap_identity(), 805)
                .unwrap();
            owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
            drop(graph.0);
            let mut stopped = era.stop().unwrap();
            stopped
                .with_root_reader(owner.key(), |reader| reader.finish().unwrap())
                .unwrap();
            assert!(era.collect_stopped(&mut stopped).unwrap().major);
            assert_owned(&era, &stopped, &graph.1, true);
            complete(stopped);
            owner.leave_native().unwrap();
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}

#[test]
fn repeated_worlds_have_distinct_heaps_and_independent_collection_cycles() {
    let mut identities = std::collections::HashSet::new();
    for round in 0..8 {
        Context::with_test_collection_era(
            |context| graph(context, 400 + round),
            |era, graph| {
                assert!(identities.insert(era.heap_identity()));
                let mut owner = era
                    .register(806, 1)
                    .unwrap()
                    .activate(era.heap_identity(), 806)
                    .unwrap();
                owner.enter_native(std::slice::from_ref(&graph.0)).unwrap();
                drop(graph.0);
                let mut stopped = era.stop().unwrap();
                let report = era.collect_stopped(&mut stopped).unwrap();
                assert_eq!(report.epoch, 1);
                assert!(report.major);
                assert_owned(&era, &stopped, &graph.1, true);
                complete(stopped);
                owner.leave_native().unwrap();
                retire(owner);
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
            },
        );
    }
}

#[test]
fn serial_heap_restoration_reclaims_former_owner_roots_after_resume() {
    Context::with_test_collection_era_then(
        |context| {
            let participant = graph(context, 501);
            let world = graph(context, 502);
            context.set_variable("p1rv-a5-persistent-world", Value::from_bits(world.1[0]));
            drop(world.0);
            let legacy = graph(context, 503);
            context.lexenv = Value::from_bits(legacy.1[0]);
            drop(legacy.0);
            (participant, world.1, legacy.1)
        },
        |era, (participant, world, legacy)| {
            let mut owner = era
                .register(807, 1)
                .unwrap()
                .activate(era.heap_identity(), 807)
                .unwrap();
            owner
                .enter_native(std::slice::from_ref(&participant.0))
                .unwrap();
            drop(participant.0);
            let mut stopped = era.stop().unwrap();
            era.collect_stopped(&mut stopped).unwrap();
            assert_owned(&era, &stopped, &participant.1, true);
            complete(stopped);
            owner.leave_native().unwrap();
            retire(owner);
            let (collector, keeper) = era.finish().unwrap();
            (collector, keeper, (participant.1, world, legacy))
        },
        |context, (participant, world, legacy)| {
            assert!(!context.tagged_heap.facade_mark_is_excluded());
            context.gc_collect_exact();
            for word in participant {
                assert!(
                    !context
                        .tagged_heap
                        .owns_heap_value_for_test(Value::from_bits(word))
                );
            }
            for word in world.into_iter().chain(legacy) {
                let value = Value::from_bits(word);
                assert!(context.tagged_heap.owns_heap_value_for_test(value));
            }
        },
    );
}
