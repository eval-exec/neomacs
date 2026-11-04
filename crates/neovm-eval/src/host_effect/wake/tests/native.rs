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

#[test]
fn loopback_pair_closes_listener_and_only_carries_the_nudge_byte() {
    use std::io::{Read, Write};
    let (mut reader, mut writer) = loopback_wake_stream_pair().unwrap();
    assert_eq!(reader.peer_addr().unwrap(), writer.local_addr().unwrap());
    assert_eq!(reader.local_addr().unwrap(), writer.peer_addr().unwrap());
    let address = writer.peer_addr().unwrap();
    assert!(
        std::net::TcpStream::connect_timeout(&address, LOOPBACK_WAKE_CONNECT_TIMEOUT).is_err(),
        "no listener may remain after pairing"
    );
    writer.write_all(&[1]).unwrap();
    let mut byte = [0];
    reader.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [1]);
}

#[test]
fn foreign_first_loopback_peer_fails_closed_without_reading_its_data() {
    let listener = LoopbackWakeListener::bind().unwrap();
    let address = listener.0.local_addr().unwrap();
    // Connect completion enqueues this client before the private writer is
    // even constructed. No scheduler timing, sleep or foreign byte is needed.
    let foreign =
        std::net::TcpStream::connect_timeout(&address, LOOPBACK_WAKE_CONNECT_TIMEOUT).unwrap();
    let result = listener.into_pair();
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("an unrelated peer must never become a wake source"),
    };
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(
        std::net::TcpStream::connect_timeout(&address, LOOPBACK_WAKE_CONNECT_TIMEOUT).is_err(),
        "rejection must close the listener before returning"
    );
    drop(foreign);
}

#[test]
fn negative_omitting_only_peer_validation_accepts_foreign_wake_injection() {
    use std::io::{Read, Write};
    let listener = LoopbackWakeListener::bind().unwrap();
    // Darwin resets an unaccepted queued connection when the final listener
    // closes. Keep this fixture's private writer connected through TCP_NODELAY;
    // otherwise EINVAL masks the foreign-reader injection being tested. The
    // production constructor still closes its own listener after one accept.
    #[cfg(target_vendor = "apple")]
    let pending_listener = listener.0.try_clone().unwrap();
    let address = listener.0.local_addr().unwrap();
    let mut foreign =
        std::net::TcpStream::connect_timeout(&address, LOOPBACK_WAKE_CONNECT_TIMEOUT).unwrap();
    let foreign_address = foreign.local_addr().unwrap();
    // The same connect/accept/close path: only the validating constructor is
    // replaced. The bad reader remains connected to the foreign client.
    let mut unchecked = listener
        .into_pair_with(|reader, _peer, writer| Ok(LoopbackWakePair { reader, writer }))
        .unwrap();
    #[cfg(target_vendor = "apple")]
    let _private_peer = {
        let (peer, address) = pending_listener.accept().unwrap();
        assert_eq!(address, unchecked.writer.local_addr().unwrap());
        drop(pending_listener);
        peer
    };
    assert_eq!(unchecked.reader.peer_addr().unwrap(), foreign_address);
    assert_ne!(
        unchecked.reader.peer_addr().unwrap(),
        unchecked.writer.local_addr().unwrap(),
        "negative control must expose the unpaired connected endpoints"
    );
    foreign.write_all(&[1]).unwrap();
    let mut byte = [0];
    unchecked.reader.read_exact(&mut byte).unwrap();
    assert_eq!(
        byte,
        [1],
        "omitting validation lets the foreign peer inject a wake"
    );
    assert!(
        std::net::TcpStream::connect_timeout(&address, LOOPBACK_WAKE_CONNECT_TIMEOUT).is_err(),
        "negative control must retain the real listener-close behavior"
    );
}
