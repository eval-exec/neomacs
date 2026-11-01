//! Abstract owned-pump histories. Channels order causal publications; they do
//! not model kernel polling, OS affinity, or the production channel layout.
#![forbid(unsafe_code)]

use loom::sync::{Arc, Mutex, mpsc};
use loom::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    A,
    B,
}
impl Key {
    fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    None,
    ResumeWrongPendingKey,
    NativeWaitStarvesSecondRequest,
    CompleteBeforeReacquire,
    FreeBackingBeforeDrain,
    CloseDropsReleasedBaton,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BatonOwner {
    Main,
    Requester(Key),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Queue {
    Empty,
    BQueued,
    BServing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Native {
    Unstarted,
    Pending,
    Settled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cancellation {
    None,
    Requested,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backing {
    Live,
    Freed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admission {
    Open,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainWait {
    Unregistered,
    Join,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeCause {
    Returned,
    CancelAcknowledged,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    ServeB,
    RearmA,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trigger {
    NativeEffect,
    Abandon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReleasedBaton {
    Retained,
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    MainJoinRegistered,
    Publish(Key),
    Reserve(Key),
    Serve(Key),
    Defer(Key),
    PhysicalRelease,
    PhysicalReturn,
    AcquireDriver,
    AcquireRequester(Key),
    ParkRequester(Key),
    CollectorVisits,
    QueueB,
    Choose(Choice),
    NativeCommitted(Key),
    NativeSettled(Key, NativeCause),
    KeyedComplete(Key),
    ReturnTo { completed: Key, target: Key },
    Restore(Key),
    Acknowledge(Key),
    Retire(Key),
    ReleaseRequester(Key),
    Cancel(Key),
    Close,
    ObserveMainJoin,
    FreeBacking,
    LastOrdinaryWorkerExit(ReleasedBaton),
}

struct Machine {
    queue: Queue,
    native: [Native; 2],
    cancellation: [Cancellation; 2],
    backing: Backing,
    admission: Admission,
    events: Vec<Event>,
}

impl Machine {
    fn new() -> Self {
        Self {
            queue: Queue::Empty,
            native: [Native::Unstarted; 2],
            cancellation: [Cancellation::None; 2],
            backing: Backing::Live,
            admission: Admission::Open,
            events: vec![Event::MainJoinRegistered],
        }
    }

    fn publish(&mut self, key: Key) {
        assert_eq!(self.admission, Admission::Open);
        self.events
            .extend([Event::Publish(key), Event::Reserve(key)]);
    }

    fn start_pending(&mut self, key: Key) {
        self.native[key.index()] = Native::Pending;
        self.events.extend([Event::Serve(key), Event::Defer(key)]);
    }

    fn choose(&mut self, mutation: Mutation) -> Choice {
        let choice = match self.queue {
            Queue::BQueued => match mutation {
                Mutation::NativeWaitStarvesSecondRequest => Choice::RearmA,
                Mutation::None
                | Mutation::ResumeWrongPendingKey
                | Mutation::CompleteBeforeReacquire
                | Mutation::FreeBackingBeforeDrain
                | Mutation::CloseDropsReleasedBaton => Choice::ServeB,
            },
            Queue::Empty | Queue::BServing => Choice::RearmA,
        };
        if choice == Choice::ServeB {
            self.queue = Queue::BServing;
            self.native[Key::B.index()] = Native::Pending;
            self.events.push(Event::Serve(Key::B));
        }
        self.events.push(Event::Choose(choice));
        choice
    }

    fn settle_native(&mut self, key: Key) {
        let cause = match self.cancellation[key.index()] {
            Cancellation::None => {
                self.events.push(Event::NativeCommitted(key));
                NativeCause::Returned
            }
            Cancellation::Requested => NativeCause::CancelAcknowledged,
        };
        self.native[key.index()] = Native::Settled;
        self.events.push(Event::NativeSettled(key, cause));
    }

    fn finish(&mut self, key: Key, mutation: Mutation) {
        assert_eq!(self.native[key.index()], Native::Settled);
        let target = match key {
            Key::A => match mutation {
                Mutation::ResumeWrongPendingKey => Key::B,
                Mutation::None
                | Mutation::NativeWaitStarvesSecondRequest
                | Mutation::CompleteBeforeReacquire
                | Mutation::FreeBackingBeforeDrain
                | Mutation::CloseDropsReleasedBaton => Key::A,
            },
            Key::B => Key::B,
        };
        self.events.extend([
            Event::KeyedComplete(key),
            Event::ReturnTo {
                completed: key,
                target,
            },
            Event::Restore(target),
            Event::Acknowledge(target),
            Event::Retire(target),
            Event::ReleaseRequester(target),
        ]);
    }

    fn cancel(&mut self, key: Key) {
        self.cancellation[key.index()] = Cancellation::Requested;
        self.events.push(Event::Cancel(key));
    }

    fn close(&mut self, mutation: Mutation) {
        self.admission = Admission::Closed;
        self.events.push(Event::Close);
        self.cancel(Key::A);
        self.cancel(Key::B);
        if mutation == Mutation::FreeBackingBeforeDrain {
            self.free_backing();
        }
    }

    fn free_backing(&mut self) {
        self.backing = Backing::Freed;
        self.events.push(Event::FreeBacking);
    }

    fn last_worker_exit(&mut self, mutation: Mutation) {
        // Main's physical native wait remains a real progress source after
        // closing; a worker's ordinary NoRunnable result cannot drop its Baton.
        let decision = if self.admission == Admission::Closed
            && mutation == Mutation::CloseDropsReleasedBaton
        {
            ReleasedBaton::Dropped
        } else {
            ReleasedBaton::Retained
        };
        self.events.push(Event::LastOrdinaryWorkerExit(decision));
    }

    fn finish_backing(&mut self) {
        match self.backing {
            Backing::Live => self.free_backing(),
            Backing::Freed => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Roots {
    Absent,
    Published,
    Restored,
    Acknowledged,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reservation {
    Absent,
    Reserved,
    Released,
    Returned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Settlement {
    Unstarted,
    Pending,
    NativeReady,
    Terminal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Physical {
    NotReleased,
    Released,
    Returned,
    Acquired,
}

/// Rebuild authority from the observable event stream, not Machine's private
/// queue, native readiness, cancellation, admission or backing fields.
fn audit(events: &[Event]) {
    let mut roots = [Roots::Absent; 2];
    let mut reservations = [Reservation::Absent; 2];
    let mut native = [Settlement::Unstarted; 2];
    let mut commits = [0; 2];
    let mut queue = Queue::Empty;
    let mut baton = Some(BatonOwner::Main);
    let mut physical = Physical::NotReleased;
    let mut main_wait = MainWait::Unregistered;
    let mut backing = Backing::Live;
    let mut admission = Admission::Open;
    for &event in events {
        match event {
            Event::MainJoinRegistered => main_wait = MainWait::Join,
            Event::Publish(key) => {
                assert_eq!(
                    admission,
                    Admission::Open,
                    "published after native admission closed"
                );
                assert_eq!(roots[key.index()], Roots::Absent);
                roots[key.index()] = Roots::Published;
            }
            Event::Reserve(key) => {
                assert_eq!(roots[key.index()], Roots::Published);
                assert_eq!(reservations[key.index()], Reservation::Absent);
                reservations[key.index()] = Reservation::Reserved;
            }
            Event::Serve(key) => {
                assert_eq!(baton, Some(BatonOwner::Main));
                assert_eq!(native[key.index()], Settlement::Unstarted);
                assert_eq!(reservations[key.index()], Reservation::Reserved);
                if key == Key::B && native[Key::A.index()] == Settlement::Pending {
                    assert_eq!(
                        roots[Key::A.index()],
                        Roots::Published,
                        "serving B erased A roots"
                    );
                    assert_eq!(
                        reservations[Key::A.index()],
                        Reservation::Released,
                        "serving B erased A reservation"
                    );
                }
                native[key.index()] = Settlement::Pending;
            }
            Event::Defer(key) => {
                assert_eq!(native[key.index()], Settlement::Pending);
                assert_eq!(reservations[key.index()], Reservation::Reserved);
            }
            Event::PhysicalRelease => {
                assert_eq!(baton, Some(BatonOwner::Main));
                assert!(matches!(
                    physical,
                    Physical::NotReleased | Physical::Acquired
                ));
                for index in 0..2 {
                    if native[index] == Settlement::Pending {
                        assert_eq!(roots[index], Roots::Published);
                        reservations[index] = Reservation::Released;
                    }
                }
                baton = None;
                physical = Physical::Released;
            }
            Event::PhysicalReturn => {
                assert_eq!(physical, Physical::Released);
                physical = Physical::Returned;
            }
            Event::AcquireDriver => {
                assert!(baton.is_none(), "two actors held the Baton");
                assert!(matches!(physical, Physical::Returned | Physical::Acquired));
                baton = Some(BatonOwner::Main);
                physical = Physical::Acquired;
            }
            Event::AcquireRequester(key) => {
                assert!(baton.is_none(), "requester acquired an occupied Baton");
                assert_eq!(
                    physical,
                    Physical::Released,
                    "artificial hop ran unrelated requester"
                );
                baton = Some(BatonOwner::Requester(key));
            }
            Event::ParkRequester(key) => {
                assert_eq!(baton, Some(BatonOwner::Requester(key)));
                assert_eq!(roots[key.index()], Roots::Published);
                baton = None;
            }
            Event::CollectorVisits => {
                assert_eq!(baton, Some(BatonOwner::Requester(Key::B)));
                assert_eq!(
                    roots[Key::A.index()],
                    Roots::Published,
                    "pending A disappeared from collector roots"
                );
            }
            Event::QueueB => queue = Queue::BQueued,
            Event::Choose(choice) => {
                if queue == Queue::BQueued {
                    assert_eq!(choice, Choice::ServeB, "native wait starved queued request");
                    assert_eq!(roots[Key::A.index()], Roots::Published);
                    assert_eq!(reservations[Key::A.index()], Reservation::Released);
                    queue = Queue::BServing;
                }
            }
            Event::NativeCommitted(key) => {
                assert_eq!(backing, Backing::Live, "native effect used freed backing");
                assert_eq!(native[key.index()], Settlement::Pending);
                commits[key.index()] += 1;
                assert_eq!(commits[key.index()], 1, "native effect replayed");
            }
            Event::NativeSettled(key, cause) => {
                assert_eq!(
                    backing,
                    Backing::Live,
                    "native settlement used freed backing"
                );
                assert_eq!(native[key.index()], Settlement::Pending);
                if cause == NativeCause::Returned {
                    assert_eq!(commits[key.index()], 1);
                }
                native[key.index()] = Settlement::NativeReady;
            }
            Event::KeyedComplete(key) => {
                assert_eq!(
                    baton,
                    Some(BatonOwner::Main),
                    "native completion occurred before reacquire"
                );
                assert_eq!(
                    native[key.index()],
                    Settlement::NativeReady,
                    "control wake was treated as keyed native completion"
                );
                assert_eq!(roots[key.index()], Roots::Published);
                native[key.index()] = Settlement::Terminal;
            }
            Event::ReturnTo { completed, target } => {
                assert_eq!(
                    completed, target,
                    "native completion resumed wrong pending key"
                );
                assert_eq!(baton, Some(BatonOwner::Main));
                assert_eq!(native[completed.index()], Settlement::Terminal);
                assert!(matches!(
                    reservations[completed.index()],
                    Reservation::Reserved | Reservation::Released
                ));
                reservations[completed.index()] = Reservation::Returned;
                baton = Some(BatonOwner::Requester(target));
            }
            Event::Restore(key) => {
                assert_eq!(baton, Some(BatonOwner::Requester(key)));
                assert_eq!(native[key.index()], Settlement::Terminal);
                assert_eq!(roots[key.index()], Roots::Published);
                roots[key.index()] = Roots::Restored;
            }
            Event::Acknowledge(key) => {
                assert_eq!(roots[key.index()], Roots::Restored);
                roots[key.index()] = Roots::Acknowledged;
            }
            Event::Retire(key) => {
                assert_eq!(
                    native[key.index()],
                    Settlement::Terminal,
                    "cancellation retired roots before actual settlement"
                );
                assert_eq!(roots[key.index()], Roots::Acknowledged);
                roots[key.index()] = Roots::Retired;
            }
            Event::ReleaseRequester(key) => {
                assert_eq!(baton, Some(BatonOwner::Requester(key)));
                baton = None;
            }
            Event::Cancel(key) => {
                assert!(matches!(
                    roots[key.index()],
                    Roots::Published | Roots::Restored | Roots::Acknowledged | Roots::Retired
                ));
            }
            Event::Close => admission = Admission::Closed,
            Event::LastOrdinaryWorkerExit(decision) => {
                assert_eq!(physical, Physical::Released);
                assert!(baton.is_none());
                assert_eq!(admission, Admission::Closed);
                if roots.iter().any(|root| *root == Roots::Published) {
                    assert_eq!(
                        decision,
                        ReleasedBaton::Retained,
                        "close dropped Baton before native acquire"
                    );
                }
            }
            Event::ObserveMainJoin => assert_eq!(
                main_wait,
                MainWait::Join,
                "native service changed original main wait"
            ),
            Event::FreeBacking => {
                assert!(
                    roots.iter().all(|root| *root == Roots::Retired)
                        && native.iter().all(|state| *state == Settlement::Terminal),
                    "native backing freed before settlement drain"
                );
                assert_eq!(backing, Backing::Live, "native backing freed twice");
                backing = Backing::Freed;
            }
        }
    }
    assert_eq!(roots, [Roots::Retired; 2], "pending native roots leaked");
    assert_eq!(
        reservations,
        [Reservation::Returned; 2],
        "pending native reservation leaked"
    );
    assert_eq!(backing, Backing::Freed);
}

fn b_unblocks_a(mutation: Mutation) {
    let state = Arc::new(Mutex::new(Machine::new()));
    let (release, may_publish) = mpsc::channel();
    let (published, queued) = mpsc::channel();
    let (effect, may_complete) = mpsc::channel();
    let (completed, native_ready) = mpsc::channel();
    let worker_state = Arc::clone(&state);
    let worker = thread::spawn(move || {
        may_publish.recv().unwrap();
        let mut state = worker_state.lock().unwrap();
        state.events.push(Event::AcquireRequester(Key::B));
        state.events.push(Event::CollectorVisits);
        state.publish(Key::B);
        state.queue = Queue::BQueued;
        state
            .events
            .extend([Event::QueueB, Event::ParkRequester(Key::B)]);
        drop(state);
        published.send(()).unwrap();
    });
    let native_state = Arc::clone(&state);
    let producer = thread::spawn(move || {
        match may_complete.recv().unwrap() {
            Trigger::NativeEffect => native_state.lock().unwrap().settle_native(Key::A),
            Trigger::Abandon => {}
        }
        completed.send(()).unwrap();
    });
    {
        let mut state = state.lock().unwrap();
        state.publish(Key::A);
        state.start_pending(Key::A);
        state.events.push(Event::PhysicalRelease);
    }
    release.send(()).unwrap();
    queued.recv().unwrap();
    let choice = {
        let mut state = state.lock().unwrap();
        state
            .events
            .extend([Event::PhysicalReturn, Event::AcquireDriver]);
        state.choose(mutation)
    };
    match choice {
        Choice::ServeB => {
            state.lock().unwrap().settle_native(Key::B);
            effect.send(Trigger::NativeEffect).unwrap();
            state.lock().unwrap().finish(Key::B, mutation);
            native_ready.recv().unwrap();
            let mut state = state.lock().unwrap();
            if mutation != Mutation::CompleteBeforeReacquire {
                state.events.push(Event::AcquireDriver);
            }
            state.finish(Key::A, mutation);
            state.events.push(Event::ObserveMainJoin);
            state.finish_backing();
        }
        Choice::RearmA => {
            // Stop at the incorrect scheduling decision. There is no timeout
            // witness and no worker left blocked on a missing service reply.
            effect.send(Trigger::Abandon).unwrap();
            native_ready.recv().unwrap();
        }
    }
    worker.join().unwrap();
    producer.join().unwrap();
    audit(&state.lock().unwrap().events);
}

fn close_and_cancel(mutation: Mutation) {
    let state = Arc::new(Mutex::new(Machine::new()));
    {
        let mut state = state.lock().unwrap();
        state.publish(Key::A);
        state.start_pending(Key::A);
        state
            .events
            .extend([Event::PhysicalRelease, Event::AcquireRequester(Key::B)]);
        state.publish(Key::B);
        state.queue = Queue::BQueued;
        state.events.extend([
            Event::QueueB,
            Event::ParkRequester(Key::B),
            Event::PhysicalReturn,
            Event::AcquireDriver,
        ]);
        assert_eq!(state.choose(Mutation::None), Choice::ServeB);
        state
            .events
            .extend([Event::Defer(Key::B), Event::PhysicalRelease]);
    }
    let mut producers = Vec::new();
    for key in [Key::A, Key::B] {
        let native_state = Arc::clone(&state);
        producers.push(thread::spawn(move || {
            native_state.lock().unwrap().settle_native(key)
        }));
    }
    let closing_state = Arc::clone(&state);
    let closer = thread::spawn(move || {
        let mut state = closing_state.lock().unwrap();
        state.cancel(Key::A);
        state.close(mutation);
        state.last_worker_exit(mutation);
    });
    for producer in producers {
        producer.join().unwrap();
    }
    closer.join().unwrap();
    let mut state = state.lock().unwrap();
    state
        .events
        .extend([Event::PhysicalReturn, Event::AcquireDriver]);
    state.finish(Key::A, Mutation::None);
    state.events.push(Event::AcquireDriver);
    state.finish(Key::B, Mutation::None);
    state.events.push(Event::ObserveMainJoin);
    state.finish_backing();
    audit(&state.events);
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
    eprintln!("LOOM_PENDING_NATIVE_WITNESS {mutation:?}: {expected}");
}

#[test]
fn b_unblocks_a_with_separate_physical_and_keyed_acquisition() {
    check(|| b_unblocks_a(Mutation::None));
}
#[test]
fn cancellation_and_close_retain_backing_until_native_settlement_and_root_drain() {
    check(|| close_and_cancel(Mutation::None));
}
#[test]
fn resume_wrong_pending_key_is_rejected() {
    negative(
        Mutation::ResumeWrongPendingKey,
        "native completion resumed wrong pending key",
        b_unblocks_a,
    );
}
#[test]
fn native_wait_starves_second_request_is_rejected() {
    negative(
        Mutation::NativeWaitStarvesSecondRequest,
        "native wait starved queued request",
        b_unblocks_a,
    );
}
#[test]
fn complete_before_reacquire_is_rejected() {
    negative(
        Mutation::CompleteBeforeReacquire,
        "native completion occurred before reacquire",
        b_unblocks_a,
    );
}
#[test]
fn free_backing_before_drain_is_rejected() {
    negative(
        Mutation::FreeBackingBeforeDrain,
        "native backing freed before settlement drain",
        close_and_cancel,
    );
}

#[test]
fn close_drops_transferred_baton_is_rejected() {
    negative(
        Mutation::CloseDropsReleasedBaton,
        "close dropped Baton before native acquire",
        close_and_cancel,
    );
}
