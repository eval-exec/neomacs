use super::super::{ControlDestination, ControlNudge, ControlNudgeOutcome};
use super::*;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
enum HandoffEvent {
    Released,
    Reacquired,
}

struct RecordedHandoff {
    wake: Arc<ControlWake>,
    events: Vec<HandoffEvent>,
}

impl NativeWaitHandoff for RecordedHandoff {
    fn release(&mut self, _permit: RealWaitPermit) {
        self.events.push(HandoffEvent::Released);
    }

    fn reacquire(&mut self) {
        // A real acquire may need this metadata: Condvar's returned predicate
        // guard must have been dropped before handing control back to us.
        assert!(self.wake.metadata.try_lock().is_ok());
        self.events.push(HandoffEvent::Reacquired);
    }
}

fn recorder(wake: &Arc<ControlWake>) -> RecordedHandoff {
    RecordedHandoff {
        wake: Arc::clone(wake),
        events: Vec::new(),
    }
}

#[test]
fn immediate_expired_and_already_ready_plans_mint_no_wait_authority() {
    let (wake, publisher) =
        ControlWake::paired_native(Arc::new(polling::Poller::new().unwrap())).unwrap();
    let mut handoff = recorder(&wake);
    for deadline in [ReadDeadline::Immediate, ReadDeadline::At(Instant::now())] {
        let mut prepared =
            PreparedNativeWait::new(&wake, NativePollOwner::NativeMain, deadline).unwrap();
        assert_eq!(
            prepared.wait_with(&mut handoff).unwrap(),
            ControlWakeOutcome::DeadlineElapsed
        );
    }
    let mut prepared =
        PreparedNativeWait::new(&wake, NativePollOwner::NativeMain, ReadDeadline::Unbounded)
            .unwrap();
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::Notified
    ));
    assert_eq!(
        prepared.wait_with(&mut handoff).unwrap(),
        ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady)
    );
    assert!(handoff.events.is_empty());
}

fn assert_physical_wait_handoff(
    wake: Arc<ControlWake>,
    publisher: super::super::ControlNudgePublisher,
) {
    let mut prepared =
        PreparedNativeWait::new(&wake, NativePollOwner::NativeMain, ReadDeadline::Unbounded)
            .unwrap();
    let (at_wait, observed) = mpsc::channel();
    *wake.wait_stage.lock().unwrap() = Some(at_wait);
    let producer = thread::spawn(move || {
        observed
            .recv_timeout(Duration::from_secs(10))
            .expect("actual native boundary was not reached");
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady,
        )
    });
    let mut handoff = recorder(&wake);
    assert_eq!(
        prepared.wait_with(&mut handoff).unwrap(),
        ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady)
    );
    assert!(matches!(
        producer.join().unwrap(),
        ControlNudgeOutcome::Notified
    ));
    assert_eq!(
        handoff.events,
        [HandoffEvent::Released, HandoffEvent::Reacquired]
    );
}

#[test]
fn condition_wait_releases_once_and_drops_its_guard_before_reacquiring() {
    let (wake, publisher) = ControlWake::paired();
    assert_physical_wait_handoff(wake, publisher);
}

#[test]
fn physical_poll_releases_once_and_reacquires_after_native_return() {
    let (wake, publisher) =
        ControlWake::paired_native(Arc::new(polling::Poller::new().unwrap())).unwrap();
    assert_physical_wait_handoff(wake, publisher);
}
