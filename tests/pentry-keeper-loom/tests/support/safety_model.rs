//! MODEL of DESIGN §9 E2 admission, serial-versus-GC, and loan retire.
//! Not product code. `tests/seam.rs` pins the landed edges this model copies.
#![forbid(unsafe_code)]

use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

fn trace() -> &'static std::sync::Mutex<Vec<&'static str>> {
    static TRACE: OnceLock<std::sync::Mutex<Vec<&'static str>>> = OnceLock::new();
    TRACE.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn trace_clear() {
    trace().lock().unwrap().clear();
}

fn trace_note(event: &'static str) {
    trace().lock().unwrap().push(event);
}

fn trace_join() -> String {
    trace().lock().unwrap().join(" > ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Case {
    Admission,
    SerialVsGc,
    LoanRetire,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Required,
    OmitAllStopped,
    OmitOverlapRefusal,
    OmitRetainedCheck,
}

struct State {
    published: [bool; 2],
    views_live: bool,
    ticket_out: bool,
    serial_collecting: bool,
    attached_gen: u8,
    gc_saw_gen: Option<u8>,
    keeper_overlapped: bool,
    admission_open: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn new(case: Case) -> Self {
        Self {
            state: Mutex::new(State {
                published: [false, false],
                views_live: case == Case::LoanRetire,
                ticket_out: false,
                serial_collecting: false,
                attached_gen: 0,
                gc_saw_gen: None,
                keeper_overlapped: false,
                admission_open: false,
            }),
            changed: Condvar::new(),
        }
    }
}

fn publish(shared: &Arc<Shared>, owner: usize) {
    let mut state = shared.state.lock().unwrap();
    state.published[owner] = true;
    shared.changed.notify_all();
    trace_note(match owner {
        0 => "owner0-publish",
        1 => "owner1-publish",
        _ => "owner-publish",
    });
}

fn admission(shared: &Arc<Shared>, edge: Edge) {
    if edge == Edge::OmitAllStopped {
        let state = shared.state.lock().unwrap();
        trace_note(match state.published {
            [false, false] => "keeper-admit-before-either-publish",
            [true, false] => "keeper-admit-after-owner0-only",
            [false, true] => "keeper-admit-after-owner1-only",
            [true, true] => "keeper-admit-after-both-published",
        });
        assert!(
            state.published.iter().all(|published| *published),
            "ADMISSION_WHILE_RUNNING"
        );
        return;
    }
    let mut state = shared.state.lock().unwrap();
    while state.published.iter().any(|published| !*published) {
        state = shared.changed.wait(state).unwrap();
    }
    assert!(
        state.published.iter().all(|published| *published),
        "ADMISSION_WHILE_RUNNING"
    );
}

fn serial_vs_gc(shared: &Arc<Shared>, _edge: Edge) {
    {
        let mut state = shared.state.lock().unwrap();
        state.ticket_out = true;
        state.attached_gen = 1;
        state.serial_collecting = true;
        shared.changed.notify_all();
    }
    trace_note("serial-ticket-out-collecting");
    let mut state = shared.state.lock().unwrap();
    state.gc_saw_gen = Some(state.attached_gen);
    assert_eq!(state.gc_saw_gen, Some(1), "STALE_ENTRY_SNAPSHOT");
    state.serial_collecting = false;
    state.ticket_out = false;
    shared.changed.notify_all();
}

fn keeper_collect(shared: &Arc<Shared>, edge: Edge) {
    let mut state = shared.state.lock().unwrap();
    loop {
        if state.serial_collecting && edge == Edge::OmitOverlapRefusal {
            state.keeper_overlapped = true;
            trace_note("keeper-collects-during-serial-ticket");
            assert!(!state.keeper_overlapped, "KEEPER_COLLECTS_WHILE_TICKET_OUT");
        }
        if state.gc_saw_gen.is_some() && !state.serial_collecting {
            break;
        }
        state = shared.changed.wait(state).unwrap();
    }
    assert!(!state.keeper_overlapped, "KEEPER_COLLECTS_WHILE_TICKET_OUT");
    assert_eq!(state.gc_saw_gen, Some(1), "STALE_ENTRY_SNAPSHOT");
}

fn end_views(shared: &Arc<Shared>) {
    let mut state = shared.state.lock().unwrap();
    state.views_live = false;
    shared.changed.notify_all();
    trace_note("holder-end-views");
}

fn retire(shared: &Arc<Shared>, edge: Edge) {
    if edge == Edge::OmitRetainedCheck {
        let mut state = shared.state.lock().unwrap();
        trace_note(if state.views_live {
            "retire-reopen-while-views-live"
        } else {
            "retire-reopen-after-views-ended"
        });
        assert!(!state.views_live, "REOPEN_WITH_RETAINED_LOAN");
        state.ticket_out = false;
        state.admission_open = true;
        return;
    }
    let mut state = shared.state.lock().unwrap();
    while state.views_live {
        state = shared.changed.wait(state).unwrap();
    }
    assert!(!state.views_live, "REOPEN_WITH_RETAINED_LOAN");
    state.ticket_out = false;
    state.admission_open = true;
}

fn propagate(result: std::thread::Result<()>) {
    if let Err(failure) = result {
        std::panic::resume_unwind(failure);
    }
}

fn explore(case: Case, edge: Edge, runs: std::sync::Arc<AtomicUsize>) {
    let bound = std::env::var("PENTRY_PREEMPTIONS")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("PENTRY_PREEMPTIONS must be an integer")
        })
        .unwrap_or(2);
    assert!([2, 3].contains(&bound), "qualify PENTRY_PREEMPTIONS=2 or 3");
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(bound);
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.max_threads = 4;
    builder.max_branches = 10_000;
    let tally = runs.clone();
    builder.check(move || {
        tally.fetch_add(1, Ordering::Relaxed);
        trace_clear();
        let shared = Arc::new(Shared::new(case));
        match case {
            Case::Admission => {
                let first = shared.clone();
                let second = shared.clone();
                let keeper = shared.clone();
                let owner0 = thread::spawn(move || publish(&first, 0));
                let owner1 = thread::spawn(move || publish(&second, 1));
                let keeper = thread::spawn(move || admission(&keeper, edge));
                propagate(owner0.join());
                propagate(owner1.join());
                propagate(keeper.join());
            }
            Case::SerialVsGc => {
                let serial = shared.clone();
                let keeper = shared.clone();
                let serial = thread::spawn(move || serial_vs_gc(&serial, edge));
                let keeper = thread::spawn(move || keeper_collect(&keeper, edge));
                propagate(serial.join());
                propagate(keeper.join());
                let state = shared.state.lock().unwrap();
                assert_eq!(state.gc_saw_gen, Some(1));
                assert!(!state.keeper_overlapped);
                assert!(!state.ticket_out);
            }
            Case::LoanRetire => {
                let holder = shared.clone();
                let retire_thread = shared.clone();
                let holder = thread::spawn(move || end_views(&holder));
                let retire_thread = thread::spawn(move || retire(&retire_thread, edge));
                propagate(holder.join());
                propagate(retire_thread.join());
                let state = shared.state.lock().unwrap();
                assert!(!state.views_live);
                assert!(state.admission_open);
            }
        }
    });
    let count = runs.load(Ordering::Relaxed);
    assert!(count > 0);
    println!("PENTRY_MODEL case={case:?} bound={bound} executions={count} kind=MODEL");
}

pub(crate) fn positive(case: Case) {
    explore(
        case,
        Edge::Required,
        std::sync::Arc::new(AtomicUsize::new(0)),
    );
}

pub(crate) fn negative(case: Case, edge: Edge, witness: &str) {
    let runs = std::sync::Arc::new(AtomicUsize::new(0));
    let tally = runs.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        explore(case, edge, tally);
    }));
    let failure = result.expect_err("one-edge negative control unexpectedly passed");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic");
    assert!(
        message.contains(witness),
        "wrong negative witness: expected {witness}, got {message}"
    );
    let count = runs.load(Ordering::Relaxed);
    assert!(
        count > 1,
        "{witness} must pass execution 1 and fail only during exploration, got {count}"
    );
    let interleaving = trace_join();
    let marker = match edge {
        Edge::OmitAllStopped => "keeper-admit-",
        Edge::OmitOverlapRefusal => "keeper-collects-during-serial-ticket",
        Edge::OmitRetainedCheck => "retire-reopen-while-views-live",
        Edge::Required => "required-edge-has-no-negative",
    };
    assert!(
        interleaving.contains(marker)
            && !interleaving.contains("after-both-published")
            && !interleaving.contains("after-views-ended"),
        "{witness} interleaving did not record its own failing edge: {interleaving}"
    );
    println!(
        "PENTRY_NEGATIVE witness={witness} outcome=detected edge={edge:?} executions_to_witness={count} interleaving={interleaving} kind=MODEL"
    );
}
