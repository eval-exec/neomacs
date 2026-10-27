//! The request word and phase are one gated snapshot. A gate refusal is
//! legal; an old word combined with a newly gated phase is not publication
//! corruption and must never be manufactured by the observer.
use super::*;
use loom::sync::Mutex;
use std::sync::TryLockError;

#[derive(Clone, Copy, Debug)]
enum Phase {
    Idle,
    Stopping(StopSequence),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observation {
    Idle,
    Active(StopSequence),
    Busy,
    Refused,
}
struct Registry {
    phase: Mutex<Phase>,
    request: StopWord,
}
impl Registry {
    fn new(previous: StopSequence) -> Self {
        Self {
            phase: Mutex::new(Phase::Idle),
            request: StopWord(AtomicUsize::new(previous.0)),
        }
    }
    fn publish(&self, epoch: StopSequence) {
        let mut phase = self.phase.lock().unwrap();
        *phase = Phase::Stopping(epoch);
        self.request.store(epoch, Ordering::Release);
    }
    fn observe(phase: &Phase, sampled: StopSequence) -> Observation {
        match phase {
            Phase::Idle => Observation::Idle,
            Phase::Stopping(epoch) if *epoch == sampled => Observation::Active(*epoch),
            Phase::Stopping(_) => Observation::Refused,
        }
    }
    fn poll(&self) -> Observation {
        match self.phase.try_lock() {
            Ok(phase) => Self::observe(&phase, self.request.load(Ordering::Acquire)),
            Err(TryLockError::WouldBlock) => Observation::Busy,
            Err(TryLockError::Poisoned(_)) => panic!("unexpected poisoned model gate"),
        }
    }
    /// The broken observation order; the caller sampled before this lock.
    fn poll_pre_sampled(&self, sampled: StopSequence) -> Observation {
        match self.phase.try_lock() {
            Ok(phase) => Self::observe(&phase, sampled),
            Err(TryLockError::WouldBlock) => Observation::Busy,
            Err(TryLockError::Poisoned(_)) => panic!("unexpected poisoned model gate"),
        }
    }
}
fn concurrent(previous: StopSequence, next: StopSequence) {
    let registry = Arc::new(Registry::new(previous));
    let r = Arc::clone(&registry);
    let keeper = thread::spawn(move || r.publish(next));
    let r = Arc::clone(&registry);
    let owner = thread::spawn(move || r.poll());
    keeper.join().unwrap();
    let observed = owner.join().unwrap();
    match observed {
        Observation::Idle | Observation::Busy => {}
        Observation::Active(epoch) => assert_eq!(epoch, next),
        Observation::Refused => panic!("coherent request poll refused valid publication"),
    }
    assert_eq!(registry.poll(), Observation::Active(next));
}
#[test]
fn first_request_poll_reads_word_under_registry_gate() {
    support::check(3, || {
        concurrent(StopSequence::UNREQUESTED, StopSequence::FIRST)
    });
}
#[test]
fn next_request_poll_reads_word_under_registry_gate() {
    support::check(3, || concurrent(StopSequence::FIRST, StopSequence::SECOND));
}
#[test]
fn pre_gate_request_sample_has_a_false_publication_witness() {
    support::negative(
        3,
        "C6 request poll mixed epochs across the registry gate",
        || {
            let registry = Arc::new(Registry::new(StopSequence::FIRST));
            let sampled = Arc::new(Latch::default());
            let committed = Arc::new(Latch::default());
            let r = Arc::clone(&registry);
            let first = Arc::clone(&sampled);
            let second = Arc::clone(&committed);
            let owner = thread::spawn(move || {
                let stale = r.request.load(Ordering::Acquire);
                first.set();
                second.wait();
                r.poll_pre_sampled(stale)
            });
            let r = Arc::clone(&registry);
            let keeper = thread::spawn(move || {
                sampled.wait();
                r.publish(StopSequence::SECOND);
                // publish returned and released the phase gate before this edge.
                committed.set();
            });
            keeper.join().unwrap();
            let observed = owner.join().unwrap();
            assert!(
                observed != Observation::Refused,
                "C6 request poll mixed epochs across the registry gate"
            );
        },
    );
}
