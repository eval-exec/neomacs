//! Abstract cut-1b main-native-wait lifetime history. Native readiness and GC
//! are events, not a model of kernel I/O or production RootBatch internals.
#![forbid(unsafe_code)]

use loom::sync::{Arc, Mutex, mpsc};
use loom::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    None,
    DropInstallNudge,
    DecodeBeforeRestore,
    ReleaseWithoutPublishedRoots,
    DoubleNativeLease,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Main,
    Worker,
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
    Signalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootRetention {
    Absent,
    Published,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostAttachment {
    Detached,
    Installed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Running,
    AcquireLease(Owner, Admission),
    ReleaseLease(Owner),
    RootsPublished,
    Released,
    NativeBlocked,
    AcquireWorker,
    WorkerGc,
    ReleaseWorker,
    InstallHost,
    NudgeOldWait,
    ObserveOldWait(Readiness),
    NativeReadiness,
    NativeReturned,
    Reacquiring,
    Reacquired,
    Restored,
    DecodeInput,
    RetireRoots,
}

#[derive(Default)]
struct NativeState {
    lease: Option<Owner>,
    signalled: Readiness,
    events: Vec<Event>,
}

impl NativeState {
    fn acquire_lease(&mut self, owner: Owner, mutation: Mutation) {
        let admission = match (self.lease, mutation) {
            (None, _) | (Some(_), Mutation::DoubleNativeLease) => {
                self.lease = Some(owner);
                Admission::Accepted
            }
            (Some(_), _) => Admission::Rejected,
        };
        self.events.push(Event::AcquireLease(owner, admission));
    }

    fn release_lease(&mut self, owner: Owner) {
        self.events.push(Event::ReleaseLease(owner));
        self.lease = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Running,
    Released,
    Blocked,
    Ready,
    Returned,
    Reacquiring,
    Reacquired,
    Restored,
}

/// Reconstruct authority independently from the public event history. The
/// checker does not consult NativeState's implementation lease/readiness slots.
fn audit(events: &[Event]) {
    let mut phase = Phase::Running;
    let mut baton = Some(Owner::Main);
    let mut lease = None;
    let mut roots = RootRetention::Absent;
    let mut host = HostAttachment::Detached;
    let mut old_wait_nudge = Readiness::Quiet;
    let mut releases = 0;
    let mut blocks = 0;
    let mut returns = 0;
    let mut reacquires = 0;
    for &event in events {
        match event {
            Event::Running => assert_eq!(phase, Phase::Running),
            Event::AcquireLease(owner, Admission::Accepted) => {
                assert!(lease.is_none(), "double native poll lease admitted");
                lease = Some(owner);
            }
            Event::AcquireLease(_, Admission::Rejected) => {
                assert!(lease.is_some(), "free native lease incorrectly refused");
            }
            Event::ReleaseLease(owner) => {
                assert_eq!(lease, Some(owner), "native lease retired by wrong owner");
                lease = None;
            }
            Event::RootsPublished => {
                assert_eq!(baton, Some(Owner::Main));
                roots = RootRetention::Published;
            }
            Event::Released => {
                assert_eq!(
                    roots,
                    RootRetention::Published,
                    "main released without published roots"
                );
                assert_eq!(baton, Some(Owner::Main));
                assert_eq!(phase, Phase::Running);
                baton = None;
                phase = Phase::Released;
                releases += 1;
            }
            Event::NativeBlocked => {
                assert_eq!(phase, Phase::Released);
                assert_eq!(lease, Some(Owner::Main));
                blocks += 1;
                phase = Phase::Blocked;
            }
            Event::AcquireWorker => {
                assert!(baton.is_none(), "worker acquired an occupied Baton");
                baton = Some(Owner::Worker);
            }
            Event::WorkerGc => {
                assert_eq!(baton, Some(Owner::Worker), "worker GC without Baton");
                assert_eq!(
                    roots,
                    RootRetention::Published,
                    "collector cannot see parked main roots"
                );
            }
            Event::ReleaseWorker => {
                assert_eq!(baton, Some(Owner::Worker));
                baton = None;
            }
            Event::InstallHost => host = HostAttachment::Installed,
            Event::NudgeOldWait => {
                assert_eq!(host, HostAttachment::Installed);
                old_wait_nudge = Readiness::Signalled;
            }
            Event::ObserveOldWait(observed) => {
                assert!(
                    host == HostAttachment::Detached || observed == Readiness::Signalled,
                    "installed host left old native wait asleep"
                );
                assert!(
                    observed != Readiness::Signalled || old_wait_nudge == Readiness::Signalled,
                    "old wait became ready without its nudge"
                );
            }
            Event::NativeReadiness => {
                assert_eq!(phase, Phase::Blocked);
                assert_eq!(old_wait_nudge, Readiness::Signalled);
                phase = Phase::Ready;
            }
            Event::NativeReturned => {
                assert_eq!(phase, Phase::Ready);
                returns += 1;
                phase = Phase::Returned;
            }
            Event::Reacquiring => {
                assert_eq!(phase, Phase::Returned);
                phase = Phase::Reacquiring;
            }
            Event::Reacquired => {
                assert_eq!(phase, Phase::Reacquiring);
                assert!(baton.is_none(), "main reacquired an occupied Baton");
                baton = Some(Owner::Main);
                reacquires += 1;
                phase = Phase::Reacquired;
            }
            Event::Restored => {
                assert_eq!(phase, Phase::Reacquired);
                assert_eq!(baton, Some(Owner::Main));
                phase = Phase::Restored;
            }
            Event::DecodeInput => {
                assert!(
                    matches!(phase, Phase::Running | Phase::Restored) && baton == Some(Owner::Main),
                    "Lisp input decoded before main restoration"
                );
            }
            Event::RetireRoots => {
                assert_eq!(phase, Phase::Restored);
                assert_eq!(roots, RootRetention::Published);
                roots = RootRetention::Absent;
            }
        }
    }
    assert_eq!(releases, blocks, "release without physical block");
    assert_eq!(blocks, returns, "physical block without native return");
    assert_eq!(blocks, reacquires, "physical block without one reacquire");
    assert_eq!(roots, RootRetention::Absent, "parked roots leaked");
    assert!(lease.is_none(), "native poll lease leaked");
}

fn physical_wait_scenario(mutation: Mutation) {
    let state = Arc::new(Mutex::new(NativeState::default()));
    let (released, may_collect) = mpsc::channel();
    let (collected, gc_finished) = mpsc::channel();
    let (blocked, may_install) = mpsc::channel();
    let (installed, install_finished) = mpsc::channel();

    let worker_state = Arc::clone(&state);
    let worker = thread::spawn(move || {
        may_collect.recv().unwrap();
        let mut state = worker_state.lock().unwrap();
        state.events.push(Event::AcquireWorker);
        state.acquire_lease(Owner::Worker, mutation);
        // Rejected ordinary lease admission leaves the main lease untouched.
        // The mutant keeps its second accepted lease live in the history.
        state.events.push(Event::WorkerGc);
        state.events.push(Event::ReleaseWorker);
        drop(state);
        collected.send(()).unwrap();
    });
    let install_state = Arc::clone(&state);
    let installer = thread::spawn(move || {
        may_install.recv().unwrap();
        let mut state = install_state.lock().unwrap();
        state.events.push(Event::InstallHost);
        if mutation != Mutation::DropInstallNudge {
            state.events.push(Event::NudgeOldWait);
            state.signalled = Readiness::Signalled;
        }
        drop(state);
        // This is the test's publication-completion witness, not a fabricated
        // native wake. The main actor separately observes actual hint state.
        installed.send(()).unwrap();
    });
    {
        let mut state = state.lock().unwrap();
        state.events.push(Event::Running);
        state.acquire_lease(Owner::Main, Mutation::None);
        if mutation != Mutation::ReleaseWithoutPublishedRoots {
            state.events.push(Event::RootsPublished);
        }
        state.events.push(Event::Released);
        state.events.push(Event::NativeBlocked);
    }
    released.send(()).unwrap();
    blocked.send(()).unwrap();
    install_finished.recv().unwrap();
    gc_finished.recv().unwrap();
    worker.join().unwrap();
    installer.join().unwrap();
    let mut state = state.lock().unwrap();
    let observed = state.signalled;
    state.events.push(Event::ObserveOldWait(observed));
    if observed == Readiness::Signalled {
        state.events.push(Event::NativeReadiness);
        state.events.push(Event::NativeReturned);
        state.events.push(Event::Reacquiring);
        state.events.push(Event::Reacquired);
        if mutation == Mutation::DecodeBeforeRestore {
            state.events.push(Event::DecodeInput);
        }
        state.events.push(Event::Restored);
        state.events.push(Event::DecodeInput);
        state.events.push(Event::RetireRoots);
        state.release_lease(Owner::Main);
    }
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

fn negative(mutation: Mutation, expected: &'static str) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check(move || physical_wait_scenario(mutation));
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
    eprintln!("LOOM_MAIN_NATIVE_WITNESS {mutation:?}: {expected}");
}

#[test]
fn main_physical_wait_roots_survive_worker_gc_and_restore_before_decode() {
    check(|| physical_wait_scenario(Mutation::None));
}

#[test]
fn an_already_ready_main_does_not_release_or_mint_a_physical_wait() {
    check(|| {
        let mut state = NativeState::default();
        state.events.push(Event::Running);
        state.acquire_lease(Owner::Main, Mutation::None);
        state.events.push(Event::InstallHost);
        state.events.push(Event::NudgeOldWait);
        state
            .events
            .push(Event::ObserveOldWait(Readiness::Signalled));
        state.events.push(Event::DecodeInput);
        state.release_lease(Owner::Main);
        audit(&state.events);
    });
}

#[test]
fn drop_install_nudge_is_rejected() {
    negative(
        Mutation::DropInstallNudge,
        "installed host left old native wait asleep",
    );
}

#[test]
fn decode_before_restore_is_rejected() {
    negative(
        Mutation::DecodeBeforeRestore,
        "Lisp input decoded before main restoration",
    );
}

#[test]
fn release_without_published_roots_is_rejected() {
    negative(
        Mutation::ReleaseWithoutPublishedRoots,
        "main released without published roots",
    );
}

#[test]
fn double_native_lease_is_rejected() {
    negative(
        Mutation::DoubleNativeLease,
        "double native poll lease admitted",
    );
}
