use super::*;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

static_assertions::assert_impl_all!(ControlWake: Send, Sync);
static_assertions::assert_impl_all!(ControlNudgePublisher: Clone, Send, Sync);
static_assertions::assert_not_impl_any!(WaitRegistration: Clone, Copy);
static_assertions::assert_not_impl_any!(NativePollLease: Clone, Copy);
static_assertions::assert_impl_all!(NativePollLease: Send);

fn stages(wake: &ControlWake) -> mpsc::Receiver<()> {
    let (sender, receiver) = mpsc::channel();
    *wake.wait_stage.lock().unwrap() = Some(sender);
    receiver
}

fn registered(stages: &mpsc::Receiver<()>) {
    stages
        .recv_timeout(Duration::from_secs(10))
        .expect("owner did not reach the actual registered wait boundary");
}

#[test]
fn notification_latch_preserves_original_atomic_address_and_api() {
    let latch = NotificationLatch::new();
    let original = (&latch as *const NotificationLatch).cast::<AtomicBool>();
    assert_eq!(original, &*latch as *const AtomicBool);
    assert!(!latch.load(Ordering::Relaxed));
    latch.store(true, Ordering::Release);
    assert!(latch.swap(false, Ordering::AcqRel));
    let (wake, _) = ControlWake::paired();
    assert!(latch.control().is_none());
    assert!(matches!(
        latch.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::RetiredSession
    ));
    latch.install_control(Arc::clone(&wake)).unwrap();
    assert!(Arc::ptr_eq(latch.control().unwrap(), &wake));
    assert!(matches!(
        latch.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::Notified
    ));
    assert!(latch.install_control(wake).is_err());
}

#[test]
fn publication_before_arm_requires_the_final_durable_predicate_recheck() {
    let (wake, publisher) = ControlWake::paired();
    let durable_request = AtomicBool::new(false);
    durable_request.store(true, Ordering::Release);
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::Notified
    ));
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    // An epoch snapshot alone cannot reconstruct pre-arm readiness. The
    // independently durable request is the authoritative final check.
    assert!(registration.poll().is_none());
    assert!(durable_request.load(Ordering::Acquire));
}

#[test]
fn publication_between_arm_and_sleep_is_durable() {
    let (wake, publisher) = ControlWake::paired();
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    assert!(registration.poll().is_none());
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::MainMailboxReady
        ),
        ControlNudgeOutcome::Notified
    ));
    assert_eq!(
        registration.wait(ReadDeadline::Unbounded),
        ControlWakeOutcome::ControlReady(ControlNudge::MainMailboxReady)
    );
}

#[test]
fn actual_condition_wait_wakes_without_input_or_periodic_polling() {
    let (wake, publisher) = ControlWake::paired();
    let stage = stages(&wake);
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let owner = thread::spawn(move || registration.wait(ReadDeadline::Unbounded));
    registered(&stage);
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::Notified
    ));
    assert_eq!(
        owner.join().unwrap(),
        ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady)
    );
}

#[test]
fn coalescing_and_other_destinations_cannot_consume_main_readiness() {
    let (wake, publisher) = ControlWake::paired();
    let main = wake.arm(ControlDestination::NativeMain).unwrap();
    let idle = wake.arm(ControlDestination::IdleScheduler).unwrap();
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::Notified
    ));
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::MainMailboxReady
        ),
        ControlNudgeOutcome::AlreadyPending
    ));
    assert!(idle.poll().is_none());
    assert!(matches!(
        publisher.nudge(
            ControlDestination::IdleScheduler,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::Notified
    ));
    let _idle_rearmed = wake.arm(ControlDestination::IdleScheduler).unwrap();
    assert_eq!(
        main.poll(),
        Some(ControlWakeOutcome::ControlReady(
            ControlNudge::MainMailboxReady
        ))
    );
    assert_eq!(
        idle.poll(),
        Some(ControlWakeOutcome::ControlReady(
            ControlNudge::HostServiceReady
        ))
    );
}

#[test]
fn cancellation_is_a_typed_hint_and_host_closing_requires_retirement() {
    let (wake, publisher) = ControlWake::paired();
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let _ = publisher.nudge(ControlDestination::NativeMain, ControlNudge::Cancellation);
    assert_eq!(
        registration.poll(),
        Some(ControlWakeOutcome::CancellationRequested)
    );
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let _ = publisher.nudge(ControlDestination::NativeMain, ControlNudge::HostClosing);
    assert_eq!(
        registration.poll(),
        Some(ControlWakeOutcome::ControlReady(ControlNudge::HostClosing))
    );
    wake.retire();
    assert_eq!(
        registration.poll(),
        Some(ControlWakeOutcome::HostClosed(HostClosure::RetiredSession))
    );
}

#[test]
fn retired_handles_cannot_wake_replacement_session() {
    let (old, publisher) = ControlWake::paired();
    let registration = old.arm(ControlDestination::NativeMain).unwrap();
    old.retire();
    assert_eq!(
        registration.wait(ReadDeadline::Unbounded),
        ControlWakeOutcome::HostClosed(HostClosure::RetiredSession)
    );
    assert!(matches!(
        old.arm(ControlDestination::NativeMain),
        Err(HostClosure::RetiredSession)
    ));
    let (replacement, _) = ControlWake::paired();
    let fresh = replacement.arm(ControlDestination::NativeMain).unwrap();
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::RetiredSession
    ));
    assert!(fresh.poll().is_none());
}

#[test]
fn native_poll_lease_excludes_both_owner_kinds_and_hands_off_on_drop() {
    let (wake, _) = ControlWake::paired();
    let lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    assert_eq!(
        wake.try_native_poll(NativePollOwner::IdleScheduler)
            .unwrap_err(),
        NativePollBusy {
            owner: NativePollOwner::NativeMain
        }
    );
    assert_eq!(
        wake.try_native_poll(NativePollOwner::NativeMain)
            .unwrap_err(),
        NativePollBusy {
            owner: NativePollOwner::NativeMain
        }
    );
    drop(lease);
    let idle = wake
        .try_native_poll(NativePollOwner::IdleScheduler)
        .unwrap();
    assert_eq!(
        wake.try_native_poll(NativePollOwner::NativeMain)
            .unwrap_err(),
        NativePollBusy {
            owner: NativePollOwner::IdleScheduler
        }
    );
    drop(idle);
    assert!(wake.try_native_poll(NativePollOwner::NativeMain).is_ok());
}

#[test]
fn unrelated_wake_does_not_restart_absolute_deadline() {
    let (wake, publisher) = ControlWake::paired();
    let start = Instant::now();
    let deadline = start + Duration::from_secs(60);
    *wake.clock.lock().unwrap() = Some(start);
    let stage = stages(&wake);
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let owner = thread::spawn(move || registration.wait(ReadDeadline::At(deadline)));
    registered(&stage);
    // Advance the test clock, then wake this Condvar for an unrelated
    // destination. A relative-duration restart would enter another wait.
    *wake.clock.lock().unwrap() = Some(deadline);
    let _ = publisher.nudge(
        ControlDestination::IdleScheduler,
        ControlNudge::HostServiceReady,
    );
    assert_eq!(owner.join().unwrap(), ControlWakeOutcome::DeadlineElapsed);
}

#[test]
fn immediate_and_elapsed_deadlines_do_not_enter_a_physical_wait() {
    let (wake, _) = ControlWake::paired();
    let stage = stages(&wake);
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    assert_eq!(
        registration.wait(ReadDeadline::Immediate),
        ControlWakeOutcome::DeadlineElapsed
    );
    assert_eq!(
        registration.wait(ReadDeadline::At(Instant::now())),
        ControlWakeOutcome::DeadlineElapsed
    );
    assert!(matches!(stage.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

fn blind_fallback(fault: FallbackFault, destination: ControlDestination) {
    let (wake, publisher) = ControlWake::paired();
    let stage = stages(&wake);
    *wake.fallback_fault.lock().unwrap() = Some(fault);
    let registration = wake.arm(destination).unwrap();
    let owner_kind = match destination {
        ControlDestination::NativeMain => NativePollOwner::NativeMain,
        ControlDestination::IdleScheduler => NativePollOwner::IdleScheduler,
    };
    let mut lease = wake.try_native_poll(owner_kind).unwrap();
    let owner = thread::spawn(move || {
        lease
            .wait(
                &registration,
                &mut polling::Events::new(),
                ReadDeadline::Unbounded,
            )
            .unwrap()
    });
    registered(&stage);
    let _ = publisher.nudge(destination, ControlNudge::HostServiceReady);
    // The injected blind transition has reentered the real fallback wait
    // despite durable readiness. Observe that exact violation, then close to
    // rescue it; an unrelated timer expiry is never the failure witness.
    registered(&stage);
    wake.retire();
    let outcome = owner.join().unwrap();
    assert_eq!(
        outcome,
        ControlWakeOutcome::HostClosed(HostClosure::RetiredSession)
    );
    assert_ne!(
        outcome,
        ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady)
    );
}

#[test]
fn negative_blind_idle_sleep_reenters_wait_with_durable_service_ready() {
    blind_fallback(
        FallbackFault::BlindIdleSleep,
        ControlDestination::IdleScheduler,
    );
}

#[test]
fn negative_blind_no_receiver_sleep_reenters_wait_with_durable_service_ready() {
    blind_fallback(
        FallbackFault::BlindNoReceiverSleep,
        ControlDestination::NativeMain,
    );
}

#[path = "native.rs"]
mod native;
