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

/// Epoll proof: holding a `try_clone()` of a registered pipe writer keeps the
/// kernel open file description alive across `close()`, so dropping
/// `PollRegistration<File>` leaves zero epoll events only if `Drop` called
/// `poller.delete()` BEFORE closing the descriptor.
#[cfg(target_os = "linux")]
#[test]
fn poll_registration_drop_unregisters_before_closing() {
    let (_reader, writer) = std::io::pipe().expect("pipe");
    let writer_file = std::fs::File::from(std::os::fd::OwnedFd::from(writer));
    let keep_open = writer_file.try_clone().expect("dup writer");
    let poller = Arc::new(polling::Poller::new().expect("poller"));
    let reg = PollRegistration::new(writer_file);
    reg.register(
        &poller,
        polling::Event::writable(77),
        polling::PollMode::Level,
    )
    .expect("register writable");

    let mut events = polling::Events::new();
    poller
        .wait(&mut events, Some(Duration::from_millis(200)))
        .expect("poll before drop");
    assert!(events.iter().any(|ev| ev.key == 77));

    drop(reg);

    events.clear();
    poller
        .wait(&mut events, Some(Duration::ZERO))
        .expect("poll after drop");
    assert!(
        events.iter().all(|ev| ev.key != 77),
        "stale epoll event remained after PollRegistration::drop"
    );
    drop(keep_open);
}

#[cfg(target_os = "linux")]
#[test]
fn poll_registration_into_inner_unregisters_before_returning() {
    let (_reader, writer) = std::io::pipe().expect("pipe");
    let writer_file = std::fs::File::from(std::os::fd::OwnedFd::from(writer));
    let poller = Arc::new(polling::Poller::new().expect("poller"));
    let reg = PollRegistration::new(writer_file);
    reg.register(
        &poller,
        polling::Event::writable(88),
        polling::PollMode::Level,
    )
    .expect("register writable");

    let inner = reg.into_inner();
    let mut events = polling::Events::new();
    poller
        .wait(&mut events, Some(Duration::ZERO))
        .expect("poll after into_inner");
    assert!(
        events.iter().all(|ev| ev.key != 88),
        "stale epoll event remained after PollRegistration::into_inner"
    );
    drop(inner);
}

#[cfg(unix)]
#[test]
fn poll_registration_rejects_binding_to_a_second_poller() {
    let (_reader, writer) = std::io::pipe().expect("pipe");
    let writer_file = std::fs::File::from(std::os::fd::OwnedFd::from(writer));
    let poller_a = Arc::new(polling::Poller::new().expect("poller_a"));
    let poller_b = Arc::new(polling::Poller::new().expect("poller_b"));
    let reg = PollRegistration::new(writer_file);
    reg.register(
        &poller_a,
        polling::Event::writable(91),
        polling::PollMode::Level,
    )
    .expect("register on poller_a");
    let err = reg
        .register(
            &poller_b,
            polling::Event::writable(91),
            polling::PollMode::Level,
        )
        .expect_err("must reject second distinct poller");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    let mod_err = reg
        .modify(&poller_b, polling::Event::all(91), polling::PollMode::Level)
        .expect_err("modify must reject second distinct poller");
    assert_eq!(mod_err.kind(), std::io::ErrorKind::InvalidInput);
}

#[cfg(unix)]
#[test]
fn poll_registration_failed_add_drop_is_safe_and_leaves_no_event() {
    use std::sync::atomic::AtomicUsize;

    struct FailingAddSource {
        file: std::fs::File,
        unregisters: Arc<AtomicUsize>,
    }

    // SAFETY: `file` is owned exclusively and its raw descriptor never changes
    // over `self`'s lifetime; `unregister_from_poller` deletes the same
    // descriptor and tolerates `NotFound` when `add_to_poller` failed.
    unsafe impl PollableResource for FailingAddSource {
        unsafe fn add_to_poller(
            &self,
            _poller: &polling::Poller,
            _event: polling::Event,
            _mode: polling::PollMode,
        ) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected add_to_poller failure",
            ))
        }

        fn modify_on_poller(
            &self,
            _poller: &polling::Poller,
            _event: polling::Event,
            _mode: polling::PollMode,
        ) -> std::io::Result<()> {
            Ok(())
        }

        fn unregister_from_poller(&self, poller: &polling::Poller) {
            self.unregisters.fetch_add(1, Ordering::SeqCst);
            let _ = poller.delete(&self.file);
        }
    }

    let (_reader, writer) = std::io::pipe().expect("pipe");
    let writer_file = std::fs::File::from(std::os::fd::OwnedFd::from(writer));
    let poller = Arc::new(polling::Poller::new().expect("poller"));
    let other_poller = Arc::new(polling::Poller::new().expect("other poller"));
    let unregisters = Arc::new(AtomicUsize::new(0));
    let reg = PollRegistration::new(FailingAddSource {
        file: writer_file,
        unregisters: Arc::clone(&unregisters),
    });

    let err = reg
        .register(
            &poller,
            polling::Event::writable(93),
            polling::PollMode::Level,
        )
        .expect_err("injected add_to_poller error must propagate");
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(unregisters.load(Ordering::SeqCst), 0);

    // Binding is retained in `OnceLock` even after failed `add_to_poller`.
    let second_err = reg
        .register(
            &other_poller,
            polling::Event::writable(93),
            polling::PollMode::Level,
        )
        .expect_err("binding must remain pinned to the original poller");
    assert_eq!(second_err.kind(), std::io::ErrorKind::InvalidInput);

    drop(reg);

    assert_eq!(
        unregisters.load(Ordering::SeqCst),
        1,
        "Drop must invoke unregister_from_poller once without panicking"
    );
    let mut events = polling::Events::new();
    poller
        .wait(&mut events, Some(Duration::ZERO))
        .expect("poll after failed add + drop");
    assert!(events.iter().next().is_none());
}
