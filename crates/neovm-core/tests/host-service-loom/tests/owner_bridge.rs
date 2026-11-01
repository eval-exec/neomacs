//! Bounded intermediate-owner protocol model. The audit reconstructs authority
//! from independently recorded effects, never from the machine's final slots.
use loom::sync::{Arc, Mutex, mpsc};
use loom::thread;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    None,
    IntermediateReturnClearsReservation,
    IntermediateHopDeliversSignal,
    IntermediateHopStealsFifo,
    IntermediateHopLosesReleased,
    DuplicateStepCompletion,
    CancelClaimRevokesLive,
    CloseRejectsAdmittedCleanup,
    DrainDropsContinuing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parent {
    Request,
    Unrelated,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nonce {
    First,
    Second,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Step {
    parent: Parent,
    nonce: Nonce,
}
const FIRST: Step = Step {
    parent: Parent::Request,
    nonce: Nonce::First,
};
const SECOND: Step = Step {
    parent: Parent::Request,
    nonce: Nonce::Second,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Eligibility {
    Reserved,
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Serving,
    AwaitingOwner(Step),
    Claimed(Step),
    Completed(Step),
    AwaitingNative,
    Ended(Step),
    Cleanup(Step),
    Final,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reservation {
    Retained,
    Absent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootState {
    Published,
    Terminal,
    Acknowledged,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntentCause {
    TargetSignal,
    HostClosed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intent {
    Uninterrupted,
    Requested(IntentCause),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admission {
    Open,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Main,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainWait {
    Input,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Completion {
    Accepted,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupAdmission {
    Admitted,
    Refused,
}

#[derive(Clone, Debug)]
enum Event {
    Published,
    RetainedUnrelated,
    IntentRequested(IntentCause),
    IntentObserved(Intent),
    NeedOwner {
        step: Step,
        eligibility: Eligibility,
        main_wait: MainWait,
    },
    ReservationRemoved(Parent),
    ReadyTaken(Owner),
    SignalCheckpoint,
    Restored(Step),
    Claimed(Step),
    GenuineRelease,
    CompletionAccepted(Step),
    CompletionRejected(Step),
    NativeContinues(Step),
    NativePending,
    PhysicalRelease,
    NativeSettled,
    PhysicalAcquire,
    ParentSettled,
    OwnerAcknowledged,
    Closed,
    NewParentRefused,
    NewParentAccepted,
    CleanupAdmitted(Step),
    CleanupRefused,
    CleanupOwnerRestored(Step),
    ReservationsObserved {
        request: Reservation,
        unrelated: Reservation,
    },
    LiveClaimRevoked(Step),
    OwnerExited(Step),
    IndexedOwnerRemoved,
    EndedAuthorityRetained(Step),
    RevokedEnded(Step),
    DrainPending,
    Drained,
    RootRetired,
}

struct Machine {
    phase: Phase,
    reservation: Reservation,
    unrelated: Reservation,
    owner_liveness: OwnerHistory,
    owner_index: IndexedHistory,
    eligibility: Eligibility,
    roots: RootState,
    intent: Intent,
    admission: Admission,
    ready: VecDeque<Owner>,
    main_wait: MainWait,
    events: Vec<Event>,
}
impl Machine {
    fn new() -> Self {
        Self {
            phase: Phase::Serving,
            reservation: Reservation::Retained,
            unrelated: Reservation::Absent,
            owner_liveness: OwnerHistory::Alive,
            owner_index: IndexedHistory::Present,
            eligibility: Eligibility::Reserved,
            roots: RootState::Published,
            intent: Intent::Uninterrupted,
            admission: Admission::Open,
            ready: VecDeque::from([Owner::Main, Owner::Other]),
            main_wait: MainWait::Input,
            events: vec![Event::Published],
        }
    }
    fn retain_reservation(&mut self, parent: Parent) {
        match parent {
            Parent::Request => self.reservation = Reservation::Retained,
            Parent::Unrelated => {
                self.unrelated = Reservation::Retained;
                self.events.push(Event::RetainedUnrelated);
            }
        }
    }
    fn remove_reservation(&mut self, parent: Parent) {
        match parent {
            Parent::Request => self.reservation = Reservation::Absent,
            Parent::Unrelated => self.unrelated = Reservation::Absent,
        }
        self.events.push(Event::ReservationRemoved(parent));
    }
    fn observe_reservations(&mut self) {
        self.events.push(Event::ReservationsObserved {
            request: self.reservation,
            unrelated: self.unrelated,
        });
    }
    fn need_owner(&mut self, step: Step, mutation: Mutation) {
        self.phase = Phase::AwaitingOwner(step);
        if mutation == Mutation::IntermediateHopLosesReleased && step == SECOND {
            self.eligibility = Eligibility::Reserved;
        }
        self.events.push(Event::NeedOwner {
            step,
            eligibility: self.eligibility,
            main_wait: self.main_wait,
        });
        if mutation == Mutation::IntermediateReturnClearsReservation {
            self.remove_reservation(Parent::Request);
        }
        if mutation == Mutation::IntermediateHopDeliversSignal {
            self.events.push(Event::SignalCheckpoint);
        }
        if mutation == Mutation::IntermediateHopStealsFifo {
            if let Some(owner) = self.ready.pop_front() {
                self.events.push(Event::ReadyTaken(owner));
            }
        }
    }
    fn restore_and_claim(&mut self, step: Step) {
        self.events.push(Event::Restored(step));
        self.phase = Phase::Claimed(step);
        self.events.push(Event::Claimed(step));
        self.events.push(Event::IntentObserved(self.intent));
    }
    fn genuine_release(&mut self) {
        self.eligibility = Eligibility::Released;
        self.events.push(Event::GenuineRelease);
    }
    fn complete(&mut self, step: Step, mutation: Mutation) -> Completion {
        let accepts =
            self.phase == Phase::Claimed(step) || mutation == Mutation::DuplicateStepCompletion;
        if accepts {
            // The mutation changes the nonce acceptance rule and consumes the
            // currently selected port, rather than merely emitting a tag.
            self.phase = Phase::Completed(step);
            self.events.push(Event::CompletionAccepted(step));
            Completion::Accepted
        } else {
            self.events.push(Event::CompletionRejected(step));
            Completion::Rejected
        }
    }
    fn continue_native(&mut self, step: Step) {
        self.phase = Phase::Serving;
        self.events.push(Event::NativeContinues(step));
    }
    fn request_intent(&mut self, cause: IntentCause, mutation: Mutation) {
        self.events.push(Event::IntentRequested(cause));
        match self.intent {
            Intent::Uninterrupted => self.intent = Intent::Requested(cause),
            Intent::Requested(_) => {}
        }
        if mutation == Mutation::CancelClaimRevokesLive {
            if let Phase::Claimed(step) = self.phase {
                self.phase = Phase::Ended(step);
                self.events.push(Event::LiveClaimRevoked(step));
            }
        }
        self.events.push(Event::IntentObserved(self.intent));
    }
    fn close(&mut self) {
        self.admission = Admission::Closed;
        self.events.push(Event::Closed);
        self.request_intent(IntentCause::HostClosed, Mutation::None);
        self.attempt_new_parent();
    }
    fn attempt_new_parent(&mut self) {
        match self.admission {
            Admission::Closed => self.events.push(Event::NewParentRefused),
            Admission::Open => self.events.push(Event::NewParentAccepted),
        }
    }
    fn admit_cleanup(&mut self, step: Step, mutation: Mutation) -> CleanupAdmission {
        if self.admission == Admission::Closed && mutation == Mutation::CloseRejectsAdmittedCleanup
        {
            self.events.push(Event::CleanupRefused);
            CleanupAdmission::Refused
        } else {
            self.phase = Phase::Cleanup(step);
            self.events.push(Event::CleanupAdmitted(step));
            CleanupAdmission::Admitted
        }
    }
    fn probe_drain(&mut self, mutation: Mutation) {
        let drained = (self.phase == Phase::Final && self.roots == RootState::Terminal)
            || mutation == Mutation::DrainDropsContinuing;
        if drained {
            self.events.push(Event::Drained);
            self.roots = RootState::Retired;
            self.events.push(Event::RootRetired);
        } else {
            self.events.push(Event::DrainPending);
        }
    }
    fn pending_native(&mut self) {
        self.phase = Phase::AwaitingNative;
        self.events
            .extend([Event::NativePending, Event::PhysicalRelease]);
    }
    fn settle_native(&mut self) {
        self.events.push(Event::NativeSettled);
    }
    fn acquire_native(&mut self) {
        self.events.push(Event::PhysicalAcquire);
    }
    fn final_live(&mut self) {
        self.phase = Phase::Final;
        self.roots = RootState::Terminal;
        self.events.push(Event::ParentSettled);
        self.remove_reservation(Parent::Request);
        self.roots = RootState::Acknowledged;
        self.events.push(Event::OwnerAcknowledged);
        self.roots = RootState::Retired;
        self.events.push(Event::RootRetired);
        self.observe_reservations();
    }
    fn resume_cleanup_owner(&mut self, step: Step) {
        self.phase = Phase::Claimed(step);
        self.events.push(Event::CleanupOwnerRestored(step));
    }
    fn owner_exits_and_is_removed(&mut self, step: Step) {
        self.phase = Phase::Ended(step);
        self.owner_liveness = OwnerHistory::Ended(step);
        self.owner_index = IndexedHistory::Removed;
        self.events.push(Event::OwnerExited(step));
        match self.owner_index {
            IndexedHistory::Removed => self.events.push(Event::IndexedOwnerRemoved),
            IndexedHistory::Present => {}
        }
        match self.owner_liveness {
            OwnerHistory::Ended(retained) => self.events.extend([
                Event::EndedAuthorityRetained(retained),
                Event::RevokedEnded(retained),
            ]),
            OwnerHistory::Alive => self.events.push(Event::LiveClaimRevoked(step)),
        }
    }
    fn final_ended(&mut self) {
        self.phase = Phase::Final;
        self.roots = RootState::Terminal;
        self.events.push(Event::ParentSettled);
        self.remove_reservation(Parent::Request);
        self.probe_drain(Mutation::None);
        self.observe_reservations();
    }
}

// This checker sees only public history effects, not Machine or Mutation.
// It reconstructs root ownership, the port nonce, first intent, and physical
// acquisition independently of the model's private phase fields.
fn audit(events: &[Event]) {
    let mut roots = RootState::Published;
    let mut reservation = Reservation::Retained;
    let mut phase = Phase::Serving;
    let mut restored = None;
    let mut completed = Vec::new();
    let mut eligibility = Eligibility::Reserved;
    let mut intent = Intent::Uninterrupted;
    let mut native = NativeHistory::Absent;
    let mut owner = OwnerHistory::Alive;
    let mut indexed = IndexedHistory::Present;
    let mut unrelated = Reservation::Absent;
    let mut admission = Admission::Open;
    for event in events {
        match *event {
            Event::Published => {
                roots = RootState::Published;
                reservation = Reservation::Retained;
            }
            Event::RetainedUnrelated => unrelated = Reservation::Retained,
            Event::IntentRequested(cause) => match intent {
                Intent::Uninterrupted => intent = Intent::Requested(cause),
                Intent::Requested(_) => {}
            },
            Event::IntentObserved(observed) => assert_eq!(
                observed, intent,
                "first continuation intent was overwritten"
            ),
            Event::NeedOwner {
                step,
                eligibility: observed,
                main_wait,
            } => {
                assert_eq!(
                    step.parent,
                    Parent::Request,
                    "owner step belongs to wrong parent"
                );
                assert_eq!(
                    main_wait,
                    MainWait::Input,
                    "intermediate changed main original wait"
                );
                assert_eq!(
                    roots,
                    RootState::Published,
                    "intermediate parent stopped being Published"
                );
                assert_eq!(
                    reservation,
                    Reservation::Retained,
                    "intermediate cleared parent reservation"
                );
                assert_eq!(
                    observed, eligibility,
                    "intermediate lost genuine Released history"
                );
                assert!(!completed.contains(&step), "owner step reused an old nonce");
                phase = Phase::AwaitingOwner(step);
                restored = None;
            }
            Event::ReservationRemoved(Parent::Request) => {
                assert_eq!(
                    roots,
                    RootState::Terminal,
                    "intermediate cleared parent reservation"
                );
                reservation = Reservation::Absent;
            }
            Event::ReservationRemoved(Parent::Unrelated) => {
                unrelated = Reservation::Absent;
                panic!("ended cleanup removed unrelated parent reservation");
            }
            Event::ReadyTaken(owner) => {
                panic!("intermediate hop stole ordinary FIFO turn: {owner:?}")
            }
            Event::SignalCheckpoint => panic!("intermediate hop delivered target signal"),
            Event::Restored(step) => {
                assert_eq!(
                    phase,
                    Phase::AwaitingOwner(step),
                    "restored wrong continuation nonce"
                );
                restored = Some(step);
            }
            Event::Claimed(step) => {
                assert_eq!(
                    restored,
                    Some(step),
                    "claim occurred before latest owner restore"
                );
                assert_eq!(
                    phase,
                    Phase::AwaitingOwner(step),
                    "claim consumed wrong continuation"
                );
                phase = Phase::Claimed(step);
            }
            Event::GenuineRelease => eligibility = Eligibility::Released,
            Event::CompletionAccepted(step) => {
                assert!(
                    phase == Phase::Claimed(step) && !completed.contains(&step),
                    "duplicate step consumed successor's rightful port"
                );
                completed.push(step);
                phase = Phase::Completed(step);
            }
            Event::CompletionRejected(step) => {
                assert!(
                    phase != Phase::Claimed(step) || completed.contains(&step),
                    "matching live completion was refused"
                );
            }
            Event::NativeContinues(step) => {
                assert_eq!(
                    phase,
                    Phase::Completed(step),
                    "native resumed before exact owner completion"
                );
                assert_eq!(
                    reservation,
                    Reservation::Retained,
                    "native lost parent reservation"
                );
                phase = Phase::Serving;
            }
            Event::NativePending => {
                phase = Phase::AwaitingNative;
                native = NativeHistory::Pending;
            }
            Event::PhysicalRelease => {
                assert_eq!(
                    native,
                    NativeHistory::Pending,
                    "physical release without native obligation"
                );
                eligibility = Eligibility::Released;
            }
            Event::NativeSettled => {
                assert_eq!(
                    native,
                    NativeHistory::Pending,
                    "native settlement without live obligation"
                );
                native = NativeHistory::Settled;
            }
            Event::PhysicalAcquire => {
                assert_eq!(
                    native,
                    NativeHistory::Settled,
                    "driver acquired before native settlement"
                );
                native = NativeHistory::Acquired;
            }
            Event::ParentSettled => {
                assert_eq!(
                    native,
                    NativeHistory::Acquired,
                    "parent settled before native reacquisition"
                );
                assert_eq!(roots, RootState::Published, "parent terminal repeated");
                roots = RootState::Terminal;
                phase = Phase::Final;
            }
            Event::OwnerAcknowledged => {
                assert_eq!(
                    owner,
                    OwnerHistory::Alive,
                    "dead owner acknowledgement was forged"
                );
                assert_eq!(
                    roots,
                    RootState::Terminal,
                    "owner acknowledged intermediate step"
                );
                roots = RootState::Acknowledged;
            }
            Event::Closed => admission = Admission::Closed,
            Event::NewParentRefused => assert_eq!(
                admission,
                Admission::Closed,
                "open host session refused new admission"
            ),
            Event::NewParentAccepted => assert_eq!(
                admission,
                Admission::Open,
                "closed host session admitted a new parent"
            ),
            Event::CleanupAdmitted(step) => {
                assert_eq!(
                    step.parent,
                    Parent::Request,
                    "cleanup switched admitted parent"
                );
                assert_eq!(
                    reservation,
                    Reservation::Retained,
                    "cleanup lost reservation"
                );
                phase = Phase::Cleanup(step);
            }
            Event::CleanupRefused => panic!("close rejected already-admitted cleanup"),
            Event::CleanupOwnerRestored(step) => {
                assert_eq!(
                    native,
                    NativeHistory::Acquired,
                    "cleanup owner resumed before native acquire"
                );
                assert_eq!(
                    owner,
                    OwnerHistory::Alive,
                    "dead owner resumed cleanup Lisp"
                );
                phase = Phase::Claimed(step);
                restored = Some(step);
            }
            Event::ReservationsObserved {
                request,
                unrelated: observed,
            } => {
                assert_eq!(
                    request, reservation,
                    "scheduler reservation effect disagreed with observation"
                );
                assert_eq!(
                    observed, unrelated,
                    "ended cleanup removed unrelated parent reservation"
                );
            }
            Event::LiveClaimRevoked(step) => {
                panic!("cancel revoked live claimed continuation: {step:?}")
            }
            Event::OwnerExited(step) => {
                assert!(
                    matches!(phase, Phase::AwaitingOwner(actual) | Phase::Claimed(actual) if actual == step),
                    "exit fact belongs to another continuation"
                );
                owner = OwnerHistory::Ended(step);
                phase = Phase::Ended(step);
            }
            Event::IndexedOwnerRemoved => {
                assert!(
                    matches!(owner, OwnerHistory::Ended(_)),
                    "indexed absence forged owner death"
                );
                indexed = IndexedHistory::Removed;
            }
            Event::EndedAuthorityRetained(step) => {
                assert_eq!(
                    owner,
                    OwnerHistory::Ended(step),
                    "index removal lost exact exit fact"
                );
                assert_eq!(
                    indexed,
                    IndexedHistory::Removed,
                    "ended authority skipped indexed removal"
                );
                assert_eq!(
                    reservation,
                    Reservation::Retained,
                    "index removal dropped continuation backing"
                );
            }
            Event::RevokedEnded(step) => assert_eq!(
                owner,
                OwnerHistory::Ended(step),
                "revoked a live or foreign owner"
            ),
            Event::DrainPending => assert_eq!(
                roots,
                RootState::Published,
                "pending cleanup lost its registered roots"
            ),
            Event::Drained => {
                assert_eq!(
                    phase,
                    Phase::Final,
                    "drain dropped Continuing owner obligations"
                );
                assert_eq!(
                    native,
                    NativeHistory::Acquired,
                    "drain preceded actual native settlement"
                );
                assert_eq!(
                    reservation,
                    Reservation::Absent,
                    "drain retained ended reservation"
                );
            }
            Event::RootRetired => {
                assert!(
                    roots == RootState::Acknowledged
                        || (matches!(owner, OwnerHistory::Ended(_))
                            && roots == RootState::Terminal),
                    "root backing retired before owner acknowledgement or ended drain"
                );
                roots = RootState::Retired;
            }
        }
    }
    assert_eq!(
        roots,
        RootState::Retired,
        "history never retired its final root backing"
    );
    assert_eq!(
        reservation,
        Reservation::Absent,
        "history retained its completed parent reservation"
    );
    assert_eq!(
        unrelated,
        match events
            .iter()
            .any(|event| matches!(event, Event::RetainedUnrelated))
        {
            true => Reservation::Retained,
            false => Reservation::Absent,
        },
        "unrelated reservation changed"
    );
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeHistory {
    Absent,
    Pending,
    Settled,
    Acquired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerHistory {
    Alive,
    Ended(Step),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexedHistory {
    Present,
    Removed,
}

fn two_steps(mutation: Mutation) {
    let machine = Arc::new(Mutex::new(Machine::new()));
    let (request, work) = mpsc::channel();
    let (answer, returned) = mpsc::channel();
    let owner_machine = Arc::clone(&machine);
    let owner = thread::spawn(move || {
        for step in [FIRST, SECOND] {
            work.recv().unwrap();
            let mut state = owner_machine.lock().unwrap();
            state.restore_and_claim(step);
            if step == FIRST {
                state.genuine_release();
            }
            if step == SECOND {
                let duplicate = state.complete(FIRST, mutation);
                if duplicate == Completion::Accepted {
                    answer.send(Completion::Rejected).unwrap();
                    continue;
                }
            }
            let completion = state.complete(step, Mutation::None);
            drop(state);
            answer.send(completion).unwrap();
        }
    });
    machine
        .lock()
        .unwrap()
        .request_intent(IntentCause::TargetSignal, Mutation::None);
    for step in [FIRST, SECOND] {
        machine.lock().unwrap().need_owner(step, mutation);
        request.send(()).unwrap();
        let completion = returned.recv().unwrap();
        if completion == Completion::Accepted {
            machine.lock().unwrap().continue_native(step);
        }
    }
    owner.join().unwrap();
    let mut state = machine.lock().unwrap();
    state.pending_native();
    state.settle_native();
    state.acquire_native();
    state.final_live();
    audit(&state.events);
}

fn cancel_claim_close(mutation: Mutation) {
    let machine = Arc::new(Mutex::new(Machine::new()));
    machine.lock().unwrap().need_owner(FIRST, Mutation::None);
    let claiming = Arc::clone(&machine);
    let claimant = thread::spawn(move || claiming.lock().unwrap().restore_and_claim(FIRST));
    let cancelling = Arc::clone(&machine);
    let cancellation = thread::spawn(move || {
        cancelling
            .lock()
            .unwrap()
            .request_intent(IntentCause::TargetSignal, mutation);
    });
    claimant.join().unwrap();
    cancellation.join().unwrap();
    let mut state = machine.lock().unwrap();
    state.close();
    state.probe_drain(mutation);
    if state.admit_cleanup(FIRST, mutation) == CleanupAdmission::Admitted {
        state.pending_native();
        state.settle_native();
        state.acquire_native();
        // The claimed callback's completion/Flow remains owned until cleanup
        // settles, even when cancellation arrived first.
        state.resume_cleanup_owner(FIRST);
        state.complete(FIRST, Mutation::None);
        state.final_live();
    }
    audit(&state.events);
}

fn ended_cleanup() {
    let machine = Arc::new(Mutex::new(Machine::new()));
    {
        let mut state = machine.lock().unwrap();
        state.retain_reservation(Parent::Unrelated);
        state.need_owner(FIRST, Mutation::None);
    }
    let ended = Arc::clone(&machine);
    let worker = thread::spawn(move || ended.lock().unwrap().owner_exits_and_is_removed(FIRST));
    worker.join().unwrap();
    let (cancel, cancellation) = mpsc::channel();
    let (settled, settlement) = mpsc::channel();
    let producer_state = Arc::clone(&machine);
    let producer = thread::spawn(move || {
        cancellation.recv().unwrap();
        producer_state.lock().unwrap().settle_native();
        settled.send(()).unwrap();
    });
    {
        let mut state = machine.lock().unwrap();
        state.close();
        state.admit_cleanup(FIRST, Mutation::None);
        state.probe_drain(Mutation::None);
        state.pending_native();
    }
    cancel.send(()).unwrap();
    settlement.recv().unwrap();
    producer.join().unwrap();
    let mut state = machine.lock().unwrap();
    state.acquire_native();
    state.final_ended();
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
    eprintln!("LOOM_OWNER_BRIDGE_WITNESS {mutation:?}: {expected}");
}

#[test]
fn two_owner_steps_preserve_parent_nonce_restore_fifo_and_real_release_history() {
    check(|| two_steps(Mutation::None));
}
#[test]
fn cancellation_racing_claim_and_close_preserves_first_intent_and_admitted_cleanup() {
    check(|| cancel_claim_close(Mutation::None));
}
#[test]
fn ended_owner_keeps_exact_exit_authority_through_index_removal_and_native_drain() {
    check(ended_cleanup);
}
#[test]
fn intermediate_return_clears_reservation_is_rejected() {
    negative(
        Mutation::IntermediateReturnClearsReservation,
        "intermediate cleared parent reservation",
        two_steps,
    );
}
#[test]
fn intermediate_hop_delivers_signal_is_rejected() {
    negative(
        Mutation::IntermediateHopDeliversSignal,
        "intermediate hop delivered target signal",
        two_steps,
    );
}
#[test]
fn intermediate_hop_steals_fifo_is_rejected() {
    negative(
        Mutation::IntermediateHopStealsFifo,
        "intermediate hop stole ordinary FIFO turn",
        two_steps,
    );
}
#[test]
fn intermediate_hop_loses_released_is_rejected() {
    negative(
        Mutation::IntermediateHopLosesReleased,
        "intermediate lost genuine Released history",
        two_steps,
    );
}
#[test]
fn duplicate_step_completion_is_rejected() {
    negative(
        Mutation::DuplicateStepCompletion,
        "duplicate step consumed successor's rightful port",
        two_steps,
    );
}
#[test]
fn cancel_claim_revokes_live_is_rejected() {
    negative(
        Mutation::CancelClaimRevokesLive,
        "cancel revoked live claimed continuation",
        cancel_claim_close,
    );
}
#[test]
fn close_rejects_admitted_cleanup_is_rejected() {
    negative(
        Mutation::CloseRejectsAdmittedCleanup,
        "close rejected already-admitted cleanup",
        cancel_claim_close,
    );
}
#[test]
fn drain_drops_continuing_is_rejected() {
    negative(
        Mutation::DrainDropsContinuing,
        "drain dropped Continuing owner obligations",
        cancel_claim_close,
    );
}
