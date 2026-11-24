//! Rooted mutator-rendezvous rows re-homed out of the value lib test.
//!
//! The core shell opens the real rooted or plain mutator era and supplies the
//! SharedRoot vector; every protocol step and assertion stays in value.
#![forbid(unsafe_code)]

use super::super::EraKeeper;
use super::fixture_helpers::*;
use super::*;
use crate::tagged::gc::mark_word::MarkWord;
use crate::tagged::value::TaggedValue;

#[path = "failure_tests.rs"]
pub(crate) mod failure;
#[path = "self_wait_tests.rs"]
pub(crate) mod self_wait;

#[cfg(any(test, feature = "test-support"))]
std::thread_local! {
    static ROOTED_MAPPING_RUNNER: std::cell::Cell<Option<fn(&'static str)>> =
        const { std::cell::Cell::new(None) };
}

/// Install the core mapping shell's row runner for the legacy zero-arg
/// Control adapters. Without one an adapter fails loudly at setup (#1580).
pub(crate) fn install_rooted_mapping_runner(runner: fn(&'static str)) {
    ROOTED_MAPPING_RUNNER.with(|cell| cell.set(Some(runner)));
}

pub(crate) fn run_rooted_mapping_row(name: &'static str) {
    let runner = ROOTED_MAPPING_RUNNER
        .with(std::cell::Cell::get)
        .unwrap_or_else(|| {
            panic!(
                "rooted mapping runner is not installed on this thread; the core mapping shell must install it before the Control checks run"
            )
        });
    runner(name);
}

macro_rules! mapping_adapter {
    ($name:ident) => {
        pub(crate) fn $name() {
            run_rooted_mapping_row(stringify!($name));
        }
    };
}
mapping_adapter!(stale_ack_identity_generation_duplicate_and_missing_image_are_refused);
mapping_adapter!(two_owners_pause_before_ack_and_while_restoring_across_two_epochs);
mapping_adapter!(native_owner_sleeps_two_epochs_latest_image_and_synthetic_work_once);
mapping_adapter!(native_idle_restore_debt_queues_request_before_any_new_epoch);
mapping_adapter!(retiring_image_and_starting_owner_keep_exact_frozen_membership);
mapping_adapter!(live_worker_loan_blocks_ack_and_preserves_running_owner);
mapping_adapter!(native_publication_is_not_stopped_until_root_and_image_commit);
mapping_adapter!(native_return_waits_for_active_stop_before_restoration);

#[test]
fn rooted_mapping_adapter_requires_an_installed_runner() {
    let failure = std::panic::catch_unwind(|| {
        stale_ack_identity_generation_duplicate_and_missing_image_are_refused()
    });
    let payload = failure.expect_err("adapter must refuse without a runner");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("rooted mapping runner is not installed"),
        "{message}"
    );
}

fn two_owners_pause_before_ack_and_while_restoring_across_two_epochs_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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
                        owner.protocol.hooks.publication_hook = Some(Box::new(move || {
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
                        owner.run_access_after_resume_drain().unwrap();
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
                let epoch = active(rendezvous.request().unwrap());
                assert_eq!(epoch.get(), round);
                assert_eq!(active(rendezvous.request().unwrap()), epoch);
                assert!(era.try_stop().unwrap().is_none());
                for sender in &prepare {
                    sender.send(()).unwrap();
                }
                for _ in 0..2 {
                    receive(&publishing_rx);
                }
                let snapshot = rendezvous.snapshot().unwrap();
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
                let mut stopped = era.stop().unwrap();
                assert_eq!(stopped.epoch(), epoch);
                assert!(matches!(era.try_stop(), Err(RendezvousError::Busy)));
                let words = trace_owners_at(ObservationAdmission::Once, &mut stopped, &keys);
                if let Some(expected) = &first_roots {
                    assert_eq!(&words, expected);
                } else {
                    first_roots = Some(words);
                }
                stopped.complete().unwrap();
                assert_eq!(rendezvous.snapshot().unwrap().debt.len(), 2);
                if round == 1 {
                    assert_eq!(rendezvous.request(), Ok(Request::Queued));
                }
                for sender in &restore {
                    sender.send(()).unwrap();
                }
                for _ in 0..2 {
                    receive(&resuming_rx);
                }
                let snapshot = rendezvous.snapshot().unwrap();
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
            assert_eq!(rendezvous.snapshot().unwrap().completed_epoch, 2);
        });
        finish_era(era);
    });
}

fn stale_ack_identity_generation_duplicate_and_missing_image_are_refused_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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
        let mut stopped = era.stop().unwrap();
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
        trace_owners_at(ObservationAdmission::Once, &mut stopped, &[key]);
        assert_eq!(
            stopped.trace(|_, _| Ok(())),
            Err(RendezvousError::Publication)
        );
        stopped.complete().unwrap();
        owner.begin_restore(false).unwrap().apply().unwrap();
        assert!(matches!(
            owner.begin_restore(false),
            Err(RendezvousError::Busy)
        ));
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

fn live_worker_loan_blocks_ack_and_preserves_running_owner_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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
        let mut stopped = era.stop().unwrap();
        trace_owners_at(ObservationAdmission::Once, &mut stopped, &[key]);
        stopped.complete().unwrap();
        owner.begin_restore(false).unwrap().apply().unwrap();
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

fn native_owner_sleeps_two_epochs_latest_image_and_synthetic_work_once_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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

fn native_idle_restore_debt_queues_request_before_any_new_epoch_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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
        assert_eq!(rv.snapshot().unwrap().owners, vec![(key, Mode::Resuming)]);
        assert_eq!(rv.request(), Ok(Request::Queued));
        assert_eq!(rv.snapshot().unwrap().request_epoch, 0);
        response.apply().unwrap();
        assert_eq!(entry.state.lock().unwrap().rendezvous.image_takes, 1);
        let epoch = active(rv.request().unwrap());
        assert_eq!(epoch.get(), 1);
        owner.acknowledge(epoch, &roots[..1]).unwrap();
        let mut stopped = era.stop().unwrap();
        trace_owners_at(ObservationAdmission::Once, &mut stopped, &[key]);
        stopped.complete().unwrap();
        owner.begin_restore(false).unwrap().apply().unwrap();
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

fn native_return_waits_for_active_stop_before_restoration_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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
                owner.protocol.hooks.restore_wait_hook = Some(Box::new(move || {
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

fn retiring_image_and_starting_owner_keep_exact_frozen_membership_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let mut owner = era
            .register(641, 1)
            .unwrap()
            .activate(era.heap_identity(), 641)
            .unwrap();
        let key = owner.key();
        let entry = owner.entry();
        {
            let mut run = owner.run_access().unwrap();
            let local = run.state();
            local.hot.remset.push(TaggedValue::fixnum(641));
            local.hot.bytes_since_gc = 641;
        }
        let epoch = active(era.rendezvous().request().unwrap());
        let mut late = era
            .register(642, 1)
            .unwrap()
            .activate(era.heap_identity(), 642)
            .unwrap();
        assert!(matches!(late.run_access(), Err(RegistrationError::Busy)));
        owner.retire_rooted(&roots[..1]).unwrap();
        assert_eq!(era.rendezvous().snapshot().unwrap().frozen, vec![key]);
        let mut stopped = era.stop().unwrap();
        assert_eq!(stopped.epoch(), epoch);
        stopped
            .trace(|guard, _proof| {
                assert_eq!(
                    read_words_at(ObservationAdmission::Once, guard, key).len(),
                    1
                );
                guard.with_owner_image(key, |image| {
                    assert_eq!(image.summary().remembered_words, 1)
                })?;
                Ok(())
            })
            .unwrap();
        stopped.complete().unwrap();
        late.wait_started().unwrap();
        assert!(late.run_access().is_ok());
        assert!(entry.state.lock().unwrap().rendezvous.retiring);
        late.retire_rooted(&roots[1..2]).unwrap();
        finish_era(era);
    });
}

fn rooted_protocol_stress_two_owners_many_epochs_body<K: EraKeeper + std::fmt::Debug + 'static>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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

fn native_publication_is_not_stopped_until_root_and_image_commit_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
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
                owner.protocol.hooks.publication_hook = Some(Box::new(move || {
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

/// A lock held by another thread. The holder lets go once the caller under
/// test has met it (the contention fence moved) or has returned without
/// meeting it, so neither outcome rides a timer.
struct Holder<'scope> {
    holder: thread::ScopedJoinHandle<'scope, ()>,
    returned: mpsc::Sender<()>,
}

/// Run on the holder thread with the lock held: report it, then keep it
/// until the caller meets it or returns.
fn hold_until_met(before: u64, held: mpsc::Sender<()>, returned: &mpsc::Receiver<()>) {
    use super::super::test_fences::gate_contention_for_test;
    held.send(()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while gate_contention_for_test() == before && returned.try_recv().is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "the caller neither met the held lock nor returned"
        );
        thread::yield_now();
    }
}

impl<'scope> Holder<'scope> {
    /// Returns once BODY has taken its lock and called `hold_until_met`.
    fn spawn(
        scope: &'scope thread::Scope<'scope, '_>,
        body: impl FnOnce(u64, mpsc::Sender<()>, mpsc::Receiver<()>) + Send + 'scope,
    ) -> Self {
        let (held_tx, held_rx) = mpsc::channel();
        let (returned, returned_rx) = mpsc::channel::<()>();
        let before = super::super::test_fences::gate_contention_for_test();
        let holder = scope.spawn(move || body(before, held_tx, returned_rx));
        receive(&held_rx);
        Self { holder, returned }
    }

    /// The registry gate.
    fn gate<'env>(scope: &'scope thread::Scope<'scope, 'env>, registry: &'env Registry) -> Self {
        Self::spawn(scope, move |before, held, returned| {
            let gate = retry(|| registry.lock().map_err(RendezvousError::from));
            hold_until_met(before, held, &returned);
            drop(gate);
        })
    }

    /// The caller under test has returned: let go if still holding, and join.
    fn join(self) {
        let _ = self.returned.send(());
        self.holder.join().unwrap();
    }
}

/// A gate holder after a native publication commits must not strand the
/// owner without its receipt: the owner is NativeStopped by then and a second
/// publication is refused, so the caller cannot retry the whole call.
fn native_receipt_outlasts_a_gate_holder_after_the_commit_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1931, 1).unwrap();
        let registry = era.registry();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (committed_tx, committed_rx) = mpsc::channel();
            let (held_tx, held_rx) = mpsc::channel();
            let (published_tx, published_rx) = mpsc::channel();
            let owner = scope.spawn(move || {
                let mut owner = activate(permit, heap, 1931);
                owner.protocol.hooks.receipt_hook = Some(Box::new(move || {
                    committed_tx.send(()).unwrap();
                    receive(&held_rx);
                }));
                let receipt = owner.publish_native_owner(&input);
                published_tx
                    .send(
                        receipt
                            .as_ref()
                            .map(|receipt| receipt.serial_facts().0)
                            .map_err(|e| *e),
                    )
                    .unwrap();
                drop(receipt);
                apply(owner.begin_restore_retry(true).unwrap());
                retire(owner, &input);
            });
            receive(&committed_rx);
            let holder = Holder::gate(scope, registry);
            held_tx.send(()).unwrap();
            let published = receive(&published_rx);
            holder.join();
            owner.join().unwrap();
            assert_eq!(
                published,
                Ok(1931),
                "the committed publication returns its receipt"
            );
        });
        finish_era(era);
    });
}

/// The stopped keeper's look at the gate, to bind the stop and to validate
/// a receipt it is about to consume, waits out a brief holder instead of
/// refusing `Busy`.
fn stopped_keeper_waits_out_a_brief_gate_holder_body<K: EraKeeper + std::fmt::Debug + 'static>(
    rooted_era: RootedEraFn<K>,
) {
    use super::super::custody_brand::with_stop_scope;
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1932, 1).unwrap();
        let key = permit.key();
        let registry = era.registry();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (receipt_tx, receipt_rx) = mpsc::channel();
            let (return_tx, return_rx) = mpsc::channel::<()>();
            let owner = scope.spawn(move || {
                let mut owner = activate(permit, heap, 1932);
                let receipt = retry(|| owner.publish_native_owner(&input));
                receipt_tx.send(receipt).unwrap();
                receive(&return_rx);
                apply(owner.begin_restore_retry(true).unwrap());
                retire(owner, &input);
            });
            let receipt = receive(&receipt_rx);
            let mut stopped = retry(|| era.stop());
            trace_owners(&mut stopped, &[key]);
            with_stop_scope(|stop_scope| {
                let holder = Holder::gate(scope, registry);
                let interval = match stop_scope.bind(stopped) {
                    Ok(interval) => interval,
                    Err((_scope, stopped, reason)) => {
                        holder.join();
                        complete(stopped);
                        panic!("bind refused while another thread held the gate: {reason:?}");
                    }
                };
                holder.join();
                let holder = Holder::gate(scope, registry);
                let validated = interval.validate_owner(&receipt);
                holder.join();
                match interval.complete_traced() {
                    Ok(()) => {}
                    Err(failure) => complete(failure.into_stopped()),
                }
                validated.expect("validation waits out the holder");
            });
            drop(receipt);
            return_tx.send(()).unwrap();
            owner.join().unwrap();
        });
        finish_era(era);
    });
}

/// One native owner on its own thread for the keeper rows below. Returns its
/// thread and entry; the owner leaves and retires once RELEASE fires.
fn native_owner_for_keeper_rows<'scope, 'env>(
    scope: &'scope thread::Scope<'scope, 'env>,
    heap: HeapIdentity,
    permit: MutatorPermit<'env>,
    owner_id: u64,
    input: Vec<SharedRoot>,
    release: mpsc::Receiver<()>,
) -> (thread::ScopedJoinHandle<'scope, ()>, Arc<MutatorEntry>) {
    let (entry_tx, entry_rx) = mpsc::channel();
    let owner = scope.spawn(move || {
        let mut owner = activate(permit, heap, owner_id);
        retry(|| owner.enter_native(&input));
        entry_tx.send(owner.entry()).unwrap();
        receive(&release);
        apply(owner.begin_restore_retry(true).unwrap());
        retire(owner, &input);
    });
    (owner, receive(&entry_rx))
}

/// The keeper's trace preflight waits out a brief gate holder.
fn keeper_trace_waits_out_a_brief_gate_holder_body<K: EraKeeper + std::fmt::Debug + 'static>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1933, 1).unwrap();
        let registry = era.registry();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (release_tx, release_rx) = mpsc::channel();
            let (owner, _entry) =
                native_owner_for_keeper_rows(scope, heap, permit, 1933, input, release_rx);
            let mut stopped = retry(|| era.stop());
            let holder = Holder::gate(scope, registry);
            let refused = stopped.trace(|_stopped, _proof| Ok(())).err();
            holder.join();
            if refused.is_some() {
                // The preflight refused before the body: retry, then finish.
                retry(|| stopped.trace(|_stopped, _proof| Ok(())));
            }
            complete(stopped);
            release_tx.send(()).unwrap();
            owner.join().unwrap();
            assert_eq!(refused, None, "the trace preflight waits out the holder");
        });
        finish_era(era);
    });
}

/// `complete` consumes the stop, so a `Busy` from a brief gate holder would
/// leave the caller only the drop, which poisons the era. It waits instead.
fn keeper_complete_waits_out_a_brief_gate_holder_body<K: EraKeeper + std::fmt::Debug + 'static>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1934, 1).unwrap();
        let key = permit.key();
        let registry = era.registry();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (release_tx, release_rx) = mpsc::channel();
            let (owner, _entry) =
                native_owner_for_keeper_rows(scope, heap, permit, 1934, input, release_rx);
            let mut stopped = retry(|| era.stop());
            trace_owners(&mut stopped, &[key]);
            let holder = Holder::gate(scope, registry);
            let completed = stopped.complete();
            holder.join();
            let refused = completed.err().map(|failure| {
                let reason = failure.reason;
                complete(failure.into_stopped());
                reason
            });
            release_tx.send(()).unwrap();
            owner.join().unwrap();
            assert_eq!(refused, None, "complete waits out the gate holder");
        });
        finish_era(era);
    });
}

/// The reader scan in `complete` stays outside the gate. An owner that holds
/// the gate and its entry for a moment there is retried, not returned as a
/// consuming `Busy`.
fn keeper_complete_retries_an_entry_held_through_the_reader_scan_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    use super::super::test_fences::{gate_contention_for_test, install_owner_moment_hook_for_test};
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1935, 1).unwrap();
        let key = permit.key();
        let registry = era.registry();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (release_tx, release_rx) = mpsc::channel();
            let (owner, entry) =
                native_owner_for_keeper_rows(scope, heap, permit, 1935, input, release_rx);
            let mut stopped = retry(|| era.stop());
            trace_owners(&mut stopped, &[key]);
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (held_tx, held_rx) = mpsc::channel::<()>();
            let (returned_tx, returned_rx) = mpsc::channel::<()>();
            let before = gate_contention_for_test();
            // An owner's moment: the gate, then its entry, exactly when the
            // keeper starts the scan with the gate released.
            let holder = scope.spawn(move || {
                receive(&go_rx);
                let gate = retry(|| registry.lock().map_err(RendezvousError::from));
                let state = entry.state.lock().unwrap();
                hold_until_met(before, held_tx, &returned_rx);
                drop(state);
                drop(gate);
            });
            install_owner_moment_hook_for_test(Box::new(move || {
                go_tx.send(()).unwrap();
                receive(&held_rx);
            }));
            let completed = stopped.complete();
            let _ = returned_tx.send(());
            holder.join().unwrap();
            let refused = completed.err().map(|failure| {
                let reason = failure.reason;
                complete(failure.into_stopped());
                reason
            });
            release_tx.send(()).unwrap();
            owner.join().unwrap();
            assert_eq!(refused, None, "the reader scan retries the held entry");
        });
        finish_era(era);
    });
}

/// F1. Under its stop the keeper takes the census through the stop, whose
/// borrow keeps `trace` and `complete` out while the census holds the gate.
/// The era's own census refuses the keeper during the stop.
fn keeper_takes_summaries_through_its_stop_body<K: EraKeeper + std::fmt::Debug + 'static>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1936, 1).unwrap();
        let key = permit.key();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            // A running owner that acknowledges the stop, so its published
            // image carries the stop's epoch, as a census expects.
            let (running_tx, running_rx) = mpsc::channel::<()>();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let owner = scope.spawn(move || {
                let mut owner = activate(permit, heap, 1936);
                running_tx.send(()).unwrap();
                let epoch = retry(|| owner.wait_request());
                retry(|| owner.acknowledge(epoch, &input));
                receive(&release_rx);
                let image = owner.begin_restore_retry(false).unwrap();
                apply(image);
                retire(owner, &input);
            });
            receive(&running_rx);
            let mut stopped = retry(|| era.stop());
            trace_owners(&mut stopped, &[key]);
            let era_census = era.stopped_summaries().err();
            let stop_census = stopped
                .summaries()
                .and_then(|summaries| summaries.census().map(|_| ()));
            let completed = stopped.complete().err().map(|failure| {
                let reason = failure.reason;
                complete(failure.into_stopped());
                reason
            });
            release_tx.send(()).unwrap();
            owner.join().unwrap();
            assert_eq!(
                era_census,
                Some(RegistrationError::WrongThread),
                "the era refuses the keeper's census during its stop"
            );
            stop_census.expect("the census under the stop");
            assert_eq!(
                completed, None,
                "the stop completes once the census is gone"
            );
        });
        finish_era(era);
    });
}

/// F2. Under a stop, verify_stopped's "not stopped yet" is a broken census,
/// not a `Busy` to wait on. With the census restored, the stop completes.
fn a_broken_census_under_a_stop_is_not_busy_body<K: EraKeeper + std::fmt::Debug + 'static>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1937, 1).unwrap();
        let key = permit.key();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (release_tx, release_rx) = mpsc::channel();
            let (owner, entry) =
                native_owner_for_keeper_rows(scope, heap, permit, 1937, input, release_rx);
            let mut stopped = retry(|| era.stop());
            let stopped_mode = std::mem::replace(
                &mut entry.state.lock().unwrap().rendezvous.mode,
                Mode::Running,
            );
            let refused = stopped.trace(|_stopped, _proof| Ok(())).err();
            entry.state.lock().unwrap().rendezvous.mode = stopped_mode;
            trace_owners(&mut stopped, &[key]);
            let completed = stopped.complete().err().map(|failure| {
                let reason = failure.reason;
                complete(failure.into_stopped());
                reason
            });
            release_tx.send(()).unwrap();
            owner.join().unwrap();
            assert_eq!(
                refused,
                Some(RendezvousError::Publication),
                "a frozen owner that is not stopped breaks the census"
            );
            assert_eq!(
                completed, None,
                "with the census restored, the stop completes"
            );
        });
        finish_era(era);
    });
}

/// F3. The keeper's world-root check waits out a brief gate holder.
fn keeper_world_root_check_waits_out_a_brief_gate_holder_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1938, 1).unwrap();
        let key = permit.key();
        let registry = era.registry();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (release_tx, release_rx) = mpsc::channel();
            let (owner, _entry) =
                native_owner_for_keeper_rows(scope, heap, permit, 1938, input, release_rx);
            let mut stopped = retry(|| era.stop());
            let holder = Holder::gate(scope, registry);
            let checked = stopped.check_world_roots(heap);
            holder.join();
            trace_owners(&mut stopped, &[key]);
            complete(stopped);
            release_tx.send(()).unwrap();
            owner.join().unwrap();
            assert_eq!(checked, Ok(()), "the check waits out the holder");
        });
        finish_era(era);
    });
}

/// F3. The root reader takes the owner's entry after the gate is released.
/// An owner's gate-and-entry moment there is waited out.
fn keeper_root_reader_waits_out_an_owner_moment_on_its_entry_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    use super::super::test_fences::{gate_contention_for_test, install_owner_moment_hook_for_test};
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1939, 1).unwrap();
        let key = permit.key();
        let registry = era.registry();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (release_tx, release_rx) = mpsc::channel();
            let (owner, entry) =
                native_owner_for_keeper_rows(scope, heap, permit, 1939, input, release_rx);
            let mut stopped = retry(|| era.stop());
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (held_tx, held_rx) = mpsc::channel::<()>();
            let (returned_tx, returned_rx) = mpsc::channel::<()>();
            let before = gate_contention_for_test();
            let holder = scope.spawn(move || {
                receive(&go_rx);
                let gate = retry(|| registry.lock().map_err(RendezvousError::from));
                let state = entry.state.lock().unwrap();
                hold_until_met(before, held_tx, &returned_rx);
                drop(state);
                drop(gate);
            });
            install_owner_moment_hook_for_test(Box::new(move || {
                go_tx.send(()).unwrap();
                receive(&held_rx);
            }));
            let read = stopped.with_root_reader(key, |reader| reader.finish());
            let _ = returned_tx.send(());
            holder.join().unwrap();
            trace_owners(&mut stopped, &[key]);
            complete(stopped);
            release_tx.send(()).unwrap();
            owner.join().unwrap();
            assert_eq!(
                read.map(|finished| finished.is_ok()),
                Ok(true),
                "the root reader waits out the owner's moment"
            );
        });
        finish_era(era);
    });
}

fn a6_ht_b9_table_scope_gate_contention_retries_before_job_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    use super::super::super::collector_tables;
    use super::super::test_fences::gate_contention_for_test;
    rooted_era(&mut |era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(1911, 1).unwrap();
        let input = vec![roots[0].clone()];
        thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let (return_tx, return_rx) = mpsc::channel();
            let (held_tx, held_rx) = mpsc::channel();
            let before = gate_contention_for_test();
            let worker = scope.spawn(move || {
                let mut owner = activate(permit, heap, 1911);
                retry(|| owner.enter_native(&input));
                ready_tx.send(()).unwrap();
                receive(&return_rx);
                // The hook runs holding the gate. The keeper's table scope
                // waits out a held gate (L6.7 F3) instead of refusing Busy,
                // so the hook holds it until the keeper has met it. A gate
                // holder must not wait on the keeper.
                owner.protocol.hooks.restore_wait_hook = Some(Box::new(move || {
                    held_tx.send(()).unwrap();
                    let deadline = std::time::Instant::now() + Duration::from_secs(30);
                    while gate_contention_for_test() == before {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "the keeper never met the held gate"
                        );
                        thread::yield_now();
                    }
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
                    || busy_observed += 1,
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
                gate_contention_for_test() > before,
                "the keeper met the real held gate"
            );
            assert_eq!(
                busy_observed, 0,
                "the table scope waits out the holder instead of refusing Busy"
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

fn a6_ht_b9_table_scope_body_error_stays_terminal_after_image_mutation_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    use super::super::super::collector_tables;
    rooted_era(&mut |era, roots| {
        let mut owner = era
            .register(1912, 1)
            .unwrap()
            .activate(era.heap_identity(), 1912)
            .unwrap();
        let key = owner.key();
        {
            let mut run = owner.run_access().unwrap();
            let local = run.state();
            local.hot.remset.push(TaggedValue::fixnum(1912));
            local.hot.bytes_since_gc = 1912;
        }
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

fn a6_ht_b9_table_scope_keeper_gate_reentry_refuses_without_retry_body<
    K: EraKeeper + std::fmt::Debug + 'static,
>(
    rooted_era: RootedEraFn<K>,
) {
    use super::super::super::collector_tables;
    rooted_era(&mut |era, _roots| {
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
        stopped.complete().unwrap();
        finish_era(era);
    });
}

/// One runner for the whole rooted-rendezvous subtree. The match is exhaustive
/// over the typed case list; `name: &str` dispatch is not used.
pub(crate) fn run_rooted_mutator_case<K: EraKeeper + std::fmt::Debug + 'static>(
    case: RootedRendezvousCase,
    rooted_era: RootedEraFn<K>,
    mutator_era: MutatorEraFn<K>,
) {
    match case {
        RootedRendezvousCase::TwoOwnersPauseBeforeAckAndWhileRestoringAcrossTwoEpochs => {
            two_owners_pause_before_ack_and_while_restoring_across_two_epochs_body(rooted_era)
        }
        RootedRendezvousCase::StaleAckIdentityGenerationDuplicateAndMissingImageAreRefused => {
            stale_ack_identity_generation_duplicate_and_missing_image_are_refused_body(rooted_era)
        }
        RootedRendezvousCase::LiveWorkerLoanBlocksAckAndPreservesRunningOwner => {
            live_worker_loan_blocks_ack_and_preserves_running_owner_body(rooted_era)
        }
        RootedRendezvousCase::NativeOwnerSleepsTwoEpochsLatestImageAndSyntheticWorkOnce => {
            native_owner_sleeps_two_epochs_latest_image_and_synthetic_work_once_body(rooted_era)
        }
        RootedRendezvousCase::NativeIdleRestoreDebtQueuesRequestBeforeAnyNewEpoch => {
            native_idle_restore_debt_queues_request_before_any_new_epoch_body(rooted_era)
        }
        RootedRendezvousCase::NativeReturnWaitsForActiveStopBeforeRestoration => {
            native_return_waits_for_active_stop_before_restoration_body(rooted_era)
        }
        RootedRendezvousCase::RetiringImageAndStartingOwnerKeepExactFrozenMembership => {
            retiring_image_and_starting_owner_keep_exact_frozen_membership_body(rooted_era)
        }
        RootedRendezvousCase::RootedProtocolStressTwoOwnersManyEpochs => {
            rooted_protocol_stress_two_owners_many_epochs_body(rooted_era)
        }
        RootedRendezvousCase::NativePublicationIsNotStoppedUntilRootAndImageCommit => {
            native_publication_is_not_stopped_until_root_and_image_commit_body(rooted_era)
        }
        RootedRendezvousCase::A6HtB9TableScopeGateContentionRetriesBeforeJob => {
            a6_ht_b9_table_scope_gate_contention_retries_before_job_body(rooted_era)
        }
        RootedRendezvousCase::A6HtB9TableScopeBodyErrorStaysTerminalAfterImageMutation => {
            a6_ht_b9_table_scope_body_error_stays_terminal_after_image_mutation_body(rooted_era)
        }
        RootedRendezvousCase::A6HtB9TableScopeKeeperGateReentryRefusesWithoutRetry => {
            a6_ht_b9_table_scope_keeper_gate_reentry_refuses_without_retry_body(rooted_era)
        }
        RootedRendezvousCase::TraceCallbackErrorPoisonIsVisibleBeforeGuardDrop => {
            failure::trace_callback_error_poison_is_visible_before_guard_drop_body(rooted_era)
        }
        RootedRendezvousCase::TraceCallbackPanicPoisonRetainsPublishedImages => {
            failure::trace_callback_panic_poison_retains_published_images_body(rooted_era)
        }
        RootedRendezvousCase::DroppedStoppedGuardWakesOwnerWaitingForCompletion => {
            failure::dropped_stopped_guard_wakes_owner_waiting_for_completion_body(rooted_era)
        }
        RootedRendezvousCase::UnfinishedReaderDropRefusesResumeAndRetainsRoots => {
            failure::unfinished_reader_drop_refuses_resume_and_retains_roots_body(rooted_era)
        }
        RootedRendezvousCase::ForgottenReaderRefusesResumeAndRetainsRoots => {
            failure::forgotten_reader_refuses_resume_and_retains_roots_body(rooted_era)
        }
        RootedRendezvousCase::CompletionRequiresTraceAndEveryExplicitReaderFinish => {
            failure::completion_requires_trace_and_every_explicit_reader_finish_body(rooted_era)
        }
        RootedRendezvousCase::ReaderCallbackReentryRefusesRunAccessWithoutWaiting => {
            failure::reader_callback_reentry_refuses_run_access_without_waiting_body(rooted_era)
        }
        RootedRendezvousCase::LostPoisonNotificationBetweenPredicateAndWaitFailsClosed => {
            failure::lost_poison_notification_between_predicate_and_wait_fails_closed_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::ExternalPoisonBetweenPredicateAndWaitIsEventDriven => {
            failure::external_poison_between_predicate_and_wait_is_event_driven_body(mutator_era)
        }
        RootedRendezvousCase::MutexUnwindRecoversGuardBeforePublishingTerminalPoison => {
            failure::mutex_unwind_recovers_guard_before_publishing_terminal_poison_body(mutator_era)
        }
        RootedRendezvousCase::CondvarReacquireRecoversPoisonedGuardWithoutRecursiveLock => {
            failure::condvar_reacquire_recovers_poisoned_guard_without_recursive_lock_body(
                mutator_era,
            )
        }
        RootedRendezvousCase::StaleRegistrationExhaustionAndFrozenRemovalRefuseBeforeTrace => {
            failure::stale_registration_exhaustion_and_frozen_removal_refuse_before_trace_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::ForgottenRunAccessBlocksFinishAndAckThenOwnerDropPoisons => {
            failure::forgotten_run_access_blocks_finish_and_ack_then_owner_drop_poisons_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::LegacyEpochOnlyPublicationAndRestoreCannotBypassRootedRendezvous => {
            failure::legacy_epoch_only_publication_and_restore_cannot_bypass_rooted_rendezvous_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::CallbackPanicOutsideTraceImmediatelyPoisonsRootAndImageReaders => {
            failure::callback_panic_outside_trace_immediately_poisons_root_and_image_readers_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::KeeperRunningOwnerStopRefusesAndCanAckAfterward => {
            self_wait::keeper_running_owner_stop_refuses_and_can_ack_afterward_body(rooted_era)
        }
        RootedRendezvousCase::KeeperNativeRestoreRefusesActiveStopAndKeepsImage => {
            self_wait::keeper_native_restore_refuses_active_stop_and_keeps_image_body(rooted_era)
        }
        RootedRendezvousCase::KeeperStartingOwnerWaitsRefuseUntilBootstrapOpens => {
            self_wait::keeper_starting_owner_waits_refuse_until_bootstrap_opens_body(rooted_era)
        }
        RootedRendezvousCase::AlreadyPublishedOwnerWaitRequestRefuses => {
            self_wait::already_published_owner_wait_request_refuses_body(rooted_era)
        }
        RootedRendezvousCase::KeeperResumeDebtRefusesNextStopUntilOwnerApplies => {
            self_wait::keeper_resume_debt_refuses_next_stop_until_owner_applies_body(rooted_era)
        }
        RootedRendezvousCase::FirstRestoredOwnerWaitsForTheFinalPeerDebt => {
            super::run_admission::first_restored_owner_waits_for_the_final_peer_debt_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::PoisonWakesARestoredOwnerWaitingForPeerDebt => {
            super::run_admission::poison_wakes_a_restored_owner_waiting_for_peer_debt_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::StopAndUnrestoredOwnerRefuseInsteadOfWaitingForThemselves => {
            super::run_admission::stop_and_unrestored_owner_refuse_instead_of_waiting_for_themselves_body(
                rooted_era,
            )
        }
        RootedRendezvousCase::NativeReceiptOutlastsAGateHolderAfterTheCommit => {
            native_receipt_outlasts_a_gate_holder_after_the_commit_body(rooted_era)
        }
        RootedRendezvousCase::StoppedKeeperWaitsOutABriefGateHolder => {
            stopped_keeper_waits_out_a_brief_gate_holder_body(rooted_era)
        }
        RootedRendezvousCase::KeeperTraceWaitsOutABriefGateHolder => {
            keeper_trace_waits_out_a_brief_gate_holder_body(rooted_era)
        }
        RootedRendezvousCase::KeeperCompleteWaitsOutABriefGateHolder => {
            keeper_complete_waits_out_a_brief_gate_holder_body(rooted_era)
        }
        RootedRendezvousCase::KeeperCompleteRetriesAnEntryHeldThroughTheReaderScan => {
            keeper_complete_retries_an_entry_held_through_the_reader_scan_body(rooted_era)
        }
        RootedRendezvousCase::KeeperTakesSummariesThroughItsStop => {
            keeper_takes_summaries_through_its_stop_body(rooted_era)
        }
        RootedRendezvousCase::ABrokenCensusUnderAStopIsNotBusy => {
            a_broken_census_under_a_stop_is_not_busy_body(rooted_era)
        }
        RootedRendezvousCase::KeeperWorldRootCheckWaitsOutABriefGateHolder => {
            keeper_world_root_check_waits_out_a_brief_gate_holder_body(rooted_era)
        }
        RootedRendezvousCase::KeeperRootReaderWaitsOutAnOwnerMomentOnItsEntry => {
            keeper_root_reader_waits_out_an_owner_moment_on_its_entry_body(rooted_era)
        }
    }
}
