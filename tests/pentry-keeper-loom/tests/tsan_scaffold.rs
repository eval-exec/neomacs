//! SCAFFOLD real-thread doubles. The product retarget is
//! `serial_gate_retarget` in neovm-eval. These doubles are not the gate.
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

/// I5 refusal. The holder keeps the view and never retires it, so reopen
/// stays closed. The same split applies when this retargets onto L6.
#[test]
fn retained_loan_is_refused() {
    let views_live = Arc::new(AtomicBool::new(true));
    let open = Arc::new(Mutex::new(false));
    let flag = views_live.clone();
    let holder = thread::spawn(move || {
        assert!(flag.load(Ordering::Acquire));
    });
    holder.join().unwrap();
    let admission = open.lock().unwrap();
    let still_live = views_live.load(Ordering::Acquire);
    assert!(still_live);
    assert!(!*admission);
    eprintln!("PENTRY_TSAN case=retained_loan_is_refused kind=SCAFFOLD refused=true");
}

/// Release path: the holder ends the view, then admission may reopen.
#[test]
fn retired_loan_reopens() {
    let views_live = Arc::new(AtomicBool::new(true));
    let open = Arc::new(Mutex::new(false));
    let flag = views_live.clone();
    let gate = open.clone();
    let holder = thread::spawn(move || {
        flag.store(false, Ordering::Release);
    });
    holder.join().unwrap();
    let mut admission = gate.lock().unwrap();
    let ended = !views_live.load(Ordering::Acquire);
    assert!(ended);
    *admission = true;
    assert!(*admission);
    eprintln!("PENTRY_TSAN case=retired_loan_reopens kind=SCAFFOLD refused=false");
}
