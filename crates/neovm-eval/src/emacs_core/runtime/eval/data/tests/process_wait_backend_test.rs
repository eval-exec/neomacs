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
    let processes = FxHashMap::default();
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
        type RegistrationToken = ();

        fn capture_registration_token(&self) {}

        unsafe fn add_to_poller(
            &self,
            _poller: &polling::Poller,
            _token: (),
            _event: polling::Event,
            _mode: polling::PollMode,
        ) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected add_to_poller failure",
            ))
        }

        unsafe fn modify_on_poller(
            &self,
            _poller: &polling::Poller,
            _token: (),
            _event: polling::Event,
            _mode: polling::PollMode,
        ) -> std::io::Result<()> {
            Ok(())
        }

        unsafe fn unregister_from_poller(&self, poller: &polling::Poller, _token: ()) {
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

#[cfg(unix)]
#[test]
fn poll_registration_captures_token_once_even_if_accessor_drifts() {
    use std::os::fd::{AsRawFd, BorrowedFd, RawFd};
    use std::sync::atomic::AtomicUsize;

    struct DriftingFdSource {
        file: std::fs::File,
        captures: Arc<AtomicUsize>,
        unregisters: Arc<AtomicUsize>,
    }

    // SAFETY: `file` stays open for `self`'s entire lifetime; even though
    // `capture_registration_token` would drift if called a second time,
    // `PollRegistration` calls it once and passes the captured `Option<RawFd>`
    // back to `add_to_poller`, `modify_on_poller`, and `unregister_from_poller`.
    unsafe impl PollableResource for DriftingFdSource {
        type RegistrationToken = Option<RawFd>;

        fn capture_registration_token(&self) -> Self::RegistrationToken {
            if self.captures.fetch_add(1, Ordering::SeqCst) == 0 {
                Some(self.file.as_raw_fd())
            } else {
                None
            }
        }

        unsafe fn add_to_poller(
            &self,
            poller: &polling::Poller,
            token: Option<RawFd>,
            event: polling::Event,
            mode: polling::PollMode,
        ) -> std::io::Result<()> {
            let raw_fd = token.expect("first capture must return raw fd");
            // SAFETY: `self.file` owns `raw_fd` until after unregistration.
            unsafe {
                use polling::AsSource;
                let borrowed = BorrowedFd::borrow_raw(raw_fd);
                poller.add_with_mode(&borrowed.source(), event, mode)
            }
        }

        unsafe fn modify_on_poller(
            &self,
            poller: &polling::Poller,
            token: Option<RawFd>,
            event: polling::Event,
            mode: polling::PollMode,
        ) -> std::io::Result<()> {
            let raw_fd = token.expect("captured raw fd must persist on modify");
            // SAFETY: `self.file` owns `raw_fd` until after unregistration.
            let borrowed = unsafe { BorrowedFd::borrow_raw(raw_fd) };
            poller.modify_with_mode(borrowed, event, mode)
        }

        unsafe fn unregister_from_poller(&self, poller: &polling::Poller, token: Option<RawFd>) {
            self.unregisters.fetch_add(1, Ordering::SeqCst);
            let raw_fd = token.expect("captured raw fd must persist on drop");
            // SAFETY: `self.file` owns `raw_fd` until after unregistration.
            let borrowed = unsafe { BorrowedFd::borrow_raw(raw_fd) };
            let _ = poller.delete(borrowed);
        }
    }

    let (_reader, writer) = std::io::pipe().expect("pipe");
    let writer_file = std::fs::File::from(std::os::fd::OwnedFd::from(writer));
    let keep_open = writer_file.try_clone().expect("dup writer");
    let poller = Arc::new(polling::Poller::new().expect("poller"));
    let captures = Arc::new(AtomicUsize::new(0));
    let unregisters = Arc::new(AtomicUsize::new(0));
    let reg = PollRegistration::new(DriftingFdSource {
        file: writer_file,
        captures: Arc::clone(&captures),
        unregisters: Arc::clone(&unregisters),
    });

    reg.register(
        &poller,
        polling::Event::writable(95),
        polling::PollMode::Level,
    )
    .expect("register writable");
    reg.modify(&poller, polling::Event::all(95), polling::PollMode::Level)
        .expect("modify uses captured token");
    assert_eq!(captures.load(Ordering::SeqCst), 1);

    let mut events = polling::Events::new();
    poller
        .wait(&mut events, Some(Duration::from_millis(200)))
        .expect("poll before drop");
    assert!(events.iter().any(|ev| ev.key == 95));

    drop(reg);

    assert_eq!(captures.load(Ordering::SeqCst), 1);
    assert_eq!(unregisters.load(Ordering::SeqCst), 1);
    events.clear();
    poller
        .wait(&mut events, Some(Duration::ZERO))
        .expect("poll after drop");
    assert!(
        events.iter().all(|ev| ev.key != 95),
        "captured token must unregister original fd before closing"
    );
    drop(keep_open);
}

#[cfg(unix)]
#[test]
fn poll_registration_real_master_pty_unregisters_before_drop() {
    let pair = portable_pty::native_pty_system()
        .openpty(portable_pty::PtySize::default())
        .expect("openpty");
    // `try_clone_reader` dups the master fd, keeping the open file description
    // alive across `drop(reg)` so epoll would retain key 97 unless `Drop`
    // deleted the captured `MasterPtyRegistrationToken` fd before closing.
    let keep_open = pair.master.try_clone_reader().expect("dup master reader");
    let poller = Arc::new(polling::Poller::new().expect("poller"));
    let reg = PollRegistration::new(pair.master);

    reg.register(
        &poller,
        polling::Event::writable(97),
        polling::PollMode::Level,
    )
    .expect("register master pty");
    reg.modify(&poller, polling::Event::all(97), polling::PollMode::Level)
        .expect("modify master pty");

    let mut events = polling::Events::new();
    poller
        .wait(&mut events, Some(Duration::from_millis(200)))
        .expect("poll master pty before drop");
    assert!(events.iter().any(|ev| ev.key == 97));

    drop(reg);

    events.clear();
    poller
        .wait(&mut events, Some(Duration::ZERO))
        .expect("poll master pty after drop");
    assert!(
        events.iter().all(|ev| ev.key != 97),
        "stale event remained after PollRegistration<Box<dyn MasterPty + Send>> drop"
    );
    drop(keep_open);
    drop(pair.slave);
}

#[cfg(target_os = "linux")]
#[test]
fn poll_zero_timeout_linux_handles_idle_and_notify_fallback() {
    let _hooks = crate::test_utils::unit_harness();
    let backend = ProcessWaitBackend::new();
    let poller = backend.poller.clone().expect("poller");
    let processes = FxHashMap::default();

    let idle = backend
        .poll_zero_timeout_linux(
            &poller,
            &processes,
            ProcessWaitBackendInterest::NotificationsAndProcesses,
        )
        .expect("idle zero-timeout epoll_wait should succeed without fallback");
    assert!(!idle.has_notification_wakeup());
    assert!(!idle.has_ready_processes());
    assert!(!idle.has_writable_processes());

    // Direct `poller.notify()` triggers `NOTIFY_KEY` without setting
    // `notification_pending`, so the zero-timeout fast path must return `None`
    // and let `poll_events` drain and rearm `polling`'s internal eventfd.
    poller.notify().expect("notify poller");
    assert!(
        backend
            .poll_zero_timeout_linux(
                &poller,
                &processes,
                ProcessWaitBackendInterest::NotificationsAndProcesses
            )
            .is_none(),
        "NOTIFY_KEY must fall back to polling::Poller::wait so eventfd is drained"
    );
    let drained = backend
        .poll_events(
            &poller,
            &processes,
            ProcessWaitDeadline::Poll,
            ProcessWaitBackendInterest::NotificationsAndProcesses,
        )
        .expect("fallback poll_events");
    assert!(!drained.has_ready_processes());
    // After fallback drained eventfd, zero-timeout fast path succeeds again.
    let idle_after = backend
        .poll_zero_timeout_linux(
            &poller,
            &processes,
            ProcessWaitBackendInterest::NotificationsAndProcesses,
        )
        .expect("fast path resumes after fallback drained eventfd");
    assert!(!idle_after.has_notification_wakeup());

    // `WaitNotifier::notify()` sets `notification_pending` AND notifies the
    // poller's internal eventfd (`NOTIFY_KEY`). Because `NOTIFY_KEY` fires,
    // `poll_zero_timeout_linux` returns `None` so `poll_events` drains the
    // eventfd and delivers `notification_wakeup == true`.
    let notifier = backend.notify_handle().expect("notify handle");
    notifier.notify();
    assert!(
        backend
            .poll_zero_timeout_linux(
                &poller,
                &processes,
                ProcessWaitBackendInterest::NotificationsAndProcesses
            )
            .is_none(),
        "WaitNotifier::notify wakes NOTIFY_KEY and falls back to poll_events"
    );
    let woken = backend
        .poll_events(
            &poller,
            &processes,
            ProcessWaitDeadline::Poll,
            ProcessWaitBackendInterest::NotificationsAndProcesses,
        )
        .expect("poll_events notification wakeup");
    assert!(woken.has_notification_wakeup());
    // Eventfd is drained and notification_pending is cleared; fast path resumes.
    let idle_final = backend
        .poll_zero_timeout_linux(
            &poller,
            &processes,
            ProcessWaitBackendInterest::NotificationsAndProcesses,
        )
        .expect("fast path resumes after WaitNotifier wakeup");
    assert!(!idle_final.has_notification_wakeup());
}

#[test]
fn process_status_symbol_symid_roundtrip_and_cons_forms() {
    let _hooks = crate::test_utils::unit_harness();
    for status in ProcessStatusSymbol::gnu_public_domain() {
        let val = status.value();
        assert_eq!(ProcessStatusSymbol::from_symbol_value(val), Some(status));
        assert_eq!(ProcessStatusSymbol::from_status_value(val), Some(status));
        let proper_list = Value::list(vec![val, Value::fixnum(0)]);
        assert_eq!(
            ProcessStatusSymbol::from_status_value(proper_list),
            Some(status)
        );
        // Improper dotted pair `(exit . 0)` is not a proper status list.
        let dotted = Value::cons(val, Value::fixnum(0));
        assert_eq!(ProcessStatusSymbol::from_status_value(dotted), None);
    }
    assert_eq!(
        ProcessStatusSymbol::from_symbol_value(Value::symbol("not-a-status")),
        None
    );
    assert_eq!(ProcessStatusSymbol::from_symbol_value(Value::NIL), None);
    assert_eq!(
        ProcessStatusSymbol::from_symbol_value(Value::fixnum(42)),
        None
    );

    // Cyclic cons lists must terminate and return `None` (no infinite loop).
    let self_cycle = Value::cons(ProcessStatusSymbol::Exit.value(), Value::NIL);
    self_cycle.set_cdr(self_cycle);
    assert_eq!(ProcessStatusSymbol::from_status_value(self_cycle), None);

    let two_cycle_tail = Value::cons(Value::fixnum(0), Value::NIL);
    let two_cycle = Value::cons(ProcessStatusSymbol::Exit.value(), two_cycle_tail);
    two_cycle_tail.set_cdr(two_cycle);
    assert_eq!(ProcessStatusSymbol::from_status_value(two_cycle), None);

    let lasso_tail = Value::cons(Value::fixnum(0), Value::NIL);
    lasso_tail.set_cdr(lasso_tail);
    let lasso = Value::cons(ProcessStatusSymbol::Signal.value(), lasso_tail);
    assert_eq!(ProcessStatusSymbol::from_status_value(lasso), None);
}

#[test]
fn process_manager_update_tick_tracks_unnotified_status_changes() {
    let _hooks = crate::test_utils::unit_harness();
    let mut manager = ProcessManager::new();
    assert!(!manager.has_unnotified_status_change());
    manager.record_status_change(StatusChangeSite::HandleChildSignal, 1);
    assert!(manager.has_unnotified_status_change());
    manager.mark_status_notify_walked();
    assert!(!manager.has_unnotified_status_change());
    manager.record_status_change(StatusChangeSite::HandleChildSignal, 1);
    assert!(manager.has_unnotified_status_change());
}

#[test]
fn find_by_buffer_id_and_process_ids_for_buffer_return_newest_first() {
    let _hooks = crate::test_utils::unit_harness();
    let mut manager = ProcessManager::new();
    let buffer_id = neovm_value::value::buffer_vocab::BufferId(42);
    let buffer = Value::make_buffer(buffer_id);
    let mut created = Vec::new();
    for i in 0..16 {
        let id = manager.create_process_record(
            neovm_value::heap_types::LispString::from_utf8(&format!("buf-proc-{i}")),
            buffer,
            neovm_value::heap_types::LispString::from_utf8("/bin/true"),
            Vec::new(),
            ProcessKind::Pipe,
            None,
            ProcessCodingSystems {
                decode: Value::NIL,
                encode: Value::NIL,
            },
        );
        created.push(id);
    }
    let newest = *created.last().expect("newest process id");
    assert_eq!(
        manager.find_by_buffer_id(buffer_id),
        Some(newest),
        "find_by_buffer_id must match GNU Vprocess_alist newest-first order"
    );
    let mut expected_desc = created;
    expected_desc.reverse();
    assert_eq!(
        manager.process_ids_for_buffer(buffer_id),
        expected_desc,
        "process_ids_for_buffer must return newest-first (descending ProcessId)"
    );
}
