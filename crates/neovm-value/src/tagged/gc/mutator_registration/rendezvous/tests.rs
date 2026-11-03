#![forbid(unsafe_code)]

use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::ProofEra as ParallelHeapEra;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::value::TaggedValue;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

#[path = "failure_tests.rs"]
mod failure;
#[path = "mapping_tests.rs"]
mod mapping;
#[path = "self_wait_tests.rs"]
mod self_wait;

fn receive<T>(receiver: &Receiver<T>) -> T {
    receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("fixture actor must reach the requested protocol boundary")
}

fn active(request: Request) -> NonZeroU64 {
    match request {
        Request::Active(epoch) => epoch,
        Request::Queued => panic!("fixture expected active request"),
    }
}

fn retry<T>(mut operation: impl FnMut() -> Result<T, RendezvousError>) -> T {
    loop {
        match operation() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) => thread::yield_now(),
            Err(error) => panic!("unexpected rendezvous refusal: {error:?}"),
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
            Err(failure) => panic!("unexpected activation refusal: {:?}", failure.reason),
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
            Err(failure) => panic!("unexpected resume refusal: {:?}", failure.reason),
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
            Err(failure) => panic!("unexpected completion refusal: {:?}", failure.reason),
        }
    }
}

fn retire(mut owner: Mutator<'_>, roots: &[SharedRoot]) {
    loop {
        match owner.retire_rooted(roots) {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy => {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("unexpected retirement refusal: {:?}", failure.reason),
        }
    }
}

fn park(owner: &mut Mutator<'_>, roots: &[SharedRoot]) -> u64 {
    let epoch = retry(|| owner.wait_request());
    retry(|| owner.acknowledge(epoch, roots));
    let image = owner.begin_restore_retry(false).unwrap();
    let returned = image.epoch();
    apply(image);
    returned
}

// Observers obey the same gate-before-entry order as mutating owners. An
// ungated test-only inspector can otherwise manufacture a post-restore Busy.
fn inspect_entry<R>(
    rv: Rendezvous<'_>,
    entry: &MutatorEntry,
    mut inspect: impl FnMut(&EntryState) -> R,
) -> R {
    retry(|| {
        let _gate = rv.registry.lock()?;
        let state = entry_state(entry)?;
        Ok(inspect(&state))
    })
}

fn seed(owner: &mut Mutator<'_>, marker: i64) {
    loop {
        match owner.run_access() {
            Ok(mut run) => {
                let local = run.state();
                local.hot.remset.push(TaggedValue::fixnum(marker));
                local.hot.bytes_since_gc = marker as usize;
                break;
            }
            Err(RegistrationError::Busy) => thread::yield_now(),
            Err(error) => panic!("unexpected synthetic run refusal: {error:?}"),
        }
    }
}

fn read_words(stopped: &AllStopped<'_>, key: MutatorKey) -> Vec<usize> {
    retry(|| {
        stopped.with_root_reader(key, |reader| {
            let mut words = Vec::new();
            reader.append_words(&mut words);
            reader.finish().unwrap();
            words
        })
    })
}

fn trace_owners(stopped: &mut AllStopped<'_>, keys: &[MutatorKey]) -> Vec<Vec<usize>> {
    retry(|| {
        stopped.trace(|guard, _proof| {
            let mut result = Vec::new();
            for key in keys {
                let words = read_words(guard, *key);
                assert_eq!(words.len(), 1);
                assert_ne!(words[0], 0);
                result.push(words);
                retry(|| {
                    guard.with_owner_image(*key, |image| {
                        assert!(image.roots_empty());
                        assert!(image.young_empty());
                    })
                });
            }
            if result.len() == 2 {
                assert_ne!(result[0], result[1], "each owner published its own root");
            }
            Ok(result)
        })
    })
}

fn finish_era(era: ParallelHeapEra<'_>) {
    let (collector, keeper) = era.finish().unwrap();
    drop(collector);
    drop(keeper);
}

#[test]
fn two_owners_pause_before_ack_and_while_restoring_across_two_epochs() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let heap = era.heap_identity();
        let permits = [era.register(601, 1).unwrap(), era.register(602, 1).unwrap()];
        let keys = permits.each_ref().map(|permit| permit.key());
        let rendezvous = era.rendezvous();
        thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let (publishing_tx, publishing_rx) = mpsc::channel();
            let (acked_tx, acked_rx) = mpsc::channel();
            let (resuming_tx, resuming_rx) = mpsc::channel();
            let (running_tx, running_rx) = mpsc::channel();
            let mut prepare = Vec::new();
            let mut release_publication = Vec::new();
            let mut restore = Vec::new();
            let mut apply_releases = Vec::new();
            let mut handles = Vec::new();
            for (index, permit) in permits.into_iter().enumerate() {
                let input = vec![roots[index].clone()];
                let ready = ready_tx.clone();
                let publishing = publishing_tx.clone();
                let acked = acked_tx.clone();
                let resuming = resuming_tx.clone();
                let running = running_tx.clone();
                let (prepare_tx, prepare_rx) = mpsc::channel();
                let (publication_tx, publication_rx) = mpsc::channel();
                let (restore_tx, restore_rx) = mpsc::channel();
                let (apply_tx, apply_rx) = mpsc::channel();
                prepare.push(prepare_tx);
                release_publication.push(publication_tx);
                restore.push(restore_tx);
                apply_releases.push(apply_tx);
                handles.push(scope.spawn(move || {
                    let owner_id = 601 + index as u64;
                    let mut owner = activate(permit, heap, owner_id);
                    let publication_rx = std::sync::Arc::new(std::sync::Mutex::new(publication_rx));
                    seed(&mut owner, owner_id as i64);
                    ready.send(index).unwrap();
                    for round in 1..=2 {
                        receive(&prepare_rx);
                        let epoch = retry(|| owner.wait_request());
                        assert_eq!(epoch.get(), round);
                        let publishing = publishing.clone();
                        let publication_rx = std::sync::Arc::clone(&publication_rx);
                        // Hook runs after Publishing is visible and the gate
                        // is released. Store a fresh one-shot closure each epoch.
                        owner.protocol.publication_hook = Some(Box::new(move || {
                            publishing.send(index).unwrap();
                            receive(&publication_rx.lock().unwrap());
                        }));
                        retry(|| owner.acknowledge(epoch, &input));
                        acked.send(index).unwrap();
                        receive(&restore_rx);
                        let image = owner.begin_restore_retry(false).unwrap();
                        assert_eq!(image.epoch(), round);
                        resuming.send(index).unwrap();
                        receive(&apply_rx);
                        apply(image);
                        retry(|| {
                            owner
                                .run_access()
                                .map(|_| ())
                                .map_err(RendezvousError::from)
                        });
                        running.send(index).unwrap();
                    }
                    retire(owner, &input);
                }));
            }
            for _ in 0..2 {
                receive(&ready_rx);
            }
            let mut first_roots = None;
            for round in 1..=2 {
                let epoch = active(retry(|| rendezvous.request()));
                assert_eq!(epoch.get(), round);
                assert_eq!(active(retry(|| rendezvous.request())), epoch);
                assert!(era.try_stop().unwrap().is_none());
                for sender in &prepare {
                    sender.send(()).unwrap();
                }
                for _ in 0..2 {
                    receive(&publishing_rx);
                }
                let snapshot = retry(|| rendezvous.snapshot());
                assert!(
                    snapshot
                        .owners
                        .iter()
                        .all(|(_, mode)| *mode == Mode::Publishing)
                );
                assert!(era.try_stop().unwrap().is_none());
                for _ in 0..4 {
                    rendezvous.notify();
                    assert!(era.try_stop().unwrap().is_none());
                }
                for sender in &release_publication {
                    sender.send(()).unwrap();
                }
                for _ in 0..2 {
                    receive(&acked_rx);
                }
                let mut stopped = retry(|| era.stop());
                assert_eq!(stopped.epoch(), epoch);
                assert!(matches!(era.try_stop(), Err(RendezvousError::Busy)));
                let words = trace_owners(&mut stopped, &keys);
                if let Some(expected) = &first_roots {
                    assert_eq!(&words, expected);
                } else {
                    first_roots = Some(words);
                }
                complete(stopped);
                assert_eq!(retry(|| rendezvous.snapshot()).debt.len(), 2);
                if round == 1 {
                    assert_eq!(rendezvous.request(), Ok(Request::Queued));
                }
                for sender in &restore {
                    sender.send(()).unwrap();
                }
                for _ in 0..2 {
                    receive(&resuming_rx);
                }
                let snapshot = retry(|| rendezvous.snapshot());
                assert_eq!(snapshot.phase, Phase::DrainingResumes);
                assert!(
                    snapshot
                        .owners
                        .iter()
                        .all(|(_, mode)| *mode == Mode::Resuming)
                );
                assert_eq!(snapshot.request_epoch, round);
                assert!(era.try_stop().unwrap().is_none());
                for sender in &apply_releases {
                    sender.send(()).unwrap();
                }
                for _ in 0..2 {
                    receive(&running_rx);
                }
            }
            for handle in handles {
                handle.join().unwrap();
            }
            assert_eq!(retry(|| rendezvous.snapshot()).completed_epoch, 2);
        });
        finish_era(era);
    });
}

#[test]
fn stale_ack_identity_generation_duplicate_and_missing_image_are_refused() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(611, 1)
            .unwrap()
            .activate(era.heap_identity(), 611)
            .unwrap();
        let key = owner.key();
        let entry = owner.entry();
        let rv = era.rendezvous();
        let epoch = active(rv.request().unwrap());
        let stale = NonZeroU64::new(epoch.get() + 1).unwrap();
        assert_eq!(
            owner.acknowledge(stale, &roots[..1]),
            Err(RendezvousError::Publication)
        );
        assert_eq!(entry.state.lock().unwrap().rendezvous.mode, Mode::Running);
        assert!(era.try_stop().unwrap().is_none());
        owner.acknowledge(epoch, &roots[..1]).unwrap();
        assert_eq!(
            owner.acknowledge(epoch, &roots[..1]),
            Err(RendezvousError::Busy)
        );
        {
            let mut state = entry.state.lock().unwrap();
            state.publication_generation += 1;
        }
        assert!(matches!(era.try_stop(), Err(RendezvousError::Identity)));
        entry.state.lock().unwrap().publication_generation -= 1;
        entry.state.lock().unwrap().rendezvous.coverage_epoch = 0;
        assert!(matches!(era.try_stop(), Err(RendezvousError::Publication)));
        entry.state.lock().unwrap().rendezvous.coverage_epoch = epoch.get();
        let saved_image = entry.state.lock().unwrap().image.take().unwrap();
        assert!(matches!(era.try_stop(), Err(RendezvousError::Publication)));
        entry.state.lock().unwrap().image = Some(saved_image);
        let saved_roots = entry.state.lock().unwrap().roots.take().unwrap();
        assert!(matches!(era.try_stop(), Err(RendezvousError::Publication)));
        entry.state.lock().unwrap().roots = Some(saved_roots);
        let mut stopped = retry(|| era.stop());
        let mut wrong = key;
        wrong.generation = NonZeroU64::new(key.generation.get() + 1).unwrap();
        assert!(matches!(
            stopped.with_owner_image(wrong, |_| ()),
            Err(RendezvousError::Identity)
        ));
        assert!(matches!(
            stopped.with_root_reader(wrong, |_reader| panic!("stale reader admitted")),
            Err(RendezvousError::Identity)
        ));
        trace_owners(&mut stopped, &[key]);
        assert_eq!(
            stopped.trace(|_, _| Ok(())),
            Err(RendezvousError::Publication)
        );
        complete(stopped);
        owner.begin_restore(false).unwrap().apply().unwrap();
        assert!(matches!(
            owner.begin_restore(false),
            Err(RendezvousError::Busy)
        ));
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

#[test]
fn live_worker_loan_blocks_ack_and_preserves_running_owner() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(612, 1)
            .unwrap()
            .activate(era.heap_identity(), 612)
            .unwrap();
        let key = owner.key();
        let loan = owner.take_young_loan().unwrap();
        let epoch = active(era.rendezvous().request().unwrap());
        assert_eq!(
            owner.acknowledge(epoch, &roots[..1]),
            Err(RendezvousError::Loan)
        );
        assert!(era.try_stop().unwrap().is_none());
        assert_eq!(
            owner.entry().state.lock().unwrap().rendezvous.mode,
            Mode::Running
        );
        owner.return_young_loan(loan).unwrap();
        owner.acknowledge(epoch, &roots[..1]).unwrap();
        let mut stopped = retry(|| era.stop());
        trace_owners(&mut stopped, &[key]);
        complete(stopped);
        owner.begin_restore(false).unwrap().apply().unwrap();
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

#[test]
fn native_owner_sleeps_two_epochs_latest_image_and_synthetic_work_once() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let heap = era.heap_identity();
        let native = era.register(621, 1).unwrap();
        let ordinary = era.register(622, 1).unwrap();
        let native_key = native.key();
        let ordinary_key = ordinary.key();
        let rv = era.rendezvous();
        thread::scope(|scope| {
            let (native_ready_tx, native_ready_rx) = mpsc::channel();
            let (ordinary_ready_tx, ordinary_ready_rx) = mpsc::channel();
            let (wake_native, native_sleep) = mpsc::channel();
            let native_roots = vec![roots[0].clone()];
            let ordinary_roots = vec![roots[1].clone()];
            let native = scope.spawn(move || {
                let mut owner = activate(native, heap, 621);
                seed(&mut owner, 621);
                retry(|| owner.enter_native(&native_roots));
                native_ready_tx.send(owner.entry()).unwrap();
                receive(&native_sleep); // host-only; no GC poll or notification
                let image = owner.begin_restore_retry(true).unwrap();
                assert_eq!(image.epoch(), 2);
                apply(image);
                assert_eq!(
                    inspect_entry(rv, &owner.entry(), |state| state.rendezvous.image_takes),
                    1
                );
                retire(owner, &native_roots);
            });
            let ordinary = scope.spawn(move || {
                let mut owner = activate(ordinary, heap, 622);
                ordinary_ready_tx.send(()).unwrap();
                for epoch in 1..=2 {
                    assert_eq!(park(&mut owner, &ordinary_roots), epoch);
                }
                retire(owner, &ordinary_roots);
            });
            let native_entry = receive(&native_ready_rx);
            receive(&ordinary_ready_rx);
            let mut folded = 0;
            let mut first_words = None;
            for epoch in 1..=2 {
                let mut stopped = retry(|| era.stop());
                assert_eq!(stopped.epoch().get(), epoch);
                retry(|| {
                    stopped.trace(|guard, _proof| {
                        let words = read_words(guard, native_key);
                        if let Some(previous) = &first_words {
                            assert_eq!(&words, previous);
                        } else {
                            first_words = Some(words);
                        }
                        folded += retry(|| {
                            guard.with_owner_image(native_key, |image| {
                                image
                                    .consume_synthetic_remembered_for_test(MarkWord::of(
                                        TaggedValue::fixnum(621),
                                    ))
                                    .unwrap()
                            })
                        });
                        assert_eq!(read_words(guard, ordinary_key).len(), 1);
                        Ok(())
                    })
                });
                complete(stopped);
                assert_eq!(
                    inspect_entry(rv, &native_entry, |state| state.rendezvous.image_epoch),
                    epoch
                );
                assert_eq!(
                    inspect_entry(rv, &native_entry, |state| state.rendezvous.image_takes),
                    0
                );
            }
            assert_eq!(folded, 1);
            wake_native.send(()).unwrap();
            native.join().unwrap();
            ordinary.join().unwrap();
            assert_eq!(retry(|| rv.snapshot()).traces, 2);
        });
        finish_era(era);
    });
}

#[test]
fn native_idle_restore_debt_queues_request_before_any_new_epoch() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(631, 1)
            .unwrap()
            .activate(era.heap_identity(), 631)
            .unwrap();
        let key = owner.key();
        let entry = owner.entry();
        owner.enter_native(&roots[..1]).unwrap();
        let rv = era.rendezvous();
        let response = owner.begin_restore(true).unwrap();
        assert_eq!(response.epoch(), 0);
        assert_eq!(retry(|| rv.snapshot()).owners, vec![(key, Mode::Resuming)]);
        assert_eq!(rv.request(), Ok(Request::Queued));
        assert_eq!(retry(|| rv.snapshot()).request_epoch, 0);
        response.apply().unwrap();
        assert_eq!(entry.state.lock().unwrap().rendezvous.image_takes, 1);
        let epoch = active(rv.request().unwrap());
        assert_eq!(epoch.get(), 1);
        owner.acknowledge(epoch, &roots[..1]).unwrap();
        let mut stopped = retry(|| era.stop());
        trace_owners(&mut stopped, &[key]);
        complete(stopped);
        owner.begin_restore(false).unwrap().apply().unwrap();
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

#[test]
fn native_return_waits_for_active_stop_before_restoration() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(632, 1).unwrap();
        let key = permit.key();
        let input = vec![roots[0].clone()];
        let rv = era.rendezvous();
        thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let (return_tx, return_rx) = mpsc::channel();
            let (waiting_tx, waiting_rx) = mpsc::channel();
            let (returned_tx, returned_rx) = mpsc::channel();
            let worker = scope.spawn(move || {
                let mut owner = activate(permit, heap, 632);
                retry(|| owner.enter_native(&input));
                ready_tx.send(()).unwrap();
                receive(&return_rx);
                owner.protocol.restore_wait_hook = Some(Box::new(move || {
                    waiting_tx.send(()).unwrap();
                }));
                let image = owner.begin_restore_retry(true).unwrap();
                assert_eq!(image.epoch(), 1);
                apply(image);
                returned_tx.send(()).unwrap();
                retire(owner, &input);
            });
            receive(&ready_rx);
            let mut stopped = retry(|| era.stop());
            return_tx.send(()).unwrap();
            receive(&waiting_rx); // actually observed the active-stop predicate
            assert!(matches!(
                returned_rx.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            assert_eq!(
                retry(|| rv.snapshot()).owners,
                vec![(key, Mode::NativeStopped)]
            );
            trace_owners(&mut stopped, &[key]);
            complete(stopped);
            receive(&returned_rx);
            worker.join().unwrap();
        });
        finish_era(era);
    });
}

#[test]
fn retiring_image_and_starting_owner_keep_exact_frozen_membership() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(641, 1)
            .unwrap()
            .activate(era.heap_identity(), 641)
            .unwrap();
        let key = owner.key();
        let entry = owner.entry();
        seed(&mut owner, 641);
        let epoch = active(era.rendezvous().request().unwrap());
        let mut late = era
            .register(642, 1)
            .unwrap()
            .activate(era.heap_identity(), 642)
            .unwrap();
        assert!(matches!(late.run_access(), Err(RegistrationError::Busy)));
        owner.retire_rooted(&roots[..1]).unwrap();
        assert_eq!(era.rendezvous().snapshot().unwrap().frozen, vec![key]);
        let mut stopped = retry(|| era.stop());
        assert_eq!(stopped.epoch(), epoch);
        stopped
            .trace(|guard, _proof| {
                assert_eq!(read_words(guard, key).len(), 1);
                guard.with_owner_image(key, |image| {
                    assert_eq!(image.summary().remembered_words, 1)
                })?;
                Ok(())
            })
            .unwrap();
        complete(stopped);
        late.wait_started().unwrap();
        assert!(late.run_access().is_ok());
        assert!(entry.state.lock().unwrap().rendezvous.retiring);
        late.retire_rooted(&roots[1..2]).unwrap();
        finish_era(era);
    });
}

/// TSan-ready: real OS threads, actual shared root handles, repeated root/image
/// transfer. It deliberately performs no allocation or collector heap access.
#[test]
fn rooted_protocol_stress_two_owners_many_epochs() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        const EPOCHS: u64 = 24;
        let heap = era.heap_identity();
        let permits = [era.register(651, 1).unwrap(), era.register(652, 1).unwrap()];
        let keys = permits.each_ref().map(|permit| permit.key());
        thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let mut handles = Vec::new();
            for (index, permit) in permits.into_iter().enumerate() {
                let input = vec![roots[index].clone()];
                let ready = ready_tx.clone();
                handles.push(scope.spawn(move || {
                    let mut owner = activate(permit, heap, 651 + index as u64);
                    ready.send(()).unwrap();
                    for expected in 1..=EPOCHS {
                        assert_eq!(park(&mut owner, &input), expected);
                    }
                    retire(owner, &input);
                }));
            }
            for _ in 0..2 {
                receive(&ready_rx);
            }
            let mut original = None;
            for expected in 1..=EPOCHS {
                let mut stopped = retry(|| era.stop());
                assert_eq!(stopped.epoch().get(), expected);
                let words = trace_owners(&mut stopped, &keys);
                if let Some(previous) = &original {
                    assert_eq!(&words, previous);
                } else {
                    original = Some(words);
                }
                complete(stopped);
            }
            for handle in handles {
                handle.join().unwrap();
            }
        });
        finish_era(era);
    });
}

#[test]
fn native_publication_is_not_stopped_until_root_and_image_commit() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(660, 1).unwrap();
        let key = permit.key();
        let rv = era.rendezvous();
        thread::scope(|scope| {
            let (publishing_tx, publishing_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let (committed_tx, committed_rx) = mpsc::channel();
            let (return_tx, return_rx) = mpsc::channel();
            let input = vec![roots[0].clone()];
            let worker = scope.spawn(move || {
                let mut owner = activate(permit, heap, 660);
                owner.protocol.publication_hook = Some(Box::new(move || {
                    publishing_tx.send(()).unwrap();
                    receive(&release_rx);
                }));
                retry(|| owner.enter_native(&input));
                committed_tx.send(()).unwrap();
                receive(&return_rx);
                apply(owner.begin_restore_retry(true).unwrap());
                retire(owner, &input);
            });
            receive(&publishing_rx);
            let epoch = active(retry(|| rv.request()));
            assert_eq!(epoch.get(), 1);
            assert_eq!(
                retry(|| rv.snapshot()).owners,
                vec![(key, Mode::Publishing)]
            );
            for _ in 0..4 {
                rv.notify();
                assert!(retry(|| era.try_stop()).is_none());
            }
            release_tx.send(()).unwrap();
            receive(&committed_rx);
            let mut stopped = retry(|| era.stop());
            trace_owners(&mut stopped, &[key]);
            complete(stopped);
            return_tx.send(()).unwrap();
            worker.join().unwrap();
        });
        finish_era(era);
    });
}

/// A real returning owner holds the predicate mutex after ACK/stop, before its
/// atomic Condvar wait. Scope installation occurs inside the terminal trace.
#[test]
fn a6_ht_b9_table_scope_gate_contention_retries_before_job() {
    use super::super::super::collector_tables;
    Context::with_test_rooted_mutator_era(|era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1911, 1).unwrap();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let (return_tx, return_rx) = mpsc::channel();
            let (held_tx, held_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let worker = scope.spawn(move || {
                let mut owner = activate(permit, heap, 1911);
                retry(|| owner.enter_native(&input));
                ready_tx.send(()).unwrap();
                receive(&return_rx);
                owner.protocol.restore_wait_hook = Some(Box::new(move || {
                    held_tx.send(()).unwrap();
                    receive(&release_rx);
                }));
                let restored = match owner.begin_restore_retry(true) {
                    Ok(image) => {
                        apply(image);
                        true
                    }
                    Err(RendezvousError::Poisoned) => false,
                    Err(error) => panic!("unexpected restore refusal: {error:?}"),
                };
                if restored {
                    retire(owner, &input);
                } else {
                    drop(owner);
                }
                restored
            });
            receive(&ready_rx);
            let mut stopped = retry(|| era.stop());
            let mut release_tx = Some(release_tx);
            let mut body_calls = 0;
            let mut busy_observed = 0;
            let result = stopped.trace(|guard, _proof| {
                return_tx.send(()).unwrap();
                receive(&held_rx);
                let mut collector = era.core.collector.try_borrow_mut().unwrap();
                let heap = collector.as_mut().unwrap().registration_heap_mut();
                collector_tables::with_stopped_tables_for_trace_observing_busy(
                    heap,
                    guard,
                    |heap| {
                        assert_eq!(
                            collector_tables::with_stopped_tables_for_trace(heap, guard, |_| ()),
                            Err(RendezvousError::Busy)
                        );
                        body_calls += 1;
                        Ok(())
                    },
                    || {
                        busy_observed += 1;
                        if let Some(tx) = release_tx.take() {
                            tx.send(()).unwrap();
                        }
                    },
                )?
            });
            if result.is_ok() {
                complete(stopped);
            } else {
                drop(stopped);
            }
            let restored = worker.join().unwrap();
            assert_eq!(
                result,
                Ok(()),
                "pre-job gate contention must not poison trace"
            );
            assert!(
                busy_observed >= 1,
                "the real gate refusal must be exercised"
            );
            assert_eq!(body_calls, 1, "only the installed job runs");
            assert!(
                restored,
                "the same owner must restore after keeper completion"
            );
        });
        finish_era(era);
    });
}

#[test]
fn a6_ht_b9_table_scope_body_error_stays_terminal_after_image_mutation() {
    use super::super::super::collector_tables;
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(1912, 1)
            .unwrap()
            .activate(era.heap_identity(), 1912)
            .unwrap();
        let key = owner.key();
        seed(&mut owner, 1912);
        owner.enter_native(&roots[..1]).unwrap();
        let rv = era.rendezvous();
        let mut stopped = era.stop().unwrap();
        let mut body_calls = 0;
        let mut busy_observed = 0;
        let result = stopped.trace::<()>(|guard, _proof| {
            let mut collector = era.core.collector.try_borrow_mut().unwrap();
            let heap = collector.as_mut().unwrap().registration_heap_mut();
            collector_tables::with_stopped_tables_for_trace_observing_busy(
                heap,
                guard,
                |_| {
                    body_calls += 1;
                    guard.with_owner_image(key, |image| {
                        assert_eq!(
                            image
                                .consume_synthetic_remembered_for_test(MarkWord::of(
                                    TaggedValue::fixnum(1912)
                                ))
                                .unwrap(),
                            1
                        );
                    })?;
                    Err(RendezvousError::Busy)
                },
                || {
                    busy_observed += 1;
                },
            )?
        });
        assert_eq!(result, Err(RendezvousError::Busy));
        assert_eq!(body_calls, 1);
        assert_eq!(busy_observed, 0, "body Busy is never an installation retry");
        assert_eq!(rv.request(), Err(RendezvousError::Poisoned));
        drop(stopped);
        drop(owner);
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
fn a6_ht_b9_table_scope_keeper_gate_reentry_refuses_without_retry() {
    use super::super::super::collector_tables;
    Context::with_test_rooted_mutator_era(|era, _roots| {
        let mut stopped = era.stop().unwrap();
        let mut body_calls = 0;
        let mut busy_observed = 0;
        stopped
            .trace(|guard, _proof| {
                let _gate = era.registry().lock().unwrap();
                let mut collector = era.core.collector.try_borrow_mut().unwrap();
                let heap = collector.as_mut().unwrap().registration_heap_mut();
                assert_eq!(
                    collector_tables::with_stopped_tables_for_trace_observing_busy(
                        heap,
                        guard,
                        |_| {
                            body_calls += 1;
                        },
                        || {
                            busy_observed += 1;
                        },
                    ),
                    Err(RendezvousError::Busy)
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(body_calls, 0);
        assert_eq!(busy_observed, 0, "same-keeper gate reentry must not retry");
        complete(stopped);
        finish_era(era);
    });
}
