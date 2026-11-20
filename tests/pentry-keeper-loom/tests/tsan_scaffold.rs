//! SCAFFOLD real-thread doubles. Not the E2 gate until pentry's L6 surface
//! replaces this ticket. Reported as scaffold green, not gate green.
#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, PartialEq, Eq)]
enum Stop {
    Busy,
}

fn self_stop(keeper: thread::ThreadId, caller: thread::ThreadId) -> Result<(), Stop> {
    if keeper == caller {
        Err(Stop::Busy)
    } else {
        Ok(())
    }
}

#[test]
fn self_stop_vs_main() {
    let main = thread::current().id();
    let keeper = thread::spawn(move || thread::current().id())
        .join()
        .unwrap();
    assert_ne!(main, keeper);
    assert_eq!(self_stop(main, main), Err(Stop::Busy));
    assert_eq!(self_stop(keeper, main), Ok(()));
    eprintln!("PENTRY_TSAN case=self_stop_vs_main kind=SCAFFOLD");
}

#[test]
fn nested_serial_errors() {
    let ticket = Arc::new(AtomicU8::new(1));
    let inner = ticket.clone();
    let worker = thread::spawn(move || {
        assert_eq!(inner.load(Ordering::Acquire), 1);
        inner.fetch_add(0, Ordering::AcqRel);
        Err::<(), &'static str>("nested")
    });
    assert!(worker.join().unwrap().is_err());
    assert_eq!(ticket.load(Ordering::Acquire), 1);
    ticket.store(0, Ordering::Release);
    assert_eq!(ticket.load(Ordering::Acquire), 0);
    eprintln!("PENTRY_TSAN case=nested_serial_errors kind=SCAFFOLD");
}

#[test]
fn retained_loan_refusal() {
    let views_live = Arc::new(AtomicBool::new(true));
    let open = Arc::new(Mutex::new(false));
    let flag = views_live.clone();
    let gate = open.clone();
    let holder = thread::spawn(move || {
        flag.store(false, Ordering::Release);
    });
    holder.join().unwrap();
    let mut admission = gate.lock().unwrap();
    if views_live.load(Ordering::Acquire) {
        eprintln!("PENTRY_TSAN case=retained_loan_refusal kind=SCAFFOLD refused=true");
    } else {
        *admission = true;
        eprintln!("PENTRY_TSAN case=retained_loan_refusal kind=SCAFFOLD refused=false");
    }
    assert!(!*admission || !views_live.load(Ordering::Acquire));
    assert!(!views_live.load(Ordering::Acquire));
    assert!(*admission);
}
