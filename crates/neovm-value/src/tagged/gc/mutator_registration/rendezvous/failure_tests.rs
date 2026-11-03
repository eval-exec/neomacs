#![forbid(unsafe_code)]

use super::*;

#[test]
fn trace_callback_error_poison_is_visible_before_guard_drop() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(701, 1)
            .unwrap()
            .activate(era.heap_identity(), 701)
            .unwrap();
        owner.enter_native(&roots[..1]).unwrap();
        let rv = era.rendezvous();
        let mut stopped = era.stop().unwrap();
        assert_eq!(
            stopped.trace::<()>(|_, _| Err(RendezvousError::Root)),
            Err(RendezvousError::Root)
        );
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
fn trace_callback_panic_poison_retains_published_images() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(702, 1)
            .unwrap()
            .activate(era.heap_identity(), 702)
            .unwrap();
        let entry = owner.entry();
        owner.enter_native(&roots[..1]).unwrap();
        let rv = era.rendezvous();
        let mut stopped = era.stop().unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = stopped.trace::<()>(|_, _| panic!("synthetic trace panic"));
        }))
        .unwrap_err();
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"synthetic trace panic"));
        assert_eq!(rv.request(), Err(RendezvousError::Poisoned));
        assert!(entry.state.lock().unwrap().image.is_some());
        assert!(entry.state.lock().unwrap().roots.is_some());
        drop(stopped);
        drop(owner);
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
fn dropped_stopped_guard_wakes_owner_waiting_for_completion() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let heap = era.heap_identity();
        let permit = era.register(703, 1).unwrap();
        let input = vec![roots[0].clone()];
        let rv = era.rendezvous();
        thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let (ack_tx, ack_rx) = mpsc::channel();
            let worker = scope.spawn(move || {
                let mut owner = activate(permit, heap, 703);
                ready_tx.send(()).unwrap();
                let epoch = retry(|| owner.wait_request());
                retry(|| owner.acknowledge(epoch, &input));
                ack_tx.send(()).unwrap();
                assert!(matches!(
                    owner.begin_restore_retry(false),
                    Err(RendezvousError::Poisoned)
                ));
            });
            receive(&ready_rx);
            retry(|| rv.request());
            receive(&ack_rx);
            drop(retry(|| era.stop()));
            worker.join().unwrap();
        });
        assert_eq!(rv.request(), Err(RendezvousError::Poisoned));
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

fn abandoned_reader_case(forget: bool) {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(704, 1)
            .unwrap()
            .activate(era.heap_identity(), 704)
            .unwrap();
        let key = owner.key();
        let entry = owner.entry();
        owner.enter_native(&roots[..1]).unwrap();
        let mut stopped = era.stop().unwrap();
        stopped
            .trace(|guard, _| {
                guard.with_root_reader(key, |reader| {
                    let mut words = Vec::new();
                    reader.append_words(&mut words);
                    assert_eq!(words.len(), 1);
                    if forget {
                        std::mem::forget(reader);
                    } else {
                        drop(reader);
                    }
                })?;
                Ok(())
            })
            .unwrap();
        let failure = stopped.complete().unwrap_err();
        assert_eq!(failure.reason, RendezvousError::Readers);
        assert!(entry.state.lock().unwrap().roots.is_some());
        assert!(entry.state.lock().unwrap().image.is_some());
        drop(failure);
        drop(owner);
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
pub(super) fn unfinished_reader_drop_refuses_resume_and_retains_roots() {
    abandoned_reader_case(false);
}

#[test]
pub(super) fn forgotten_reader_refuses_resume_and_retains_roots() {
    abandoned_reader_case(true);
}

#[test]
pub(super) fn completion_requires_trace_and_every_explicit_reader_finish() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(705, 1)
            .unwrap()
            .activate(era.heap_identity(), 705)
            .unwrap();
        let key = owner.key();
        owner.enter_native(&roots[..1]).unwrap();
        let stopped = era.stop().unwrap();
        let failure = stopped.complete().unwrap_err();
        assert_eq!(failure.reason, RendezvousError::Publication);
        let mut stopped = failure.into_stopped();
        stopped
            .trace(|guard, _| {
                for _ in 0..2 {
                    assert_eq!(read_words(guard, key).len(), 1);
                }
                Ok(())
            })
            .unwrap();
        {
            let mut gate = era.registry().lock().unwrap();
            gate.rendezvous.active_readers = 2;
        }
        let failure = stopped.complete().unwrap_err();
        assert_eq!(failure.reason, RendezvousError::Readers);
        let stopped = failure.into_stopped();
        era.registry().lock().unwrap().rendezvous.active_readers = 1;
        let failure = stopped.complete().unwrap_err();
        assert_eq!(failure.reason, RendezvousError::Readers);
        era.registry().lock().unwrap().rendezvous.active_readers = 0;
        failure.into_stopped().complete().unwrap();
        owner.leave_native().unwrap();
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

#[test]
fn reader_callback_reentry_refuses_run_access_without_waiting() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(706, 1)
            .unwrap()
            .activate(era.heap_identity(), 706)
            .unwrap();
        let key = owner.key();
        owner.enter_native(&roots[..1]).unwrap();
        let mut stopped = era.stop().unwrap();
        stopped
            .trace(|guard, _| {
                guard.with_root_reader(key, |reader| {
                    assert!(matches!(owner.run_access(), Err(RegistrationError::Busy)));
                    reader.finish().unwrap();
                })?;
                Ok(())
            })
            .unwrap();
        stopped.complete().unwrap();
        owner.leave_native().unwrap();
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

#[test]
pub(super) fn lost_poison_notification_between_predicate_and_wait_fails_closed() {
    Context::with_test_rooted_mutator_era(|era, _roots| {
        let registry = era.registry();
        let gate = registry.lock().unwrap();
        assert!(!registry.park.poisoned.load(Ordering::Acquire));
        // Model the exact gap AFTER wait's initial check, BEFORE CV wait:
        // same-thread poison recognizes the held gate; wait rechecks it.
        registry.poison();
        registry.park.wake.notify_all();
        assert!(matches!(
            wait_after_poison_check(registry, gate),
            Err(RendezvousError::Poisoned)
        ));
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
pub(super) fn external_poison_between_predicate_and_wait_is_event_driven() {
    Context::with_test_mutator_era(|era| {
        let registry = era.registry();
        std::thread::scope(|scope| {
            let gate = registry.lock().unwrap();
            assert!(!registry.park.poisoned.load(Ordering::Acquire));
            let (attempted, ready) = std::sync::mpsc::channel();
            let poisoner = scope.spawn(move || {
                assert!(matches!(
                    registry.gate.state.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                attempted.send(()).unwrap();
                registry.poison(); // blocks until atomic Condvar release
            });
            ready
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            assert!(matches!(
                wait_after_poison_check(registry, gate),
                Err(RendezvousError::Poisoned)
            ));
            poisoner.join().unwrap();
            assert_eq!(registry.gate.gate_owner.load(Ordering::Acquire), 0);
        });
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
fn mutex_unwind_recovers_guard_before_publishing_terminal_poison() {
    Context::with_test_mutator_era(|era| {
        let registry = era.registry();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _gate = registry.lock().unwrap();
            panic!("forced registry gate unwind");
        }));
        assert!(panic.is_err());
        assert_eq!(registry.gate.gate_owner.load(Ordering::Acquire), 0);
        assert!(matches!(registry.lock(), Err(RegistrationError::Poisoned)));
        assert!(registry.park.poisoned.load(Ordering::Acquire));
        assert_eq!(registry.gate.gate_owner.load(Ordering::Acquire), 0);
        // Both the external blocking path and recovered try-lock path are safe.
        registry.poison();
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
fn condvar_reacquire_recovers_poisoned_guard_without_recursive_lock() {
    Context::with_test_mutator_era(|era| {
        let registry = era.registry();
        std::thread::scope(|scope| {
            let gate = registry.lock().unwrap();
            let poisoner = scope.spawn(move || {
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _gate = loop {
                        match registry.lock() {
                            Ok(guard) => break guard,
                            Err(RegistrationError::Busy) => std::thread::yield_now(),
                            Err(error) => panic!("unexpected lock failure: {error:?}"),
                        }
                    };
                    panic!("forced condvar mutex unwind");
                }));
                assert!(panic.is_err());
                registry.poison(); // recovers real poisoned mutex and notifies
            });
            assert!(matches!(
                wait_after_poison_check(registry, gate),
                Err(RendezvousError::Poisoned)
            ));
            poisoner.join().unwrap();
            assert_eq!(registry.gate.gate_owner.load(Ordering::Acquire), 0);
        });
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
pub(super) fn stale_registration_exhaustion_and_frozen_removal_refuse_before_trace() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(707, 1)
            .unwrap()
            .activate(era.heap_identity(), 707)
            .unwrap();
        let key = owner.key();
        let entry = owner.entry();
        let epoch = active(era.rendezvous().request().unwrap());
        entry.state.lock().unwrap().publication_generation = u64::MAX;
        assert_eq!(
            owner.acknowledge(epoch, &roots[..1]),
            Err(RendezvousError::Generation)
        );
        assert_eq!(entry.state.lock().unwrap().rendezvous.mode, Mode::Running);
        entry.state.lock().unwrap().publication_generation = 0;
        owner.acknowledge(epoch, &roots[..1]).unwrap();
        entry.state.lock().unwrap().rendezvous.mode = Mode::Retired;
        assert!(matches!(era.try_stop(), Err(RendezvousError::Identity)));
        entry.state.lock().unwrap().rendezvous.mode = Mode::Stopped;
        {
            let gate = era.registry().lock().unwrap();
            let mut stale = key;
            stale.generation = NonZeroU64::new(key.generation.get() + 1).unwrap();
            assert!(matches!(
                era.registry().validate(&gate, stale),
                Err(RegistrationError::StaleRegistration)
            ));
        }
        let mut stopped = era.stop().unwrap();
        trace_owners(&mut stopped, &[key]);
        stopped.complete().unwrap();
        owner.begin_restore(false).unwrap().apply().unwrap();
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

#[test]
fn forgotten_run_access_blocks_finish_and_ack_then_owner_drop_poisons() {
    Context::with_test_rooted_mutator_era(|era, _roots| {
        let mut owner = era
            .register(709, 1)
            .unwrap()
            .activate(era.heap_identity(), 709)
            .unwrap();
        std::mem::forget(owner.run_access().unwrap());
        // The compiler forbids moving an era while a live owner is borrowed.
        // Forget the owner as well to exercise the retained runtime obligation.
        std::mem::forget(owner);
        let registry = Arc::clone(era.core.registry.as_ref().unwrap());
        let failure = era.finish().unwrap_err();
        assert_eq!(failure.reason, RegistrationError::OutstandingLoan);
        drop(failure);
        assert!(registry.park.poisoned.load(Ordering::Acquire));
    });
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(710, 1)
            .unwrap()
            .activate(era.heap_identity(), 710)
            .unwrap();
        let entry = owner.entry();
        std::mem::forget(owner.run_access().unwrap());
        let rv = era.rendezvous();
        let epoch = active(rv.request().unwrap());
        assert_eq!(
            owner.acknowledge(epoch, &roots[..1]),
            Err(RendezvousError::Loan)
        );
        assert!(entry.admitted());
        assert_eq!(entry.state.lock().unwrap().rendezvous.mode, Mode::Running);
        assert!(era.try_stop().unwrap().is_none());
        drop(owner);
        assert_eq!(rv.request(), Err(RendezvousError::Poisoned));
        assert_eq!(
            era.finish().unwrap_err().reason,
            RegistrationError::Poisoned
        );
    });
}

#[test]
fn legacy_epoch_only_publication_and_restore_cannot_bypass_rooted_rendezvous() {
    Context::with_test_rooted_mutator_era(|era, roots| {
        let mut owner = era
            .register(711, 1)
            .unwrap()
            .activate(era.heap_identity(), 711)
            .unwrap();
        let key = owner.key();
        let epoch = active(era.rendezvous().request().unwrap());
        assert_eq!(
            owner.begin_publication(epoch).unwrap().publish(),
            Err(RegistrationError::Busy)
        );
        owner.acknowledge(epoch, &roots[..1]).unwrap();
        assert_eq!(owner.restore(epoch), Err(RegistrationError::Publication));
        let mut stopped = era.stop().unwrap();
        trace_owners(&mut stopped, &[key]);
        complete(stopped);
        // Even after collector completion, only the one-shot rooted resume
        // carries the current generation and discharges restoration debt.
        assert_eq!(owner.restore(epoch), Err(RegistrationError::Publication));
        owner.begin_restore(false).unwrap().apply().unwrap();
        assert_eq!(
            owner.begin_publication(epoch).unwrap().publish(),
            Err(RegistrationError::Busy)
        );
        owner.retire_rooted(&roots[..1]).unwrap();
        finish_era(era);
    });
}

#[test]
fn callback_panic_outside_trace_immediately_poisons_root_and_image_readers() {
    for image in [false, true] {
        Context::with_test_rooted_mutator_era(|era, roots| {
            let mut owner = era
                .register(712, 1)
                .unwrap()
                .activate(era.heap_identity(), 712)
                .unwrap();
            let key = owner.key();
            owner.enter_native(&roots[..1]).unwrap();
            let stopped = era.stop().unwrap();
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if image {
                    let _ =
                        stopped.with_owner_image(key, |_| panic!("image callback outside trace"));
                } else {
                    let _ = stopped
                        .with_root_reader(key, |_reader| panic!("root callback outside trace"));
                }
            }));
            assert!(panic.is_err());
            assert_eq!(era.rendezvous().request(), Err(RendezvousError::Poisoned));
            drop(stopped);
            drop(owner);
            assert_eq!(
                era.finish().unwrap_err().reason,
                RegistrationError::Poisoned
            );
        });
    }
}
