#![forbid(unsafe_code)]

mod model;
#[path = "rendezvous/resume_drain.rs"]
mod resume_drain;
mod support;

use loom::sync::Arc;
use loom::thread;
use model::{Fault, Mode, Phase, Refusal, Rendezvous, Request};
use support::{check, negative};

fn cycle(r: &Rendezvous) {
    r.request().unwrap();
    let mut stopped = r.try_stop().unwrap().expect("fixture owners stopped");
    stopped.trace();
    stopped.resume();
}

fn restore(r: &Rendezvous, slot: usize) {
    r.begin_restore(r.key(slot), false).unwrap();
    r.finish_restore(r.key(slot)).unwrap();
}

#[test]
fn keeper_two_allocating_owners_one_epoch() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        let workers: Vec<_> = (0..2)
            .map(|slot| {
                let r = r.clone();
                thread::spawn(move || {
                    let owner = r.owner(r.key(slot));
                    for _ in 0..1 {
                        owner.allocate().unwrap();
                        owner.park_once().unwrap();
                    }
                })
            })
            .collect();
        for _ in 0..1 {
            let mut stopped = r.stop().unwrap();
            stopped.trace();
            stopped.resume();
        }
        for worker in workers {
            worker.join().unwrap();
        }
        let s = r.snapshot();
        assert_eq!(s.phase, Phase::Idle);
        assert_eq!(s.traces, 1);
        for e in s.entries {
            assert_eq!(e.folded, e.issued);
            assert_eq!(e.issued, 2);
            assert_eq!(e.image_takes, 1);
            assert!(e.restored && !e.image_resident);
        }
    });
}

#[test]
fn concurrent_requesters_coalesce() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let r = r.clone();
                thread::spawn(move || r.request().unwrap())
            })
            .collect();
        for w in workers {
            assert_eq!(w.join().unwrap(), Request::Active(1));
        }
        assert_eq!(r.snapshot().epoch, 1);
        assert_eq!(r.snapshot().frozen.len(), 2);
    });
}

#[test]
fn queued_stop_waits_for_every_resume_debt() {
    check(1, || {
        let r = Rendezvous::new(2);
        r.request().unwrap();
        for slot in 0..2 {
            r.ack(r.key(slot), true).unwrap();
        }
        cycle(&r);
        r.begin_restore(r.key(0), false).unwrap();
        assert_eq!(r.request().unwrap(), Request::Queued);
        assert_eq!(r.snapshot().epoch, 1);
        r.finish_restore(r.key(0)).unwrap();
        assert_eq!(r.snapshot().phase, Phase::DrainingResumes);
        assert_eq!(r.request().unwrap(), Request::Queued);
        restore(&r, 1);
        assert_eq!(r.request().unwrap(), Request::Active(2));
        for slot in 0..2 {
            r.ack(r.key(slot), true).unwrap();
        }
        cycle(&r);
        for slot in 0..2 {
            restore(&r, slot);
        }
        assert_eq!(r.snapshot().phase, Phase::Idle);
    });
}

#[test]
fn native_entry_races_request_without_polling_in_native() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        let native = {
            let r = r.clone();
            thread::spawn(move || r.enter_native(r.key(0)).unwrap())
        };
        let ordinary = {
            let r = r.clone();
            thread::spawn(move || r.owner(r.key(1)).park_once().unwrap())
        };
        let mut stopped = r.stop().unwrap();
        stopped.trace();
        stopped.resume();
        native.join().unwrap();
        ordinary.join().unwrap();
        let s = r.snapshot();
        assert_eq!(s.entries[0].mode, Mode::NativeStopped);
        assert_eq!(s.entries[0].image_epoch, 1);
        assert_eq!(s.entries[0].image_takes, 0);
    });
}

#[test]
fn native_return_races_request_with_idle_resume_debt() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        r.enter_native(r.key(0)).unwrap();
        let native = {
            let r = r.clone();
            thread::spawn(move || {
                let owner = r.owner(r.key(0));
                if owner.return_native().unwrap() == 0 {
                    owner.park_once().unwrap();
                }
            })
        };
        let ordinary = {
            let r = r.clone();
            thread::spawn(move || r.owner(r.key(1)).park_once().unwrap())
        };
        let mut stopped = r.stop().unwrap();
        stopped.trace();
        stopped.resume();
        native.join().unwrap();
        ordinary.join().unwrap();
        assert_eq!(r.snapshot().phase, Phase::Idle);
        assert!(r.snapshot().entries.iter().all(|e| e.mode == Mode::Running));
    });
}

#[test]
fn native_sleeps_two_epochs_takes_latest_image_once() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        // Host-only syscall/latch completion, independent of GC notifications.
        // The native actor executes no heap-rendezvous wait or poll while asleep.
        let (wake_native, native_block) = loom::sync::mpsc::channel();
        let native = {
            let r = r.clone();
            thread::spawn(move || {
                r.enter_native(r.key(0)).unwrap();
                native_block.recv().unwrap();
                assert_eq!(r.owner(r.key(0)).return_native().unwrap(), 2);
            })
        };
        let ordinary = {
            let r = r.clone();
            thread::spawn(move || {
                let owner = r.owner(r.key(1));
                for _ in 0..2 {
                    owner.park_once().unwrap();
                }
            })
        };
        for _ in 0..2 {
            let mut stopped = r.stop().unwrap();
            stopped.trace();
            stopped.resume();
        }
        wake_native.send(()).unwrap();
        native.join().unwrap();
        ordinary.join().unwrap();
        let s = r.snapshot();
        assert_eq!(s.entries[0].image_takes, 1);
        assert_eq!(s.entries[0].folded, 1);
        assert_eq!(s.entries[0].image_epoch, 2);
        assert_eq!(s.traces, 2);
    });
}

#[test]
fn registration_race_freezes_membership() {
    check(2, || {
        let r = Arc::new(Rendezvous::new(2));
        let added = {
            let r = r.clone();
            thread::spawn(move || r.register().unwrap())
        };
        r.request().unwrap();
        let key = added.join().unwrap();
        let s = r.snapshot();
        if s.frozen.contains(&key) {
            assert_eq!(s.entries[key.slot].mode, Mode::Running);
        } else {
            assert_eq!(s.entries[key.slot].mode, Mode::StartingStopped);
            assert!(s.entries[key.slot].bootstrap_root);
            assert_eq!(s.frozen.len(), 2);
        }
        let frozen = s.frozen;
        assert_eq!(r.request().unwrap(), Request::Active(1));
        assert_eq!(r.snapshot().frozen, frozen);
    });
}

#[test]
fn retiring_owner_remains_rooted_through_collection() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        let retiring = {
            let r = r.clone();
            thread::spawn(move || r.retire(r.key(0)).unwrap())
        };
        let ordinary = {
            let r = r.clone();
            thread::spawn(move || r.owner(r.key(1)).park_once().unwrap())
        };
        let mut stopped = r.stop().unwrap();
        stopped.trace();
        stopped.resume();
        retiring.join().unwrap();
        ordinary.join().unwrap();
        r.finish_retire(r.key(0)).unwrap();
        let s = r.snapshot();
        assert_eq!(s.entries[0].mode, Mode::Retired);
        assert_eq!(s.entries[0].folded, s.entries[0].issued);
        assert!(s.entries[0].bootstrap_root);
    });
}

#[test]
fn failed_publication_and_loans_preserve_ack_state() {
    check(1, || {
        let r = Rendezvous::new(2);
        r.request().unwrap();
        r.set_worker_loan(r.key(0), true);
        let before = r.snapshot();
        assert_eq!(r.ack(r.key(0), true), Err(Refusal::Loan));
        assert_eq!(r.snapshot(), before);
        r.set_worker_loan(r.key(0), false);
        let before = r.snapshot();
        assert_eq!(r.ack(r.key(0), false), Err(Refusal::Publication));
        assert_eq!(r.snapshot(), before);
        for slot in 0..2 {
            r.ack(r.key(slot), true).unwrap();
        }
        let before = r.snapshot();
        assert_eq!(r.enter_native(r.key(0)), Err(Refusal::Busy));
        assert_eq!(r.retire(r.key(0)), Err(Refusal::Busy));
        assert_eq!(r.ack(r.key(0), true), Err(Refusal::Busy));
        assert_eq!(r.snapshot(), before);
        cycle(&r);
        for slot in 0..2 {
            restore(&r, slot);
        }
    });
}

#[test]
fn foreign_identity_and_generation_exhaustion_are_atomic_refusals() {
    check(1, || {
        let r = Rendezvous::configured(2, Fault::None, 1);
        r.request().unwrap();
        for change in 0..3 {
            let mut key = r.key(0);
            match change {
                0 => key.heap += 1,
                1 => key.world += 1,
                _ => key.generation += 1,
            }
            let before = r.snapshot();
            assert_eq!(r.ack(key, true), Err(Refusal::Identity));
            assert_eq!(r.snapshot(), before);
        }
        r.retire(r.key(0)).unwrap();
        r.enter_native(r.key(1)).unwrap();
        cycle(&r);
        r.finish_retire(r.key(0)).unwrap();
        let before = r.snapshot();
        assert_eq!(r.renew(0), Err(Refusal::Generation));
        assert_eq!(r.snapshot(), before);
        // Native return restores before a new publication; publication generation
        // exhaustion must retain the old record and running state unchanged.
        r.owner(r.key(1)).return_native().unwrap();
        let before = r.snapshot();
        assert_eq!(r.enter_native(r.key(1)), Err(Refusal::Generation));
        assert_eq!(r.snapshot(), before);
    });
}

#[test]
fn retired_slot_renewal_rejects_old_key() {
    check(1, || {
        let r = Rendezvous::new(2);
        let old = r.key(0);
        r.retire(old).unwrap();
        r.enter_native(r.key(1)).unwrap();
        cycle(&r);
        r.finish_retire(old).unwrap();
        let new = r.renew(0).unwrap();
        assert_ne!(old.generation, new.generation);
        let before = r.snapshot();
        assert_eq!(r.enter_native(old), Err(Refusal::Identity));
        assert_eq!(r.snapshot(), before);
    });
}

#[test]
fn poison_wakes_keeper_and_owner_without_resuming() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        let poisoner = {
            let r = r.clone();
            thread::spawn(move || {
                r.owner(r.key(0)).wait_request().unwrap();
                r.poison();
            })
        };
        let owner = {
            let r = r.clone();
            thread::spawn(move || {
                assert_eq!(r.owner(r.key(1)).park_once(), Err(Refusal::Poisoned));
            })
        };
        assert!(matches!(r.stop(), Err(Refusal::Poisoned)));
        poisoner.join().unwrap();
        owner.join().unwrap();
        let s = r.snapshot();
        assert_eq!(s.phase, Phase::Poisoned);
        assert_eq!(s.traces, 0);
        assert!(s.entries.iter().all(|e| e.issued == 1 && e.folded == 0));
        assert_eq!(r.register(), Err(Refusal::Poisoned));
    });
}

#[test]
fn early_and_redundant_notifications_are_hints() {
    check(3, || {
        let r = Arc::new(Rendezvous::new(2));
        r.request().unwrap();
        r.notify();
        r.notify();
        let workers: Vec<_> = (0..2)
            .map(|slot| {
                let r = r.clone();
                thread::spawn(move || r.owner(r.key(slot)).park_once().unwrap())
            })
            .collect();
        let mut stopped = r.stop().unwrap();
        stopped.trace();
        stopped.resume();
        for w in workers {
            w.join().unwrap();
        }
        assert_eq!(r.snapshot().phase, Phase::Idle);
    });
}

#[test]
fn unfinished_collector_poison_retains_images() {
    check(1, || {
        let r = Rendezvous::new(2);
        for slot in 0..2 {
            r.enter_native(r.key(slot)).unwrap();
        }
        r.request().unwrap();
        drop(r.try_stop().unwrap().unwrap());
        let s = r.snapshot();
        assert_eq!(s.phase, Phase::Poisoned);
        assert_eq!(s.completed, 0);
        assert!(s.entries.iter().all(|e| e.image_resident));
    });
}

fn bad_ack(fault: Fault, witness: &'static str, trace: bool) {
    negative(1, witness, move || {
        let r = Rendezvous::configured(2, fault, u8::MAX);
        r.request().unwrap();
        if fault == Fault::IgnoreLoan {
            r.set_worker_loan(r.key(0), true);
        }
        for slot in 0..2 {
            r.ack(r.key(slot), true).unwrap();
        }
        let mut stopped = r.try_stop().unwrap().unwrap();
        if trace {
            stopped.trace();
        }
    });
}

#[test]
fn negative_ack_before_roots() {
    bad_ack(Fault::AckBeforeRoots, "ROOT_COVERAGE", false);
}
#[test]
fn negative_ack_before_logs() {
    bad_ack(Fault::AckBeforeLogs, "LOG_COVERAGE", true);
}
#[test]
fn negative_outstanding_loan() {
    bad_ack(Fault::IgnoreLoan, "OUTSTANDING_LOAN", false);
}
#[test]
fn negative_stale_registration() {
    bad_ack(Fault::StaleRegistration, "STALE_REGISTRATION", false);
}
#[test]
fn negative_stale_ack_epoch() {
    bad_ack(Fault::StaleEpoch, "STALE_ACK_EPOCH", false);
}

#[test]
fn negative_native_ack_before_roots() {
    negative(1, "ROOT_COVERAGE", || {
        let r = Rendezvous::configured(2, Fault::NativeBeforeRoots, u8::MAX);
        for slot in 0..2 {
            r.enter_native(r.key(slot)).unwrap();
        }
        r.request().unwrap();
        let _ = r.try_stop();
    });
}

#[test]
fn negative_stop_ignores_resume_debt() {
    negative(1, "RESTORATION_DURING_STOP", || {
        let r = Rendezvous::configured(2, Fault::IgnoreResumeDebt, u8::MAX);
        r.request().unwrap();
        for slot in 0..2 {
            r.ack(r.key(slot), true).unwrap();
        }
        cycle(&r);
        r.begin_restore(r.key(0), false).unwrap();
        r.request().unwrap();
    });
}

#[test]
fn negative_native_return_omits_idle_resume_debt() {
    negative(1, "RESTORATION_DURING_STOP", || {
        let r = Rendezvous::configured(2, Fault::NativeReturnWithoutDebt, u8::MAX);
        r.enter_native(r.key(0)).unwrap();
        r.begin_restore(r.key(0), true).unwrap();
        r.request().unwrap();
    });
}

#[test]
fn negative_native_image_log_refolded() {
    negative(1, "DUPLICATE_LOG_FOLD", || {
        let r = Rendezvous::configured(2, Fault::RefoldNativeImage, u8::MAX);
        for slot in 0..2 {
            r.enter_native(r.key(slot)).unwrap();
        }
        cycle(&r);
        cycle(&r);
    });
}

#[test]
fn negative_retirement_drops_pending_log() {
    negative(1, "LOG_COVERAGE", || {
        let r = Rendezvous::configured(2, Fault::DropRetiringLog, u8::MAX);
        r.retire(r.key(0)).unwrap();
        r.enter_native(r.key(1)).unwrap();
        cycle(&r);
    });
}

#[test]
fn negative_retirement_removes_frozen_participant() {
    negative(1, "FROZEN_OWNER_REMOVED", || {
        let r = Rendezvous::configured(2, Fault::RemoveFrozenOwner, u8::MAX);
        r.request().unwrap();
        r.retire(r.key(0)).unwrap();
        r.ack(r.key(1), true).unwrap();
        let _ = r.try_stop();
    });
}

#[test]
fn negative_stale_publication_generation() {
    bad_ack(Fault::StalePublication, "STALE_PUBLICATION", false);
}

#[test]
fn negative_native_return_skips_active_stop_gate() {
    negative(1, "NATIVE_RETURN_DURING_STOP", || {
        let r = Rendezvous::configured(2, Fault::NativeReturnWithoutGate, u8::MAX);
        for slot in 0..2 {
            r.enter_native(r.key(slot)).unwrap();
        }
        r.request().unwrap();
        let mut stopped = r.try_stop().unwrap().unwrap();
        stopped.trace();
        r.begin_restore(r.key(0), true).unwrap();
    });
}

#[test]
fn collector_finishes_every_reader_before_resume() {
    check(1, || {
        let r = Rendezvous::new(2);
        for slot in 0..2 {
            r.enter_native(r.key(slot)).unwrap();
        }
        r.request().unwrap();
        let mut stopped = r.try_stop().unwrap().unwrap();
        r.begin_reader();
        r.begin_reader();
        stopped.trace();
        r.finish_reader();
        r.finish_reader();
        stopped.resume();
        assert_eq!(r.snapshot().phase, Phase::Idle);
    });
}

#[test]
fn negative_collector_resumes_with_reader_alive() {
    negative(1, "READER_STILL_ACTIVE", || {
        let r = Rendezvous::new(2);
        for slot in 0..2 {
            r.enter_native(r.key(slot)).unwrap();
        }
        r.request().unwrap();
        let mut stopped = r.try_stop().unwrap().unwrap();
        r.begin_reader();
        stopped.trace();
        stopped.resume();
    });
}
