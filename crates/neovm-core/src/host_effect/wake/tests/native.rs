use super::*;

fn native_pair() -> (Arc<ControlWake>, ControlNudgePublisher) {
    ControlWake::paired_native(Arc::new(polling::Poller::new().unwrap())).unwrap()
}

#[test]
fn loopback_cancellation_source_close_produces_native_reader_readiness() {
    let poller = polling::Poller::new().unwrap();
    let (reader, writer) = loopback_wake_stream_pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    writer.set_nonblocking(true).unwrap();
    // SAFETY: the socket is deregistered before it is dropped, below. This
    // fixture owns the only physical waiter and the unique write endpoint.
    unsafe {
        poller.add(&reader, polling::Event::readable(43)).unwrap();
    }
    drop(writer);
    let mut events = polling::Events::new();
    poller.wait(&mut events, None).unwrap();
    assert!(events.iter().any(|event| event.key == 43));
    use std::io::Read;
    assert_eq!((&reader).read(&mut [0]).unwrap(), 0);
    poller.delete(&reader).unwrap();
}

#[test]
fn registered_native_source_wakes_without_a_fake_input_event() {
    let (wake, publisher) = native_pair();
    let stage = stages(&wake);
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let mut lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    let owner = thread::spawn(move || {
        let mut events = polling::Events::new();
        let outcome = lease
            .wait(&registration, &mut events, ReadDeadline::Unbounded)
            .unwrap();
        (
            outcome,
            events
                .iter()
                .any(|event| event.key == ControlEventKey::reserved().0),
        )
    });
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
        (
            ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady),
            true
        )
    );
}

#[test]
fn native_failure_after_publication_closes_unique_writer_and_wakes_kernel_reader() {
    let (wake, publisher) = native_pair();
    let native = wake.native.as_ref().unwrap();
    native.fail_next_notification.store(true, Ordering::Release);
    let stage = stages(&wake);
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let mut lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    let owner = thread::spawn(move || {
        let mut events = polling::Events::new();
        let outcome = lease
            .wait(&registration, &mut events, ReadDeadline::Unbounded)
            .unwrap();
        (
            outcome,
            events
                .iter()
                .any(|event| event.key == ControlEventKey::reserved().0),
        )
    });
    registered(&stage);
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::WakeFailedAfterPublication(_)
    ));
    let (outcome, kernel_source_ready) = owner.join().unwrap();
    assert_eq!(
        outcome,
        ControlWakeOutcome::HostClosed(HostClosure::NativeWakeFailed)
    );
    assert!(
        kernel_source_ready,
        "durable closure must interrupt the registered native wait"
    );
    assert!(native.writer.lock().unwrap().is_none());
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::RetiredSession
    ));
}

#[test]
fn negative_lost_wake_after_notify_failure_has_no_kernel_readiness_when_close_is_omitted() {
    let (wake, publisher) = native_pair();
    let native = wake.native.as_ref().unwrap();
    native.fail_next_notification.store(true, Ordering::Release);
    native.suppress_close.store(true, Ordering::Release);
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let _lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::WakeFailedAfterPublication(_)
    ));
    assert_eq!(
        registration.poll(),
        Some(ControlWakeOutcome::HostClosed(
            HostClosure::NativeWakeFailed
        ))
    );
    // Inspect the registered OS source directly under the sole fixture lease.
    // This is a readiness query, not a timeout used to infer a sleeping thread.
    let mut events = polling::Events::new();
    native
        .poller
        .wait(&mut events, Some(Duration::ZERO))
        .unwrap();
    assert!(
        !events
            .iter()
            .any(|event| event.key == ControlEventKey::reserved().0),
        "negative mutation must lose the kernel wake despite durable closure"
    );
    native.suppress_close.store(false, Ordering::Release);
    native.close_writer();
}

#[test]
fn retirement_releases_an_armed_kernel_wait_and_old_publishers_stay_retired() {
    let (wake, publisher) = native_pair();
    let stage = stages(&wake);
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let mut lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    let owner = thread::spawn(move || {
        let mut events = polling::Events::new();
        let outcome = lease
            .wait(&registration, &mut events, ReadDeadline::Unbounded)
            .unwrap();
        (
            outcome,
            events
                .iter()
                .any(|event| event.key == ControlEventKey::reserved().0),
        )
    });
    registered(&stage);
    wake.retire();
    assert_eq!(
        owner.join().unwrap(),
        (
            ControlWakeOutcome::HostClosed(HostClosure::RetiredSession),
            true
        )
    );
    assert!(matches!(
        publisher.nudge(
            ControlDestination::NativeMain,
            ControlNudge::MainMailboxReady
        ),
        ControlNudgeOutcome::RetiredSession
    ));
}

#[test]
fn native_lease_rejects_another_session_or_destination_registration() {
    let (wake, _) = native_pair();
    let (other, _) = native_pair();
    let wrong_session = other.arm(ControlDestination::NativeMain).unwrap();
    let wrong_destination = wake.arm(ControlDestination::IdleScheduler).unwrap();
    let mut lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    let mut events = polling::Events::new();
    assert_eq!(
        lease
            .wait(&wrong_session, &mut events, ReadDeadline::Immediate)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        lease
            .wait(&wrong_destination, &mut events, ReadDeadline::Immediate)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn unrelated_descriptor_remains_in_native_observation() {
    let (wake, _) = native_pair();
    let native = wake.native.as_ref().unwrap();
    let (reader, mut writer) = wake_stream_pair().unwrap();
    // SAFETY: reader is deregistered below, before either socket is dropped.
    unsafe {
        native
            .poller
            .add(&reader, polling::Event::readable(41))
            .unwrap()
    };
    use std::io::Write;
    writer.write_all(&[1]).unwrap();
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let mut lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    let mut events = polling::Events::new();
    assert_eq!(
        lease
            .wait(&registration, &mut events, ReadDeadline::Unbounded)
            .unwrap(),
        ControlWakeOutcome::NativeReady
    );
    assert!(events.iter().any(|event| event.key == 41));
    native.poller.delete(&reader).unwrap();
}

#[test]
fn immediate_native_snapshot_polls_descriptors_once_without_blocking() {
    let (wake, _) = native_pair();
    let native = wake.native.as_ref().unwrap();
    let (reader, mut writer) = wake_stream_pair().unwrap();
    // SAFETY: reader is deregistered below, before either socket is dropped.
    unsafe {
        native
            .poller
            .add(&reader, polling::Event::readable(42))
            .unwrap()
    };
    let registration = wake.arm(ControlDestination::IdleScheduler).unwrap();
    let mut lease = wake
        .try_native_poll(NativePollOwner::IdleScheduler)
        .unwrap();
    let mut events = polling::Events::new();
    assert_eq!(
        lease
            .wait(&registration, &mut events, ReadDeadline::Immediate)
            .unwrap(),
        ControlWakeOutcome::DeadlineElapsed
    );
    use std::io::Write;
    writer.write_all(&[1]).unwrap();
    assert_eq!(
        lease
            .wait(&registration, &mut events, ReadDeadline::Immediate)
            .unwrap(),
        ControlWakeOutcome::NativeReady
    );
    assert!(events.iter().any(|event| event.key == 42));
    native.poller.delete(&reader).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn poller_rearm_failure_selects_paired_condition_on_next_wait() {
    let (wake, publisher) = native_pair();
    let native = wake.native.as_ref().unwrap();
    native.poller.delete(&native.reader).unwrap();
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let mut lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    assert!(
        lease
            .wait(
                &registration,
                &mut polling::Events::new(),
                ReadDeadline::Unbounded
            )
            .is_err()
    );
    assert_eq!(native.health.load(), NativeHealth::Failed);
    let stage = stages(&wake);
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
    let _ = publisher.nudge(
        ControlDestination::NativeMain,
        ControlNudge::HostServiceReady,
    );
    assert_eq!(
        owner.join().unwrap(),
        ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady)
    );
}
