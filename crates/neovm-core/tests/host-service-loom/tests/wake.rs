//! Abstract wake-contract model. The mutex is the metadata linearization
//! boundary; this does not import production std atomics or model kernel I/O.
#![forbid(unsafe_code)]

use loom::sync::{Arc, Mutex, mpsc};
use loom::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    None,
    NoFinalPredicateRecheck,
    SchedulerConsumesMainNudge,
    ConcurrentPollerWaiters,
    IgnoreClosedSession,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Main,
    Scheduler,
}

impl Destination {
    fn index(self) -> usize {
        match self {
            Self::Main => 0,
            Self::Scheduler => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Ready,
    MaySleep,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observation {
    Ready,
    Quiet,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admission {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Readiness {
    #[default]
    Quiet,
    Pending,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SessionState {
    #[default]
    Live,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LeaseState {
    #[default]
    Free,
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArmState {
    Unarmed,
    Armed,
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Arm(Destination, Admission),
    Publish(Destination, Admission),
    Rearm(Destination),
    Decide(Destination, Decision),
    Observe(Destination, Observation),
    Drain(Destination),
    Acquire(Destination, Admission),
    Release(Destination),
    Close,
}

#[derive(Default)]
struct State {
    epochs: [u64; 2],
    armed_epochs: [u64; 2],
    durable: [Readiness; 2],
    hint: Readiness,
    session: SessionState,
    leases: [LeaseState; 2],
    events: Vec<Event>,
}

struct Registration {
    destination: Destination,
    epoch: u64,
}

struct Lease(Destination);

struct Session {
    state: Mutex<State>,
    mutation: Mutation,
}

impl Session {
    fn new(mutation: Mutation) -> Self {
        Self {
            state: Mutex::new(State::default()),
            mutation,
        }
    }

    fn arm(&self, destination: Destination) -> Option<Registration> {
        let mut state = self.state.lock().unwrap();
        let admission = if state.session == SessionState::Live
            || self.mutation == Mutation::IgnoreClosedSession
        {
            Admission::Accepted
        } else {
            Admission::Rejected
        };
        state.events.push(Event::Arm(destination, admission));
        if admission == Admission::Rejected {
            return None;
        }
        let epoch = state.epochs[destination.index()];
        state.armed_epochs[destination.index()] = epoch;
        Some(Registration { destination, epoch })
    }

    fn publish(&self, destination: Destination) {
        let mut state = self.state.lock().unwrap();
        let admission = if state.session == SessionState::Live
            || self.mutation == Mutation::IgnoreClosedSession
        {
            Admission::Accepted
        } else {
            Admission::Rejected
        };
        if admission == Admission::Accepted {
            // The obligation precedes its epoch and hint at this abstract
            // metadata linearization point. A hint carries no completion.
            state.durable[destination.index()] = Readiness::Pending;
            state.epochs[destination.index()] += 1;
            state.hint = Readiness::Pending;
        }
        state.events.push(Event::Publish(destination, admission));
    }

    fn rearm(&self, registration: &Registration) {
        self.state
            .lock()
            .unwrap()
            .events
            .push(Event::Rearm(registration.destination));
    }

    fn decide(&self, registration: &Registration) {
        let mut state = self.state.lock().unwrap();
        let index = registration.destination.index();
        let decision = if state.session == SessionState::Closed {
            Decision::Closed
        } else if self.mutation != Mutation::NoFinalPredicateRecheck
            && (state.durable[index] == Readiness::Pending
                || state.epochs[index] != registration.epoch)
        {
            Decision::Ready
        } else {
            Decision::MaySleep
        };
        state
            .events
            .push(Event::Decide(registration.destination, decision));
    }

    fn observe(&self, registration: &Registration) {
        let mut state = self.state.lock().unwrap();
        let observation = if state.session == SessionState::Closed {
            Observation::Closed
        } else if state.epochs[registration.destination.index()] != registration.epoch {
            Observation::Ready
        } else {
            Observation::Quiet
        };
        state
            .events
            .push(Event::Observe(registration.destination, observation));
    }

    fn drain_hint(&self, destination: Destination) {
        let mut state = self.state.lock().unwrap();
        state.hint = Readiness::Quiet;
        if self.mutation == Mutation::SchedulerConsumesMainNudge
            && destination == Destination::Scheduler
        {
            // Model a destructive shared read cursor: scheduler consumption
            // wrongly erases the main registration's independent progress.
            state.epochs[Destination::Main.index()] = state.armed_epochs[Destination::Main.index()];
        }
        state.events.push(Event::Drain(destination));
    }

    fn acquire(&self, destination: Destination) -> Option<Lease> {
        let mut state = self.state.lock().unwrap();
        let admission = if !state.leases.contains(&LeaseState::Held)
            || self.mutation == Mutation::ConcurrentPollerWaiters
        {
            Admission::Accepted
        } else {
            Admission::Rejected
        };
        if admission == Admission::Accepted {
            state.leases[destination.index()] = LeaseState::Held;
        }
        state.events.push(Event::Acquire(destination, admission));
        match admission {
            Admission::Accepted => Some(Lease(destination)),
            Admission::Rejected => None,
        }
    }

    fn release(&self, lease: Lease) {
        let mut state = self.state.lock().unwrap();
        state.leases[lease.0.index()] = LeaseState::Free;
        state.events.push(Event::Release(lease.0));
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.session = SessionState::Closed;
        state.hint = Readiness::Pending;
        state.events.push(Event::Close);
    }

    fn audit(&self) {
        audit(&self.state.lock().unwrap().events);
    }
}

/// Replay only public transitions. Expected readiness, epochs and lease
/// ownership are reconstructed here, independently of Session's state.
fn audit(events: &[Event]) {
    let mut session = SessionState::Live;
    let mut publications = [0_u64; 2];
    let mut armed = [None; 2];
    let mut rearmed = [ArmState::Unarmed; 2];
    let mut leases = [LeaseState::Free; 2];
    for event in events {
        match *event {
            Event::Arm(destination, admission) => {
                assert!(
                    admission == Admission::Rejected || session == SessionState::Live,
                    "closed session accepted work"
                );
                assert!(
                    admission == Admission::Accepted || session == SessionState::Closed,
                    "open session refused registration"
                );
                if admission == Admission::Accepted {
                    armed[destination.index()] = Some(publications[destination.index()]);
                    rearmed[destination.index()] = ArmState::Unarmed;
                }
            }
            Event::Publish(destination, admission) => {
                assert!(
                    admission == Admission::Rejected || session == SessionState::Live,
                    "closed session accepted work"
                );
                assert!(
                    admission == Admission::Accepted || session == SessionState::Closed,
                    "open session refused publication"
                );
                if admission == Admission::Accepted {
                    publications[destination.index()] += 1;
                }
            }
            Event::Rearm(destination) => {
                assert!(
                    armed[destination.index()].is_some(),
                    "rearm without registration"
                );
                rearmed[destination.index()] = ArmState::Armed;
            }
            Event::Decide(destination, decision) => {
                let index = destination.index();
                assert!(
                    rearmed[index] == ArmState::Armed,
                    "sleep decision preceded physical arm"
                );
                let expected = if session == SessionState::Closed {
                    Decision::Closed
                } else if publications[index] != 0 {
                    Decision::Ready
                } else {
                    Decision::MaySleep
                };
                assert_eq!(
                    decision, expected,
                    "sleep committed without final predicate recheck"
                );
            }
            Event::Observe(destination, observation) => {
                let index = destination.index();
                let snapshot = armed[index].expect("observation without registration");
                let expected = if session == SessionState::Closed {
                    Observation::Closed
                } else if publications[index] != snapshot {
                    Observation::Ready
                } else {
                    Observation::Quiet
                };
                assert_eq!(
                    observation, expected,
                    "scheduler consumed native main nudge"
                );
            }
            Event::Drain(_destination) => {
                // A physical hint drain cannot erase either durable epoch.
            }
            Event::Acquire(destination, admission) => {
                if admission == Admission::Accepted {
                    assert!(
                        !leases.contains(&LeaseState::Held),
                        "concurrent physical poller waiters"
                    );
                    leases[destination.index()] = LeaseState::Held;
                } else {
                    assert!(
                        leases.contains(&LeaseState::Held),
                        "unowned poll lease refused"
                    );
                }
            }
            Event::Release(destination) => {
                assert!(
                    leases[destination.index()] == LeaseState::Held,
                    "release without physical poll lease"
                );
                leases[destination.index()] = LeaseState::Free;
            }
            Event::Close => session = SessionState::Closed,
        }
    }
    assert!(
        !leases.contains(&LeaseState::Held),
        "physical poll lease leaked"
    );
}

fn check(body: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.max_threads = 4;
    builder.max_branches = 256;
    builder.preemption_bound = Some(2);
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.check(body);
}

fn negative(mutation: Mutation, expected: &'static str, scenario: fn(Mutation)) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check(move || scenario(mutation));
    }));
    let panic = result.expect_err("mutation did not produce its required counterexample");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("non-string model failure");
    assert!(
        message
            .lines()
            .next()
            .is_some_and(|line| line.contains(expected)),
        "wrong mutation witness: expected {expected:?}, observed {message:?}"
    );
    eprintln!("LOOM_WAKE_WITNESS {mutation:?}: {expected}");
}

fn arm_scenario(mutation: Mutation) {
    let session = Arc::new(Session::new(mutation));
    let producer_session = Arc::clone(&session);
    let producer = thread::spawn(move || producer_session.publish(Destination::Main));
    let registration = session.arm(Destination::Main).expect("open registration");
    session.rearm(&registration);
    session.decide(&registration);
    producer.join().unwrap();
    // A publication after the final check remains visible after that arm.
    session.observe(&registration);
    session.audit();
}

fn destinations_scenario(mutation: Mutation) {
    let session = Arc::new(Session::new(mutation));
    let main = session.arm(Destination::Main).unwrap();
    let scheduler = session.arm(Destination::Scheduler).unwrap();
    let (published, publication) = mpsc::channel();
    let (drained, drain) = mpsc::channel();
    let producer_session = Arc::clone(&session);
    let producer = thread::spawn(move || {
        producer_session.publish(Destination::Main);
        producer_session.publish(Destination::Scheduler);
        published.send(()).unwrap();
    });
    let scheduler_session = Arc::clone(&session);
    let observer = thread::spawn(move || {
        publication.recv().unwrap();
        scheduler_session.observe(&scheduler);
        scheduler_session.drain_hint(Destination::Scheduler);
        drained.send(()).unwrap();
    });
    drain.recv().unwrap();
    session.observe(&main);
    producer.join().unwrap();
    observer.join().unwrap();
    session.audit();
}

fn lease_scenario(mutation: Mutation) {
    let session = Arc::new(Session::new(mutation));
    let (attempted, attempts) = mpsc::channel();
    let mut holders = Vec::new();
    let mut releases = Vec::new();
    for destination in [Destination::Main, Destination::Scheduler] {
        let session = Arc::clone(&session);
        let attempted = attempted.clone();
        let (release, released) = mpsc::channel();
        releases.push(release);
        holders.push(thread::spawn(move || {
            let lease = session.acquire(destination);
            attempted.send(()).unwrap();
            // Keep a successful lease live until both actors have attempted.
            released.recv().unwrap();
            if let Some(lease) = lease {
                session.release(lease);
            }
        }));
    }
    attempts.recv().unwrap();
    attempts.recv().unwrap();
    for release in releases {
        release.send(()).unwrap();
    }
    for holder in holders {
        holder.join().unwrap();
    }
    session.audit();
}

fn close_scenario(mutation: Mutation) {
    let old = Arc::new(Session::new(mutation));
    let registration = old.arm(Destination::Main).unwrap();
    let producer_session = Arc::clone(&old);
    let producer = thread::spawn(move || producer_session.publish(Destination::Main));
    // Accept-before-close is legal; late publication must be refused.
    old.close();
    producer.join().unwrap();
    old.observe(&registration);
    let _late_registration = old.arm(Destination::Main);
    old.publish(Destination::Main);
    old.audit();

    // A replacement is a separate owner allocation, unaffected by the old
    // publisher or registration. This is identity separation, not an ABA model.
    let replacement = Session::new(mutation);
    let replacement_registration = replacement.arm(Destination::Main).unwrap();
    replacement.publish(Destination::Main);
    replacement.rearm(&replacement_registration);
    replacement.decide(&replacement_registration);
    replacement.audit();
}

#[test]
fn arm_then_recheck_covers_publication_before_during_and_after_arm() {
    check(|| arm_scenario(Mutation::None));
}

#[test]
fn scheduler_hint_drain_preserves_both_destination_epochs() {
    check(|| destinations_scenario(Mutation::None));
}

#[test]
fn native_main_and_scheduler_cannot_hold_the_poller_together() {
    check(|| lease_scenario(Mutation::None));
}

#[test]
fn retired_session_rejects_late_work_without_closing_replacement() {
    check(|| close_scenario(Mutation::None));
}

#[test]
fn no_final_predicate_recheck_is_rejected() {
    negative(
        Mutation::NoFinalPredicateRecheck,
        "sleep committed without final predicate recheck",
        arm_scenario,
    );
}

#[test]
fn scheduler_consumes_main_nudge_is_rejected() {
    negative(
        Mutation::SchedulerConsumesMainNudge,
        "scheduler consumed native main nudge",
        destinations_scenario,
    );
}

#[test]
fn concurrent_poller_waiters_is_rejected() {
    negative(
        Mutation::ConcurrentPollerWaiters,
        "concurrent physical poller waiters",
        lease_scenario,
    );
}

#[test]
fn ignore_closed_session_is_rejected() {
    negative(
        Mutation::IgnoreClosedSession,
        "closed session accepted work",
        close_scenario,
    );
}
