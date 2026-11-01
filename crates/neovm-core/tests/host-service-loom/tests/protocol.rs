//! Abstract history model: the mutex represents the registry's linearization
//! boundary, not the production transport's implementation or memory layout.
#![forbid(unsafe_code)]

use loom::sync::{Arc, Mutex, mpsc};
use loom::thread;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mutation {
    None,
    PublishBeforePark,
    WakeBeforeDurableReply,
    ReplayAfterWakeFailure,
    CancelDropsLiveRoots,
    CloseRetiresUnconsumedReplyRoots,
    CompleteAfterCancelTwice,
    RetireBeforeRestore,
    ServiceMakesMainRunnable,
    ArtificialHopYieldsToUnrelatedOwner,
    ArtificialHopDeliversSignal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MainWait {
    Join,
    Mutex,
    Condition,
    Sleep,
    Input,
    Process,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cause {
    Returned,
    Signal,
    Closed,
    OwnerEnded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Entry {
    Prepared,
    Parked,
    Queued,
    Serving(CompletionIntent),
    Terminal(Cause),
    Restored(Cause),
    Acknowledged,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompletionIntent {
    Return,
    Interrupt(Cause),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Admission {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnerLife {
    Alive,
    Ended,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Reservation {
    None,
    ServiceSlice(MainWait),
    ReleasedAtRealWait(MainWait),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetireReason {
    Acknowledged,
    Cancellation,
    Closure,
    BeforeRestore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Park,
    Publish,
    Reserve(MainWait),
    Service(MainWait),
    Dispatch,
    Commit,
    AcquireAfterRealWait,
    Settle(Cause),
    Notify,
    Restore,
    Acknowledge,
    OwnerEnded,
    DeadOwnerAcknowledge,
    Retire(RetireReason),
    MainBecameRunnable,
    UnrelatedOwnerRan,
    SignalDelivered,
    RealWait,
    ReturnReservedSlice(MainWait),
}

struct Machine {
    entry: Entry,
    admission: Admission,
    owner: OwnerLife,
    reservation: Reservation,
    original_main_wait: MainWait,
    mutation: Mutation,
    events: Vec<Event>,
}

impl Machine {
    fn new(wait: MainWait, mutation: Mutation) -> Self {
        Self {
            entry: Entry::Prepared,
            admission: Admission::Open,
            owner: OwnerLife::Alive,
            reservation: Reservation::None,
            original_main_wait: wait,
            mutation,
            events: Vec::new(),
        }
    }

    /// Root preparation precedes publication. A malformed order has a real
    /// witness: a service observer can see the request before its parked roots.
    fn publish(&mut self) {
        assert_eq!(self.entry, Entry::Prepared);
        if self.mutation == Mutation::PublishBeforePark {
            self.events.push(Event::Publish);
            self.events.push(Event::Park);
        } else {
            self.events.push(Event::Park);
            self.entry = Entry::Parked;
            self.events.push(Event::Publish);
        }
        self.entry = Entry::Queued;
    }

    fn settle(&mut self, cause: Cause) {
        self.entry = Entry::Terminal(cause);
        if self.mutation == Mutation::WakeBeforeDurableReply {
            self.events.push(Event::Notify);
            self.events.push(Event::Settle(cause));
        } else {
            self.events.push(Event::Settle(cause));
            self.events.push(Event::Notify);
        }
    }

    /// Service is native-only: main's original Lisp wait is never selected.
    fn dispatch(&mut self) {
        match self.entry {
            Entry::Queued => {
                self.reservation = Reservation::ServiceSlice(self.original_main_wait);
                self.events.push(Event::Reserve(self.original_main_wait));
                if self.mutation == Mutation::ServiceMakesMainRunnable {
                    self.events.push(Event::MainBecameRunnable);
                }
                self.events.push(Event::Service(self.original_main_wait));
                self.entry = Entry::Serving(CompletionIntent::Return);
                self.events.push(Event::Dispatch);
            }
            Entry::Terminal(_) => {}
            Entry::Prepared
            | Entry::Parked
            | Entry::Serving(_)
            | Entry::Restored(_)
            | Entry::Acknowledged
            | Entry::Retired => panic!("invalid service driver stage"),
        }
    }

    fn wake_delivery_failed(&mut self) {
        // A failed wake after accepted publication never replays the native op.
        if self.mutation == Mutation::ReplayAfterWakeFailure {
            self.events.push(Event::Dispatch);
        }
    }

    fn cancel(&mut self, cause: Cause) {
        if self.mutation == Mutation::CancelDropsLiveRoots {
            self.events.push(Event::Retire(RetireReason::Cancellation));
        }
        match self.entry {
            Entry::Queued => self.settle(cause),
            Entry::Serving(CompletionIntent::Return) => {
                self.entry = Entry::Serving(CompletionIntent::Interrupt(cause));
            }
            // First cancellation wins, later close/death still retains backing.
            Entry::Serving(CompletionIntent::Interrupt(_))
            | Entry::Terminal(_)
            | Entry::Restored(_)
            | Entry::Acknowledged
            | Entry::Retired => {}
            Entry::Prepared | Entry::Parked => panic!("cancellation before publication"),
        }
    }

    fn close(&mut self) {
        self.admission = Admission::Closed;
        if self.mutation == Mutation::CloseRetiresUnconsumedReplyRoots {
            self.events.push(Event::Retire(RetireReason::Closure));
        }
        self.cancel(Cause::Closed);
    }

    fn owner_ended(&mut self) {
        self.owner = OwnerLife::Ended;
        self.events.push(Event::OwnerEnded);
        self.cancel(Cause::OwnerEnded);
    }

    fn native_complete(&mut self) {
        match self.entry {
            Entry::Serving(intent) => {
                if let Reservation::ReleasedAtRealWait(wait) = self.reservation {
                    self.events.push(Event::AcquireAfterRealWait);
                    self.reservation = Reservation::ServiceSlice(wait);
                }
                // The effect can commit despite a concurrent cancellation.
                self.events.push(Event::Commit);
                let cause = match intent {
                    CompletionIntent::Return => Cause::Returned,
                    CompletionIntent::Interrupt(cause) => cause,
                };
                self.settle(cause);
            }
            Entry::Terminal(cause) => {
                if self.mutation == Mutation::CompleteAfterCancelTwice && cause != Cause::Returned {
                    self.settle(Cause::Returned);
                }
                // A duplicate/late completion has no second settlement authority.
            }
            Entry::Prepared
            | Entry::Parked
            | Entry::Queued
            | Entry::Restored(_)
            | Entry::Acknowledged
            | Entry::Retired => panic!("completion without service or terminal record"),
        }
    }

    fn try_unrelated_owner(&mut self) {
        match self.reservation {
            Reservation::ServiceSlice(_) => {
                if self.mutation == Mutation::ArtificialHopYieldsToUnrelatedOwner {
                    self.events.push(Event::UnrelatedOwnerRan);
                }
            }
            Reservation::ReleasedAtRealWait(_) | Reservation::None => {
                self.events.push(Event::UnrelatedOwnerRan);
            }
        }
    }

    fn try_signal_checkpoint(&mut self) {
        match self.reservation {
            Reservation::ServiceSlice(_) => {
                if self.mutation == Mutation::ArtificialHopDeliversSignal {
                    self.events.push(Event::SignalDelivered);
                }
            }
            Reservation::ReleasedAtRealWait(_) | Reservation::None => {
                self.events.push(Event::SignalDelivered);
            }
        }
    }

    /// This operation stands for the unforgeable permit produced by a real
    /// blocking wrapper, not an RPC queue handoff or transport notification.
    fn real_wait(&mut self) {
        match self.reservation {
            Reservation::ServiceSlice(wait) => {
                self.reservation = Reservation::ReleasedAtRealWait(wait);
                self.events.push(Event::RealWait);
            }
            Reservation::None | Reservation::ReleasedAtRealWait(_) => {
                panic!("real-wait driver without service reservation")
            }
        }
    }

    fn acknowledge_and_retire(&mut self) {
        let cause = match self.entry {
            Entry::Terminal(cause) => cause,
            Entry::Prepared
            | Entry::Parked
            | Entry::Queued
            | Entry::Serving(_)
            | Entry::Restored(_)
            | Entry::Acknowledged
            | Entry::Retired => panic!("owner completion before terminal settlement"),
        };
        if self.mutation == Mutation::RetireBeforeRestore {
            self.events.push(Event::Retire(RetireReason::BeforeRestore));
        }
        match self.owner {
            OwnerLife::Alive => {
                self.entry = Entry::Restored(cause);
                self.events.push(Event::Restore);
                self.events.push(Event::Acknowledge);
            }
            OwnerLife::Ended => self.events.push(Event::DeadOwnerAcknowledge),
        }
        self.entry = Entry::Acknowledged;
        self.events.push(Event::Retire(RetireReason::Acknowledged));
        self.entry = Entry::Retired;
        match self.reservation {
            Reservation::ServiceSlice(wait) | Reservation::ReleasedAtRealWait(wait) => {
                self.events.push(Event::ReturnReservedSlice(wait));
            }
            Reservation::None => {}
        }
        self.reservation = Reservation::None;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParkProof {
    Absent,
    Published,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettlementProof {
    Pending,
    Terminal,
    Notified,
    Restored,
    Acknowledged,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectProof {
    NotDispatched,
    Dispatched,
    Committed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BatonProof {
    Requester,
    MainNative,
    NativeReleased,
    RequesterRestored,
    SchedulerAfterOwnerDeath,
}

/// Independent trace checker. It reasons about observable obligations, not the
/// Machine's state fields, so a mutant transition cannot bless its own result.
fn audit(events: &[Event], original_wait: MainWait) {
    let mut park = ParkProof::Absent;
    let mut settlement = SettlementProof::Pending;
    let mut effect = EffectProof::NotDispatched;
    let mut reservation = Reservation::None;
    let mut life = OwnerLife::Alive;
    let mut baton = BatonProof::Requester;
    for event in events {
        match *event {
            Event::Park => park = ParkProof::Published,
            Event::Publish => assert_eq!(
                park,
                ParkProof::Published,
                "publication preceded parked roots"
            ),
            Event::Reserve(wait) => {
                assert_eq!(
                    baton,
                    BatonProof::Requester,
                    "Baton transferred without requester authority"
                );
                baton = BatonProof::MainNative;
                assert_eq!(wait, original_wait, "main's original wait changed");
                assert_eq!(
                    reservation,
                    Reservation::None,
                    "two Baton service reservations"
                );
                reservation = Reservation::ServiceSlice(wait);
            }
            Event::Service(wait) => {
                assert_eq!(
                    baton,
                    BatonProof::MainNative,
                    "native service has no Baton authority"
                );
                assert_eq!(wait, original_wait, "main's original wait changed");
                assert_eq!(
                    reservation,
                    Reservation::ServiceSlice(wait),
                    "native service without reserved Baton"
                );
            }
            Event::Dispatch => {
                assert_eq!(
                    effect,
                    EffectProof::NotDispatched,
                    "native dispatch replayed"
                );
                effect = EffectProof::Dispatched;
            }
            Event::Commit => {
                assert_eq!(
                    baton,
                    BatonProof::MainNative,
                    "native settlement has no reacquired Baton"
                );
                assert_eq!(
                    effect,
                    EffectProof::Dispatched,
                    "native effect committed twice or without dispatch"
                );
                effect = EffectProof::Committed;
            }
            Event::AcquireAfterRealWait => {
                assert_eq!(
                    baton,
                    BatonProof::NativeReleased,
                    "native acquire without real release"
                );
                let wait = match reservation {
                    Reservation::ReleasedAtRealWait(wait) => wait,
                    Reservation::None | Reservation::ServiceSlice(_) => {
                        panic!("native acquire lost reserved slice")
                    }
                };
                baton = BatonProof::MainNative;
                reservation = Reservation::ServiceSlice(wait);
            }
            Event::Settle(_) => {
                assert_eq!(
                    settlement,
                    SettlementProof::Pending,
                    "terminal settlement occurred twice"
                );
                settlement = SettlementProof::Terminal;
            }
            Event::Notify => {
                assert_eq!(
                    settlement,
                    SettlementProof::Terminal,
                    "notification preceded durable settlement"
                );
                settlement = SettlementProof::Notified;
            }
            Event::Restore => {
                match baton {
                    BatonProof::Requester | BatonProof::MainNative => {
                        baton = BatonProof::RequesterRestored
                    }
                    BatonProof::NativeReleased
                    | BatonProof::RequesterRestored
                    | BatonProof::SchedulerAfterOwnerDeath => {
                        panic!("restore without sole Baton authority")
                    }
                }
                assert_eq!(
                    settlement,
                    SettlementProof::Notified,
                    "restore preceded durable reply"
                );
                settlement = SettlementProof::Restored;
            }
            Event::Acknowledge => {
                assert_eq!(
                    settlement,
                    SettlementProof::Restored,
                    "acknowledgement preceded owner restoration"
                );
                settlement = SettlementProof::Acknowledged;
            }
            Event::OwnerEnded => life = OwnerLife::Ended,
            Event::DeadOwnerAcknowledge => {
                match baton {
                    BatonProof::Requester | BatonProof::MainNative => {
                        baton = BatonProof::SchedulerAfterOwnerDeath
                    }
                    BatonProof::NativeReleased
                    | BatonProof::RequesterRestored
                    | BatonProof::SchedulerAfterOwnerDeath => {
                        panic!("dead-owner drain without sole Baton authority")
                    }
                }
                assert_eq!(
                    life,
                    OwnerLife::Ended,
                    "live owner claimed dead-owner authority"
                );
                assert_eq!(
                    settlement,
                    SettlementProof::Notified,
                    "dead-owner drain preceded native settlement"
                );
                settlement = SettlementProof::Acknowledged;
            }
            Event::Retire(reason) => {
                let diagnostic = match reason {
                    RetireReason::Cancellation => "cancellation dropped live roots",
                    RetireReason::Closure => "closure retired unconsumed reply roots",
                    RetireReason::BeforeRestore => "roots retired before owner restoration",
                    RetireReason::Acknowledged => "roots retired without acknowledgement",
                };
                assert_eq!(settlement, SettlementProof::Acknowledged, "{diagnostic}");
                settlement = SettlementProof::Retired;
            }
            Event::MainBecameRunnable => panic!("service made main's original wait runnable"),
            Event::UnrelatedOwnerRan => assert!(
                !matches!(reservation, Reservation::ServiceSlice(_)),
                "artificial hop yielded to unrelated owner"
            ),
            Event::SignalDelivered => assert!(
                !matches!(reservation, Reservation::ServiceSlice(_)),
                "artificial hop delivered target signal"
            ),
            Event::RealWait => {
                assert_eq!(
                    baton,
                    BatonProof::MainNative,
                    "real wait without native Baton authority"
                );
                baton = BatonProof::NativeReleased;
                let wait = match reservation {
                    Reservation::ServiceSlice(wait) => wait,
                    Reservation::None | Reservation::ReleasedAtRealWait(_) => {
                        panic!("release without service reservation")
                    }
                };
                reservation = Reservation::ReleasedAtRealWait(wait);
            }
            Event::ReturnReservedSlice(wait) => {
                assert!(
                    matches!(
                        baton,
                        BatonProof::RequesterRestored | BatonProof::SchedulerAfterOwnerDeath
                    ),
                    "reserved slice returned before sole-owner restoration"
                );
                assert_eq!(wait, original_wait, "main's original wait changed");
                assert_eq!(
                    settlement,
                    SettlementProof::Retired,
                    "Baton returned before retained obligations retired"
                );
                reservation = Reservation::None;
            }
        }
    }
    assert_eq!(
        settlement,
        SettlementProof::Retired,
        "unretired root obligations"
    );
    assert_eq!(
        reservation,
        Reservation::None,
        "stranded service reservation"
    );
}

fn check(body: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.max_threads = 4;
    builder.max_branches = 256;
    builder.preemption_bound = Some(2);
    // No duration/permutation truncation; the explicit preemption bound is the
    // model's coverage bound, not a timeout mistaken for exhaustive proof.
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
    // assert_eq! failures include the standard left/right display; identify the
    // precise invariant and reject deadlocks, branch exhaustion and driver bugs.
    assert!(
        message
            .lines()
            .next()
            .is_some_and(|line| line.contains(expected)),
        "wrong mutation witness: expected {expected:?}, observed {message:?}"
    );
    eprintln!("LOOM_WITNESS {mutation:?}: {expected}");
}

fn returned_scenario(mutation: Mutation) {
    let mut machine = Machine::new(MainWait::Join, mutation);
    machine.publish();
    machine.dispatch();
    machine.try_unrelated_owner();
    machine.try_signal_checkpoint();
    machine.native_complete();
    machine.wake_delivery_failed();
    machine.acknowledge_and_retire();
    audit(&machine.events, machine.original_main_wait);
}

fn cancelled_scenario(mutation: Mutation) {
    let mut machine = Machine::new(MainWait::Input, mutation);
    machine.publish();
    machine.cancel(Cause::Signal);
    machine.native_complete();
    machine.acknowledge_and_retire();
    audit(&machine.events, machine.original_main_wait);
}

fn closed_reply_scenario(mutation: Mutation) {
    let mut machine = Machine::new(MainWait::Condition, mutation);
    machine.publish();
    machine.dispatch();
    machine.native_complete();
    machine.close();
    machine.acknowledge_and_retire();
    audit(&machine.events, machine.original_main_wait);
}

#[test]
fn request_service_cancel_close_and_worker_death_race() {
    check(|| {
        let machine = Arc::new(Mutex::new(Machine::new(MainWait::Join, Mutation::None)));
        machine.lock().unwrap().publish();
        let (native_tx, native_rx) = mpsc::channel();
        let host_machine = Arc::clone(&machine);
        let host = thread::spawn(move || {
            host_machine.lock().unwrap().dispatch();
            native_tx.send(()).unwrap();
        });
        let completion_machine = Arc::clone(&machine);
        let completion = thread::spawn(move || {
            native_rx.recv().unwrap();
            let mut state = completion_machine.lock().unwrap();
            state.native_complete();
            state.native_complete(); // late duplicate completion is inert
        });
        let control_machine = Arc::clone(&machine);
        let control = thread::spawn(move || {
            control_machine.lock().unwrap().cancel(Cause::Signal);
            control_machine.lock().unwrap().close();
            control_machine.lock().unwrap().owner_ended();
        });
        host.join().unwrap();
        completion.join().unwrap();
        control.join().unwrap();
        let mut state = machine.lock().unwrap();
        state.acknowledge_and_retire();
        audit(&state.events, state.original_main_wait);
    });
}

#[test]
fn every_original_main_wait_survives_service_without_becoming_runnable() {
    check(|| {
        for wait in [
            MainWait::Join,
            MainWait::Mutex,
            MainWait::Condition,
            MainWait::Sleep,
            MainWait::Input,
            MainWait::Process,
        ] {
            let mut machine = Machine::new(wait, Mutation::None);
            machine.publish();
            machine.dispatch();
            machine.try_unrelated_owner();
            machine.try_signal_checkpoint();
            machine.native_complete();
            machine.acknowledge_and_retire();
            audit(&machine.events, wait);
        }
    });
}

#[test]
fn genuine_blocking_release_allows_other_owner_and_target_signal() {
    check(|| {
        let mut machine = Machine::new(MainWait::Process, Mutation::None);
        machine.publish();
        machine.dispatch();
        machine.real_wait();
        machine.try_unrelated_owner();
        machine.try_signal_checkpoint();
        machine.cancel(Cause::Signal);
        machine.native_complete();
        machine.acknowledge_and_retire();
        assert!(machine.events.contains(&Event::UnrelatedOwnerRan));
        assert!(machine.events.contains(&Event::SignalDelivered));
        audit(&machine.events, machine.original_main_wait);
    });
}

#[test]
fn cancel_before_dispatch_and_close_after_commit_retain_roots_until_ack() {
    check(|| {
        cancelled_scenario(Mutation::None);
        closed_reply_scenario(Mutation::None);
    });
}

#[test]
fn publish_before_park_is_rejected() {
    negative(
        Mutation::PublishBeforePark,
        "publication preceded parked roots",
        returned_scenario,
    );
}
#[test]
fn wake_before_durable_reply_is_rejected() {
    negative(
        Mutation::WakeBeforeDurableReply,
        "notification preceded durable settlement",
        returned_scenario,
    );
}
#[test]
fn replay_after_wake_failure_is_rejected() {
    negative(
        Mutation::ReplayAfterWakeFailure,
        "native dispatch replayed",
        returned_scenario,
    );
}
#[test]
fn cancel_drops_live_roots_is_rejected() {
    negative(
        Mutation::CancelDropsLiveRoots,
        "cancellation dropped live roots",
        cancelled_scenario,
    );
}
#[test]
fn close_retires_unconsumed_reply_roots_is_rejected() {
    negative(
        Mutation::CloseRetiresUnconsumedReplyRoots,
        "closure retired unconsumed reply roots",
        closed_reply_scenario,
    );
}
#[test]
fn complete_after_cancel_twice_is_rejected() {
    negative(
        Mutation::CompleteAfterCancelTwice,
        "terminal settlement occurred twice",
        cancelled_scenario,
    );
}
#[test]
fn retire_before_restore_is_rejected() {
    negative(
        Mutation::RetireBeforeRestore,
        "roots retired before owner restoration",
        returned_scenario,
    );
}
#[test]
fn service_makes_main_runnable_is_rejected() {
    negative(
        Mutation::ServiceMakesMainRunnable,
        "service made main's original wait runnable",
        returned_scenario,
    );
}
#[test]
fn artificial_hop_yields_to_unrelated_owner_is_rejected() {
    negative(
        Mutation::ArtificialHopYieldsToUnrelatedOwner,
        "artificial hop yielded to unrelated owner",
        returned_scenario,
    );
}
#[test]
fn artificial_hop_delivers_signal_is_rejected() {
    negative(
        Mutation::ArtificialHopDeliversSignal,
        "artificial hop delivered target signal",
        returned_scenario,
    );
}

#[path = "pending_native.rs"]
mod pending_native;
