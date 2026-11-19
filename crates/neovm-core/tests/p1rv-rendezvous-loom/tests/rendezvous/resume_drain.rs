//! Owner run admission parks only for other owners' outstanding resume debt.
//!
//! This extends the independent control model, not the production VM or its
//! atomic admission implementation. No time bound or successful-body retry
//! supplies progress: only the existing debt/poison condition variable does.

use super::{Fault, Mode, Phase, Refusal, Rendezvous, cycle, restore, support};
use loom::sync::{Arc, mpsc};
use loom::thread;

fn first_owner_restored(fault: Fault) -> Rendezvous {
    let r = Rendezvous::configured(2, fault, u8::MAX);
    r.request().unwrap();
    for slot in 0..2 {
        r.ack(r.key(slot), true).unwrap();
    }
    cycle(&r);
    restore(&r, 0);
    let s = r.snapshot();
    assert_eq!(s.phase, Phase::DrainingResumes);
    assert_eq!(s.debt, 1 << 1);
    assert_eq!(s.entries[0].mode, Mode::Running);
    assert!(s.entries[0].restored && !s.entries[0].image_resident);
    assert!(s.entries[1].image_resident);
    r
}

fn assert_admitted(r: &Rendezvous) {
    // Taking the gate here also proves that returning admission released it.
    let s = r.snapshot();
    assert_eq!(s.phase, Phase::Idle);
    assert_eq!(s.debt, 0);
    assert_eq!(s.epoch, 1);
    assert_eq!(s.traces, 1);
    assert!(s.entries[0].run_loan);
    for e in &s.entries {
        assert_eq!(e.mode, Mode::Running);
        assert!(e.restored && !e.image_resident);
        assert_eq!(e.image_takes, 1);
    }
}

#[test]
fn restored_owner_admission_races_final_peer_debt_completion() {
    support::check(2, || {
        let r = Arc::new(first_owner_restored(Fault::None));
        let final_r = Arc::clone(&r);
        let final_owner = thread::spawn(move || restore(&final_r, 1));
        let owner = r.owner(r.key(0));
        let run = owner.run_access_after_resume_drain().unwrap();
        assert_admitted(&r);
        drop(run);
        final_owner.join().unwrap();
        assert!(!r.snapshot().entries[0].run_loan);
    });
}

fn parked_history(fault: Fault) {
    let r = Arc::new(first_owner_restored(fault));
    let (waiting, observed_wait) = mpsc::channel();
    let final_r = Arc::clone(&r);
    let final_owner = thread::spawn(move || {
        observed_wait.recv().unwrap();
        restore(&final_r, 1);
    });
    let owner = r.owner(r.key(0));
    let run = owner
        .run_access_observing_wait(|| {
            // Sent after observing the actual predicate, while holding its
            // mutex. The peer cannot mutate debt until atomic wait releases it.
            waiting.send(()).unwrap();
        })
        .unwrap();
    assert_admitted(&r);
    drop(run);
    final_owner.join().unwrap();
    assert!(!r.snapshot().entries[0].run_loan);
}

#[test]
fn parked_restored_owner_is_woken_by_final_peer_debt_completion() {
    support::check(2, || parked_history(Fault::None));
}

#[test]
fn completion_before_admission_needs_no_remembered_notification() {
    support::check(1, || {
        let r = first_owner_restored(Fault::None);
        restore(&r, 1);
        // Both notifications happened before this attempt. The Idle predicate
        // supplies authority; condvar notifications are not stored permits.
        let owner = r.owner(r.key(0));
        let run = owner
            .run_access_observing_wait(|| panic!("Idle admission unexpectedly parked"))
            .unwrap();
        assert_admitted(&r);
        drop(run);
    });
}

#[test]
fn admission_refuses_stopping_and_tracing_without_waiting_or_mutation() {
    support::check(1, || {
        let r = Rendezvous::new(2);
        r.request().unwrap();
        let before = r.snapshot();
        assert_eq!(before.phase, Phase::Stopping(1));
        assert!(matches!(
            r.owner(r.key(0))
                .run_access_observing_wait(|| panic!("Stopping owner must ACK, not wait")),
            Err(Refusal::Busy)
        ));
        assert_eq!(r.snapshot(), before);
        for slot in 0..2 {
            r.ack(r.key(slot), true).unwrap();
        }
        let mut stopped = r.try_stop().unwrap().unwrap();
        let before = r.snapshot();
        assert_eq!(before.phase, Phase::Tracing(1));
        assert!(matches!(
            r.owner(r.key(0))
                .run_access_observing_wait(|| panic!("Tracing owner cannot run or wait")),
            Err(Refusal::Busy)
        ));
        assert_eq!(r.snapshot(), before);
        stopped.trace();
        stopped.resume();
        restore(&r, 0);
        restore(&r, 1);
    });
}

#[test]
fn admission_cannot_wait_for_its_own_unrestored_image() {
    support::check(1, || {
        let r = first_owner_restored(Fault::None);
        let before = r.snapshot();
        assert!(matches!(
            r.owner(r.key(1))
                .run_access_observing_wait(|| panic!("owner must finish its own restore first")),
            Err(Refusal::Busy)
        ));
        assert_eq!(r.snapshot(), before);
        restore(&r, 1);
    });
}

#[test]
fn poison_wakes_restored_owner_waiting_for_peer_resume_debt() {
    support::check(2, || {
        let r = Arc::new(first_owner_restored(Fault::None));
        let (waiting, observed_wait) = mpsc::channel();
        let poison_r = Arc::clone(&r);
        let poisoner = thread::spawn(move || {
            observed_wait.recv().unwrap();
            poison_r.poison();
        });
        let owner = r.owner(r.key(0));
        assert!(matches!(
            owner.run_access_observing_wait(|| waiting.send(()).unwrap()),
            Err(Refusal::Poisoned)
        ));
        poisoner.join().unwrap();
        let s = r.snapshot();
        assert_eq!(s.phase, Phase::Poisoned);
        assert_eq!(s.debt, 1 << 1);
        assert!(!s.entries[0].run_loan);
        assert_eq!(s.entries[0].image_takes, 1);
        assert_eq!(s.entries[1].image_takes, 0);
        assert!(s.entries[1].image_resident);
    });
}

/// Capture Loom's real blocked-thread panic before unwinding its still-blocked
/// runtime can cause a second destructor panic. This is the isolated-child
/// pattern used by the intern read-domain model; no modeled actor invents a
/// deadlock message or converts an invariant assertion into the witness.
#[test]
fn negative_omitting_final_debt_notification_strands_parked_owner() {
    const WITNESS_EXIT: i32 = 73;
    const UNRELATED_PANIC_EXIT: i32 = 74;
    const CHILD: &str = "P1RV_RESUME_DRAIN_DEADLOCK_CHILD";
    const HEADING: &str = "deadlock; threads = ";
    const RECEIPT: &str = "LOOM_RESUME_DRAIN_DEADLOCK ";
    let current = std::thread::current();
    let name = current.name().expect("named libtest actor");
    if std::env::var(CHILD).ok().as_deref() == Some(name) {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = info
                .payload()
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| info.payload().downcast_ref::<&str>().copied());
            if let Some(message) = message.filter(|text| text.starts_with(HEADING)) {
                eprintln!("{RECEIPT}{message}");
                std::process::exit(WITNESS_EXIT);
            }
            previous(info);
            // Reject the first unrelated panic before unwinding blocked Loom
            // actors can produce a secondary deadlock/destructor diagnostic.
            std::process::exit(UNRELATED_PANIC_EXIT);
        }));
        support::check(2, || parked_history(Fault::OmitDebtNotification));
        panic!("missing debt notification unexpectedly completed");
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(CHILD, name)
        .output()
        .expect("run isolated lost-notification control");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 Loom diagnostic");
    assert_eq!(output.status.code(), Some(WITNESS_EXIT), "{stderr}");
    let witness = stderr
        .lines()
        .find_map(|line| line.strip_prefix(RECEIPT))
        .expect("child must capture Loom's original deadlock panic");
    assert!(witness.starts_with(HEADING));
    assert_eq!(witness.matches("Blocked(").count(), 1, "{witness}");
    assert_eq!(witness.matches("Terminated").count(), 1, "{witness}");
    eprintln!("{RECEIPT}{witness}");
}
