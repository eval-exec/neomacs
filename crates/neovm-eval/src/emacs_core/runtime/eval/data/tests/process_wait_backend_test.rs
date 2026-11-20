use super::*;

/// A level-triggered descriptor whose key maps to no process stays ready.
/// The wait must hand that batch back to its caller and not re-poll it in
/// place, or one stale registration pegs a core until the deadline.
#[cfg(unix)]
#[test]
fn wait_backend_irrelevant_readiness_returns_to_the_caller_without_spinning() {
    use std::io::Write;

    let _hooks = crate::test_utils::unit_harness();
    let backend = ProcessWaitBackend::new();
    let poller = backend.poller.clone().expect("poller");
    let processes = HashMap::new();
    let (reader, mut writer) = UnixStream::pair().expect("socket pair");
    // SAFETY: `reader` outlives the registration, which is deleted below.
    unsafe {
        poller
            .add_with_mode(
                &reader,
                polling::Event::readable(1),
                polling::PollMode::Level,
            )
            .expect("register an fd that maps to no process");
    }
    writer.write_all(b"wake").expect("make the fd readable");

    BACKEND_POLL_COUNT.store(0, Ordering::Relaxed);
    let deadline = Instant::now() + Duration::from_millis(200);
    // Stand in for the caller's wait loop: it re-checks timers and the
    // deadline after every returned batch, then waits again.  (The public
    // entry also consults the system layer's signal state, which this
    // crate's unit runtime does not carry.)
    let mut now = Instant::now();
    while now < deadline {
        let events = backend
            .poll_events(
                &poller,
                &processes,
                ProcessWaitDeadline::from(deadline - now),
                ProcessWaitBackendInterest::ProcessesOnly,
            )
            .expect("poller backend");
        assert!(!events.has_ready_processes());
        assert!(!events.has_writable_processes());
        now = Instant::now();
    }
    let polls = BACKEND_POLL_COUNT.load(Ordering::Relaxed);
    poller.delete(&reader).expect("deregister");
    assert!(
        polls < 20,
        "a 200ms wait on readiness that maps to no process polled {polls} times"
    );
}
