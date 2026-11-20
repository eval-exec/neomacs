//! Executable stop/ticket contract, not an implementation of heap borrowing.
#![forbid(unsafe_code)]

use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnerId(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TicketId(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Epoch {
    Stop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Roots {
    Entry,
    SerialFirst,
    SerialSecond,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerMode {
    Running,
    Published,
    Serial,
    Failed,
    Resumed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Open,
    Stopping,
    Draining,
    Poisoned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeapLoan {
    Ended,
    Local,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Views {
    Ended,
    Outer,
    Nested,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transfer {
    Outbound,
    OnOwner,
    Returned,
    RetainedFailure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Publication {
    owner: OwnerId,
    epoch: Epoch,
    roots: Roots,
}
#[derive(Debug)]
struct Ticket {
    owner: OwnerId,
    id: TicketId,
    transfer: Transfer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnedFailure {
    owner: OwnerId,
    ticket: TicketId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    StopPublication,
    TicketRoundtrip,
    SerialCollection,
    TwoRequests,
    MidTransferFailure,
    RestoreFailure,
    SelfStopMain,
    NestedSerial,
}
impl Case {
    fn owners(self) -> usize {
        match self {
            Self::StopPublication
            | Self::SerialCollection
            | Self::TwoRequests
            | Self::SelfStopMain => 2,
            Self::TicketRoundtrip
            | Self::MidTransferFailure
            | Self::RestoreFailure
            | Self::NestedSerial => 1,
        }
    }
    fn requests(self) -> usize {
        match self {
            Self::StopPublication => 0,
            Self::TwoRequests => 2,
            Self::TicketRoundtrip
            | Self::SerialCollection
            | Self::MidTransferFailure
            | Self::RestoreFailure
            | Self::SelfStopMain
            | Self::NestedSerial => 1,
        }
    }
}

/// Each control removes or reverses one required protocol ordering edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Required,
    OmitStoppedWait,
    OmitKeeperLoanEnd,
    OmitCurrentRoots,
    OmitUniqueTicketWait,
    OmitPoisonResumeFence,
    OmitViewEnd,
    AckBeforePublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationStep {
    Roots,
    Ack,
}

#[derive(Debug)]
struct State {
    phase: Phase,
    modes: Vec<OwnerMode>,
    publications: Vec<Option<Publication>>,
    current_roots: Vec<Roots>,
    heap_loans: Vec<HeapLoan>,
    views: Vec<Views>,
    keeper_loan: HeapLoan,
    queue: Vec<OwnerId>,
    tickets: Vec<Ticket>,
    failure: Option<OwnedFailure>,
    serviced: Vec<OwnerId>,
    submitted: Vec<OwnerId>,
}
impl State {
    fn new(case: Case) -> Self {
        Self {
            phase: Phase::Open,
            modes: vec![OwnerMode::Running; case.owners()],
            publications: vec![None; case.owners()],
            current_roots: vec![Roots::Entry; case.owners()],
            heap_loans: vec![HeapLoan::Ended; case.owners()],
            views: vec![Views::Ended; case.owners()],
            keeper_loan: HeapLoan::Local,
            queue: Vec::new(),
            tickets: Vec::new(),
            failure: None,
            serviced: Vec::new(),
            submitted: Vec::new(),
        }
    }
    fn all_published(&self) -> bool {
        self.modes.iter().all(|mode| *mode == OwnerMode::Published)
    }
    fn outstanding(&self) -> usize {
        self.tickets
            .iter()
            .filter(|ticket| ticket.transfer != Transfer::Returned)
            .count()
    }
    fn check_ticket(&self) {
        assert!(
            self.modes.iter().all(|mode| *mode != OwnerMode::Running),
            "TICKET_WHILE_RUNNING"
        );
        assert_eq!(
            self.keeper_loan,
            HeapLoan::Ended,
            "KEEPER_TOUCHES_HEAP_WITH_TICKET_OUT"
        );
        assert_eq!(self.outstanding(), 1, "DOUBLE_TICKET");
    }
    fn keeper_heap_access(&self) {
        assert_eq!(self.outstanding(), 0, "KEEPER_TOUCHES_HEAP_WITH_TICKET_OUT");
        assert_eq!(self.keeper_loan, HeapLoan::Local);
    }
    fn resume(&mut self) {
        assert!(self.failure.is_none(), "RESUME_AFTER_FAILED_RESTORE");
        assert_eq!(self.outstanding(), 0);
        assert!(self.heap_loans.iter().all(|loan| *loan == HeapLoan::Ended));
        self.phase = Phase::Draining;
        for mode in &mut self.modes {
            *mode = OwnerMode::Resumed;
        }
    }
}
struct Shared {
    state: Mutex<State>,
    keeper_changed: Condvar,
    owner_changed: [Condvar; 2],
}
impl Shared {
    fn new(case: Case) -> Self {
        Self {
            state: Mutex::new(State::new(case)),
            keeper_changed: Condvar::new(),
            owner_changed: [Condvar::new(), Condvar::new()],
        }
    }
    fn wake_owners(&self) {
        for changed in &self.owner_changed {
            changed.notify_all();
        }
    }
}

fn owner(shared: Arc<Shared>, id: OwnerId, case: Case, edge: Edge) {
    let mut state = shared.state.lock().unwrap();
    if id.0 < case.requests() {
        state.queue.push(id);
        state.submitted.push(id);
        shared.keeper_changed.notify_one();
    }
    while state.phase == Phase::Open {
        state = shared.owner_changed[id.0].wait(state).unwrap();
    }
    // Root publication and acknowledgment are separate locked steps in BOTH
    // protocols. Only their order changes in the A5 negative. No yield, run
    // counter, or schedule-dependent branch manufactures the counterexample.
    let steps = if edge == Edge::AckBeforePublication {
        [PublicationStep::Ack, PublicationStep::Roots]
    } else {
        [PublicationStep::Roots, PublicationStep::Ack]
    };
    for step in steps {
        match step {
            PublicationStep::Roots => {
                state.publications[id.0] = Some(Publication {
                    owner: id,
                    epoch: Epoch::Stop,
                    roots: state.current_roots[id.0],
                });
            }
            PublicationStep::Ack => {
                state.modes[id.0] = OwnerMode::Published;
                shared.keeper_changed.notify_one();
            }
        }
        drop(state);
        state = shared.state.lock().unwrap();
    }
    loop {
        if state.phase == Phase::Draining || state.phase == Phase::Poisoned {
            break;
        }
        if let Some(index) = state
            .tickets
            .iter()
            .position(|ticket| ticket.owner == id && ticket.transfer == Transfer::Outbound)
        {
            state.check_ticket();
            if case == Case::MidTransferFailure {
                state.tickets[index].transfer = Transfer::RetainedFailure;
                state.failure = Some(OwnedFailure {
                    owner: id,
                    ticket: state.tickets[index].id,
                });
                state.modes[id.0] = OwnerMode::Failed;
                state.phase = Phase::Poisoned;
                shared.keeper_changed.notify_one();
                shared.wake_owners();
                break;
            }
            state.tickets[index].transfer = Transfer::OnOwner;
            state.heap_loans[id.0] = HeapLoan::Local;
            state.modes[id.0] = OwnerMode::Serial;
            state.views[id.0] = Views::Outer;
            if case == Case::NestedSerial {
                // Nested reborrow uses the same ticket; it never queues another stop.
                state.views[id.0] = Views::Nested;
                assert_eq!(state.outstanding(), 1);
                state.views[id.0] = Views::Outer;
            }
            if case == Case::SerialCollection {
                for current in [Roots::SerialFirst, Roots::SerialSecond] {
                    state.current_roots[id.0] = current;
                    let roots = match edge {
                        Edge::OmitCurrentRoots => state.publications[id.0].unwrap().roots,
                        Edge::Required
                        | Edge::OmitStoppedWait
                        | Edge::OmitKeeperLoanEnd
                        | Edge::OmitUniqueTicketWait
                        | Edge::OmitPoisonResumeFence
                        | Edge::OmitViewEnd
                        | Edge::AckBeforePublication => state.current_roots[id.0],
                    };
                    assert_eq!(
                        roots, state.current_roots[id.0],
                        "STALE_PUBLICATION_USED_FOR_SERIAL_GC"
                    );
                    // GC scans current attached roots, plus frozen peers and World.
                    state.check_ticket();
                    for (peer, publication) in state.publications.iter().enumerate() {
                        if peer != id.0 {
                            assert_eq!(publication.unwrap().epoch, Epoch::Stop);
                        }
                    }
                }
            }
            if case == Case::RestoreFailure {
                state.tickets[index].transfer = Transfer::RetainedFailure;
                state.failure = Some(OwnedFailure {
                    owner: id,
                    ticket: state.tickets[index].id,
                });
                state.modes[id.0] = OwnerMode::Failed;
                state.phase = Phase::Poisoned;
                shared.keeper_changed.notify_one();
                shared.wake_owners();
                break;
            }
            if edge != Edge::OmitViewEnd {
                state.views[id.0] = Views::Ended;
            }
            assert_eq!(state.views[id.0], Views::Ended, "VIEW_OUTLIVES_GUARD");
            // Loan destruction precedes the return message; current roots are republished.
            state.heap_loans[id.0] = HeapLoan::Ended;
            state.publications[id.0] = Some(Publication {
                owner: id,
                epoch: Epoch::Stop,
                roots: state.current_roots[id.0],
            });
            state.modes[id.0] = OwnerMode::Published;
            state.tickets[index].transfer = Transfer::Returned;
            state.serviced.push(id);
            shared.keeper_changed.notify_one();
        }
        state = shared.owner_changed[id.0].wait(state).unwrap();
    }
}

fn keeper(shared: Arc<Shared>, case: Case, edge: Edge) {
    let mut state = shared.state.lock().unwrap();
    if case.requests() != 0 {
        while state.queue.is_empty() {
            state = shared.keeper_changed.wait(state).unwrap();
        }
    }
    state.phase = Phase::Stopping;
    shared.wake_owners();
    if edge != Edge::OmitStoppedWait {
        while !state.all_published() {
            state = shared.keeper_changed.wait(state).unwrap();
        }
        for (index, publication) in state.publications.iter().enumerate() {
            assert_eq!(
                *publication,
                Some(Publication {
                    owner: OwnerId(index),
                    epoch: Epoch::Stop,
                    roots: Roots::Entry
                }),
                "ACK_BEFORE_PUBLICATION"
            );
        }
    }
    if case.requests() == 0 {
        state.keeper_heap_access();
    }
    for request in 0..case.requests() {
        while state.queue.is_empty() {
            state = shared.keeper_changed.wait(state).unwrap();
        }
        let id = state.queue.remove(0);
        if edge != Edge::OmitKeeperLoanEnd {
            state.keeper_loan = HeapLoan::Ended;
        }
        state.tickets.push(Ticket {
            owner: id,
            id: TicketId(request),
            transfer: Transfer::Outbound,
        });
        state.check_ticket();
        shared.owner_changed[id.0].notify_one();
        if edge != Edge::OmitUniqueTicketWait {
            while state.tickets[request].transfer != Transfer::Returned && state.failure.is_none() {
                state = shared.keeper_changed.wait(state).unwrap();
            }
        }
        if state.failure.is_some() {
            if edge == Edge::OmitPoisonResumeFence {
                state.resume();
            }
            assert_eq!(state.phase, Phase::Poisoned);
            assert_eq!(state.keeper_loan, HeapLoan::Ended);
            assert_eq!(state.outstanding(), 1);
            assert!(!state.modes.contains(&OwnerMode::Resumed));
            return;
        }
    }
    // Keeper stays reference-free across the queue batch. The sole mutation
    // for DOUBLE_TICKET is skipping the return wait above; every other action
    // is identical, including this final reconstruction after all returns.
    state.keeper_loan = HeapLoan::Local;
    state.keeper_heap_access();
    state.resume();
    shared.wake_owners();
}

fn propagate(result: std::thread::Result<()>) {
    if let Err(failure) = result {
        std::panic::resume_unwind(failure);
    }
}

type RunCount = std::sync::Arc<AtomicUsize>;

fn explore(case: Case, edge: Edge, count: RunCount) -> usize {
    let bound = std::env::var("PENTRY_PREEMPTIONS")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("PENTRY_PREEMPTIONS must be an integer")
        })
        .unwrap_or(2);
    assert!([2, 3].contains(&bound), "qualify PENTRY_PREEMPTIONS=2 or 3");
    let runs = count.clone();
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(bound);
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.max_threads = 4;
    builder.max_branches = 10_000;
    builder.check(move || {
        runs.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared::new(case));
        let service = shared.clone();
        let keeper_thread = thread::spawn(move || keeper(service, case, edge));
        let peer = if case.owners() == 2 {
            let peer = shared.clone();
            Some(thread::spawn(move || owner(peer, OwnerId(1), case, edge)))
        } else {
            None
        };
        // The model's original/main owner publishes BEFORE waiting for keeper.
        // Keeper never doubles as a Lisp owner or waits for its own ack.
        owner(shared.clone(), OwnerId(0), case, edge);
        if let Some(peer) = peer {
            propagate(peer.join());
        }
        propagate(keeper_thread.join());
        let state = shared.state.lock().unwrap();
        match case {
            Case::MidTransferFailure | Case::RestoreFailure => {
                assert!(state.failure.is_some());
                assert_eq!(state.phase, Phase::Poisoned);
                assert_eq!(state.outstanding(), 1);
            }
            Case::StopPublication
            | Case::TicketRoundtrip
            | Case::SerialCollection
            | Case::TwoRequests
            | Case::SelfStopMain
            | Case::NestedSerial => {
                assert_eq!(state.phase, Phase::Draining);
                assert_eq!(state.serviced.len(), case.requests());
                assert_eq!(state.serviced, state.submitted);
                if case == Case::TwoRequests {
                    assert_ne!(state.serviced[0], state.serviced[1]);
                }
            }
        }
    });
    let count = count.load(Ordering::Relaxed);
    assert!(count > 0);
    println!("PENTRY_MODEL case={case:?} bound={bound} executions={count}");
    count
}

macro_rules! positive {
    ($name:ident, $case:ident) => {
        #[test]
        fn $name() {
            explore(
                Case::$case,
                Edge::Required,
                RunCount::new(AtomicUsize::new(0)),
            );
        }
    };
}
positive!(keeper_owner_stop_publication, StopPublication);
positive!(ticket_transfer_return, TicketRoundtrip);
positive!(serial_collection_with_ticket_out, SerialCollection);
positive!(two_concurrent_serial_requests, TwoRequests);
positive!(owner_fails_mid_transfer, MidTransferFailure);
positive!(owner_fails_restore, RestoreFailure);
positive!(self_stop_main_participant, SelfStopMain);
positive!(nested_serial_reborrow, NestedSerial);

fn control(case: Case, edge: Edge, witness: &str) -> usize {
    let runs = RunCount::new(AtomicUsize::new(0));
    let result = std::panic::catch_unwind(|| explore(case, edge, runs.clone()));
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
    println!(
        "PENTRY_NEGATIVE witness={witness} outcome=detected edge={edge:?} executions_to_witness={count}"
    );
    count
}
#[test]
fn negative_ticket_while_running() {
    control(
        Case::TicketRoundtrip,
        Edge::OmitStoppedWait,
        "TICKET_WHILE_RUNNING",
    );
}
#[test]
fn negative_keeper_touches_heap_with_ticket_out() {
    control(
        Case::TicketRoundtrip,
        Edge::OmitKeeperLoanEnd,
        "KEEPER_TOUCHES_HEAP_WITH_TICKET_OUT",
    );
}
#[test]
fn negative_stale_publication_used_for_serial_gc() {
    control(
        Case::SerialCollection,
        Edge::OmitCurrentRoots,
        "STALE_PUBLICATION_USED_FOR_SERIAL_GC",
    );
}
#[test]
fn negative_double_ticket() {
    control(
        Case::TwoRequests,
        Edge::OmitUniqueTicketWait,
        "DOUBLE_TICKET",
    );
}
#[test]
fn negative_resume_after_failed_restore() {
    control(
        Case::RestoreFailure,
        Edge::OmitPoisonResumeFence,
        "RESUME_AFTER_FAILED_RESTORE",
    );
}
#[test]
fn negative_view_outlives_guard() {
    control(
        Case::TicketRoundtrip,
        Edge::OmitViewEnd,
        "VIEW_OUTLIVES_GUARD",
    );
}

#[test]
fn negative_ack_before_publication_requires_exploration() {
    let count = control(
        Case::TicketRoundtrip,
        Edge::AckBeforePublication,
        "ACK_BEFORE_PUBLICATION",
    );
    assert!(
        count > 1,
        "A5 must pass execution 1 and fail only during exploration"
    );
}
