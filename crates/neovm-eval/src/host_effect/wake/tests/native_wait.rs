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
    fn release(&mut self, _permit: RealWaitPermit) -> ReleaseWaitBound {
        self.events.push(HandoffEvent::Released);
        ReleaseWaitBound::CallerDeadline
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

#[test]
fn earlier_peer_bound_clamps_return_to_caller_poll_and_condvar_waits() {
    struct PeerBoundHandoff {
        peer_deadline: Instant,
        releases: usize,
    }

    impl NativeWaitHandoff for PeerBoundHandoff {
        fn release(&mut self, _permit: RealWaitPermit) -> ReleaseWaitBound {
            self.releases += 1;
            ReleaseWaitBound::EarlierPeer(self.peer_deadline)
        }

        fn reacquire(&mut self) {}
    }

    for (label, wake) in [
        (
            "native-poller",
            ControlWake::paired_native(Arc::new(polling::Poller::new().unwrap()))
                .unwrap()
                .0,
        ),
        ("condvar-fallback", ControlWake::paired().0),
    ] {
        let start = Instant::now();
        let peer_deadline = start + Duration::from_millis(30);
        let caller_deadline = start + Duration::from_secs(5);
        let mut prepared = PreparedNativeWait::new(
            &wake,
            NativePollOwner::NativeMain,
            ReadDeadline::At(caller_deadline),
        )
        .unwrap();
        let mut handoff = PeerBoundHandoff {
            peer_deadline,
            releases: 0,
        };
        let outcome = prepared.wait_with(&mut handoff).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(
            outcome,
            ControlWakeOutcome::NativeReady,
            "{label}: clamped wait must return NativeReady so caller loop dispatches expired peer"
        );
        assert_eq!(handoff.releases, 1, "{label}: single physical release");
        assert!(
            elapsed < Duration::from_secs(1),
            "{label}: physical wait must not outlive earlier peer deadline (elapsed={elapsed:?})"
        );
    }
}

#[test]
fn earlier_peer_bound_recheck_in_wait_reenters_release_before_service_completion() {
    struct RecheckServiceHandoff {
        peer_deadline: Instant,
        trigger_nudge: Option<mpsc::Sender<()>>,
        releases: usize,
    }

    impl NativeWaitHandoff for RecheckServiceHandoff {
        fn release(&mut self, _permit: RealWaitPermit) -> ReleaseWaitBound {
            self.releases += 1;
            if self.releases == 1 {
                ReleaseWaitBound::EarlierPeer(self.peer_deadline)
            } else {
                // On the re-entered release after peer_deadline elapsed,
                // ask the background thread to deliver HostServiceReady so
                // release() itself never locks ControlWake's predicate mutex.
                if let Some(tx) = self.trigger_nudge.take() {
                    let _ = tx.send(());
                }
                ReleaseWaitBound::CallerDeadline
            }
        }

        fn reacquire(&mut self) {}

        fn on_peer_deadline_elapsed(&self) -> PeerDeadlineBehavior {
            PeerDeadlineBehavior::RecheckInWait
        }
    }

    for (label, (wake, publisher)) in [
        (
            "native-poller",
            ControlWake::paired_native(Arc::new(polling::Poller::new().unwrap())).unwrap(),
        ),
        ("condvar-fallback", ControlWake::paired()),
    ] {
        let (tx, rx) = mpsc::channel();
        let nudger = thread::spawn(move || {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("second release must fire after peer deadline");
            publisher.nudge(
                ControlDestination::NativeMain,
                ControlNudge::HostServiceReady,
            )
        });
        let start = Instant::now();
        let peer_deadline = start + Duration::from_millis(30);
        let mut prepared =
            PreparedNativeWait::new(&wake, NativePollOwner::NativeMain, ReadDeadline::Unbounded)
                .unwrap();
        let mut handoff = RecheckServiceHandoff {
            peer_deadline,
            trigger_nudge: Some(tx),
            releases: 0,
        };
        let outcome = prepared.wait_with(&mut handoff).unwrap();
        let elapsed = start.elapsed();
        assert!(matches!(
            nudger.join().unwrap(),
            ControlNudgeOutcome::Notified | ControlNudgeOutcome::AlreadyPending
        ));
        assert_eq!(
            outcome,
            ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady),
            "{label}: RecheckInWait must stay in wait_with until HostServiceReady arrives"
        );
        assert_eq!(
            handoff.releases, 2,
            "{label}: must wake at peer_deadline and re-enter release() once"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "{label}: unbounded wait must not hang past earlier peer deadline (elapsed={elapsed:?})"
        );
    }
}
