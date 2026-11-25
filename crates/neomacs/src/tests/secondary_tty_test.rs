#[cfg(unix)]
#[test]
fn opened_secondary_tty_reads_bytes_for_its_own_frame_and_uses_device_size() {
    use neovm_core::emacs_core::eval::QuitRequest;
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::time::Duration;

    let mut master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open slave");
    rustix::termios::tcsetwinsize(
        &slave,
        rustix::termios::Winsize {
            ws_row: 37,
            ws_col: 119,
            ws_xpixel: 0,
            ws_ypixel: 0,
        },
    )
    .expect("set PTY size");
    let original_modes = terminal_modes(slave.as_raw_fd());
    // Keep a slave descriptor open: macOS resets PTY state when the last one closes.

    let request = neovm_core::emacs_core::terminal::pure::TtyFrameOpenRequest::new(
        7,
        neovm_core::window::FrameId(123),
        slave_name.clone(),
        "xterm-256color".to_string(),
    )
    .expect("valid request");
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let input = neovm_core::host_input::HostInputPublisher::install(&mut context);
    let (session, opened_size, _) =
        super::SecondaryTtySession::open(&request, input, QuitRequest::new())
            .expect("open secondary TTY");

    assert_eq!(opened_size.columns(), 119);
    assert_eq!(opened_size.rows(), 37);
    master.write_all(b"z").expect("write PTY input");
    let event = context
        .receive_host_input_for_test(Duration::from_secs(2))
        .expect("secondary TTY input event");
    assert!(matches!(
        event,
        neovm_core::keyboard::InputEvent::RawTtyBytes {
            ref bytes,
            target: neovm_core::keyboard::TtyInputTarget::Terminal(7),
        } if bytes == b"z"
    ));

    drop(session);
    // Observe restoration before closing the last slave resets the PTY.
    let restored_modes = terminal_modes(slave.as_raw_fd());
    assert_eq!(restored_modes.c_iflag, original_modes.c_iflag);
    assert_eq!(restored_modes.c_oflag, original_modes.c_oflag);
    assert_eq!(restored_modes.c_cflag, original_modes.c_cflag);
    // Darwin sets PENDIN when tcsetattr restores ICANON; it is pending-input
    // state, not a mode that the application failed to restore (XNU tty.c).
    #[cfg(target_os = "macos")]
    let transient_flags = libc::PENDIN;
    #[cfg(not(target_os = "macos"))]
    let transient_flags = 0;
    assert_eq!(
        restored_modes.c_lflag & !transient_flags,
        original_modes.c_lflag & !transient_flags
    );
    assert_eq!(restored_modes.c_cc, original_modes.c_cc);
}

#[cfg(target_os = "linux")]
#[test]
fn secondary_tty_hangup_identifies_the_terminal_instead_of_closing_one_frame() {
    use neovm_core::emacs_core::eval::QuitRequest;
    use std::fs::{File, OpenOptions};
    use std::time::Duration;

    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let device = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&device)
        .expect("retain the slave during setup");
    let request = neovm_core::emacs_core::terminal::pure::TtyFrameOpenRequest::new(
        7,
        neovm_core::window::FrameId(123),
        device,
        "xterm-256color".to_owned(),
    )
    .expect("valid request");
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let input = neovm_core::host_input::HostInputPublisher::install(&mut context);
    let (session, _, _) = super::SecondaryTtySession::open(&request, input, QuitRequest::new())
        .expect("open secondary TTY");

    // GNU keyboard.c:8129 loses the terminal, not merely the frame which
    // opened its native reader. Every frame on that terminal must be retired.
    drop(master);
    let event = context
        .receive_host_input_for_test(Duration::from_secs(2))
        .expect("secondary terminal hangup event");
    assert!(
        matches!(
            &event,
            neovm_core::keyboard::InputEvent::TtyHangup { terminal_id: 7 }
        ),
        "terminal 7 loss must not become WindowClose for frame 123: {event:?}"
    );
    drop(session);
    drop(slave);
}

#[cfg(unix)]
#[test]
fn secondary_tty_bounds_cells_from_an_extreme_winsize() {
    use neovm_core::emacs_core::eval::QuitRequest;
    use std::fs::File;

    // Realistic sizes pass through untouched; only the cell count is bounded.
    let ordinary = super::bounded_tty_size(119, 37).expect("non-zero size");
    assert_eq!((ordinary.columns(), ordinary.rows()), (119, 37));
    let wide = super::bounded_tty_size(65535, 65535).expect("non-zero size");
    assert_eq!(wide.columns(), 65535);
    assert!(wide.columns() * wide.rows() <= super::MAX_SECONDARY_TTY_CELLS);
    assert!(super::bounded_tty_size(0, 24).is_none());

    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    // macOS requires an open slave for PTY ioctls and retaining device state.
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open slave");
    rustix::termios::tcsetwinsize(
        &slave,
        rustix::termios::Winsize {
            ws_row: u16::MAX,
            ws_col: u16::MAX,
            ws_xpixel: 0,
            ws_ypixel: 0,
        },
    )
    .expect("set PTY size");
    let request = neovm_core::emacs_core::terminal::pure::TtyFrameOpenRequest::new(
        7,
        neovm_core::window::FrameId(123),
        slave_name,
        "xterm-256color".to_string(),
    )
    .expect("valid request");
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let input = neovm_core::host_input::HostInputPublisher::install(&mut context);
    let (session, opened_size, _) =
        super::SecondaryTtySession::open(&request, input, QuitRequest::new())
            .expect("open secondary TTY");

    // The frame size and the renderer grid come from the same bounded query.
    assert_eq!(
        (opened_size.columns(), opened_size.rows()),
        (wide.columns(), wide.rows())
    );
    assert_eq!(
        (session.rif.width(), session.rif.height()),
        (wide.columns() as usize, wide.rows() as usize)
    );
    drop(session);
    drop(slave);
}

#[cfg(unix)]
fn terminal_modes(fd: std::os::fd::RawFd) -> libc::termios {
    let mut modes = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(unsafe { libc::tcgetattr(fd, &mut modes) }, 0, "tcgetattr");
    modes
}

#[cfg(unix)]
#[test]
fn padded_secondary_terminal_renders_and_resumes_with_its_attached_device() {
    if super::super::terminal_capabilities::tests::run_native_fixture_child() {
        return;
    }
    use neovm_core::emacs_core::eval::QuitRequest;
    use std::fs::File;
    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .unwrap(),
    );
    rustix::pty::grantpt(&master).unwrap();
    rustix::pty::unlockpt(&master).unwrap();
    let slave_name = rustix::pty::ptsname(&master, Vec::new()).unwrap();
    // macOS requires an open slave for PTY ioctls and retaining device state.
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(std::path::Path::new(slave_name.to_str().unwrap()))
        .unwrap();
    rustix::termios::tcsetwinsize(
        &slave,
        rustix::termios::Winsize {
            ws_row: 2,
            ws_col: 8,
            ws_xpixel: 0,
            ws_ypixel: 0,
        },
    )
    .unwrap();
    let request = neovm_core::emacs_core::terminal::pure::TtyFrameOpenRequest::new(
        7,
        neovm_core::window::FrameId(123),
        slave_name.to_string_lossy().into_owned(),
        "neo-app-padding".to_owned(),
    )
    .unwrap();
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let input = neovm_core::host_input::HostInputPublisher::install(&mut context);
    let (mut session, _, _) =
        super::SecondaryTtySession::open(&request, input, QuitRequest::new()).unwrap();
    drop(slave);
    for _ in 0..2 {
        super::super::tty_output::paint_to(
            &mut session.rif,
            &mut session.device.file,
            &session.device.capabilities,
        )
        .expect("padded secondary render must use the attached device snapshot");
        session.suspend(&[]).unwrap();
        session.resume(&[]).unwrap();
    }
}

/// Retain both PTY ends so closing a TtyDevice cannot reset the modes before
/// the restoration assertion observes them.
#[cfg(target_os = "linux")]
#[derive(Debug)]
struct ModeRestorationPty {
    _master: std::fs::File,
    slave: std::fs::File,
    path: String,
}

#[cfg(target_os = "linux")]
impl ModeRestorationPty {
    fn new() -> Self {
        let master = std::fs::File::from(
            rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
                .expect("open restoration PTY"),
        );
        rustix::pty::grantpt(&master).expect("grant restoration PTY");
        rustix::pty::unlockpt(&master).expect("unlock restoration PTY");
        let path = rustix::pty::ptsname(&master, Vec::new())
            .expect("restoration PTY name")
            .into_string()
            .expect("UTF-8 restoration PTY name");
        let slave = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open restoration PTY slave");
        Self {
            _master: master,
            slave,
            path,
        }
    }

    fn modes(&self) -> rustix::termios::Termios {
        rustix::termios::tcgetattr(&self.slave).expect("read restoration PTY modes")
    }

    fn assert_modes(&self, expected: &rustix::termios::Termios) {
        // Preserve the baud encoding as well as its decoded numeric speed.
        assert_eq!(format!("{:?}", self.modes()), format!("{expected:?}"));
    }

    fn open_device(&self) -> super::TtyDevice {
        super::TtyDevice::open(
            &self.path,
            super::super::tty_output::Capabilities::load("xterm-256color")
                .expect("restoration PTY capabilities"),
        )
        .expect("open restoration TtyDevice")
    }
}

#[cfg(target_os = "linux")]
#[test]
fn secondary_tty_restores_implicit_input_baud_alias_and_erase_character() {
    let pty = ModeRestorationPty::new();
    let mut initial = pty.modes();
    // Linux permits an implicit input rate: CIBAUD=0 means use the output
    // rate. A libc snapshot can replace that alias with an explicit rate.
    initial
        .control_modes
        .remove(rustix::termios::ControlModes::from_bits_retain(
            libc::CIBAUD,
        ));
    initial.special_codes[rustix::termios::SpecialCodeIndex::VERASE] = 8;
    rustix::termios::tcsetattr(&pty.slave, rustix::termios::OptionalActions::Now, &initial)
        .expect("install implicit input baud alias");
    let original = pty.modes();
    assert_eq!(original.control_modes.bits() & libc::CIBAUD, 0);
    assert_eq!(original.input_speed(), original.output_speed());
    let mut raw = original.clone();
    raw.make_raw();

    let mut device = pty.open_device();
    assert_eq!(device.erase_char, 8);
    pty.assert_modes(&raw);
    for _ in 0..2 {
        device.suspend(&[]).expect("suspend implicit-baud TTY");
        pty.assert_modes(&original);
        device.suspend(&[]).expect("suspend already suspended TTY");
        pty.assert_modes(&original);
        device.resume(&[]).expect("resume implicit-baud TTY");
        pty.assert_modes(&raw);
        device.resume(&[]).expect("resume already active TTY");
        pty.assert_modes(&raw);
    }
    drop(device);
    pty.assert_modes(&original);
}

#[cfg(target_os = "linux")]
#[test]
fn secondary_tty_restores_distinct_input_and_output_speeds() {
    let pty = ModeRestorationPty::new();
    let mut initial = pty.modes();
    initial.set_output_speed(38_400).expect("set output speed");
    initial.set_input_speed(19_200).expect("set input speed");
    rustix::termios::tcsetattr(&pty.slave, rustix::termios::OptionalActions::Now, &initial)
        .expect("install distinct PTY speeds");
    // This Linux-only control requires the kernel to retain distinct rates;
    // it does not count an ignored input-rate request as coverage.
    let original = pty.modes();
    assert_eq!(original.input_speed(), 19_200);
    assert_eq!(original.output_speed(), 38_400);
    let mut raw = original.clone();
    raw.make_raw();
    let mut device = pty.open_device();
    pty.assert_modes(&raw);
    device.suspend(&[]).expect("suspend distinct-speed TTY");
    pty.assert_modes(&original);
    device.resume(&[]).expect("resume distinct-speed TTY");
    pty.assert_modes(&raw);
    drop(device);
    pty.assert_modes(&original);
}

#[cfg(target_os = "linux")]
#[test]
fn secondary_tty_guard_restores_modes_after_later_setup_failure() {
    let pty = ModeRestorationPty::new();
    let original = pty.modes();
    let mut raw = original.clone();
    raw.make_raw();
    let failure = (|| -> std::io::Result<()> {
        let _device = pty.open_device();
        pty.assert_modes(&raw);
        // A later setup operation must unwind through the already-owned
        // TtyDevice rather than relying on the session becoming complete.
        Err(std::io::Error::other("controlled later setup failure"))
    })();
    assert!(failure.is_err());
    pty.assert_modes(&original);
}

#[cfg(target_os = "linux")]
#[test]
fn secondary_tty_restores_modes_even_when_leaving_screen_fails() {
    let pty = ModeRestorationPty::new();
    let original = pty.modes();
    let mut device = pty.open_device();
    // A retained read-only descriptor still permits terminal ioctls, while
    // screen-output writes fail. Exercise suspend's real output error path.
    device.file = std::fs::OpenOptions::new()
        .read(true)
        .open(&pty.path)
        .expect("open read-only PTY descriptor");
    assert!(
        device.suspend(&[]).is_err(),
        "screen leave must fail to write"
    );
    pty.assert_modes(&original);
    drop(device);
    pty.assert_modes(&original);
}

#[cfg(unix)]
#[test]
fn secondary_tty_registry_isolates_mode_strings_across_suspend_resume_remove_and_close_all() {
    use neovm_core::emacs_core::eval::QuitRequest;
    use neovm_core::emacs_core::terminal::pure::TtyFrameHostFactory as _;
    use std::fs::{File, OpenOptions};
    use std::io::Read as _;
    use std::os::fd::AsRawFd;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    struct DrainingPty {
        _slave: File,
        path: String,
        buffer: Arc<(Mutex<Vec<u8>>, Condvar)>,
        stop: Arc<AtomicBool>,
        reader: Option<std::thread::JoinHandle<()>>,
    }

    impl DrainingPty {
        fn open() -> Self {
            let mut master = File::from(
                rustix::pty::openpt(
                    rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY,
                )
                .expect("posix_openpt failed"),
            );
            rustix::pty::grantpt(&master).expect("grantpt failed");
            rustix::pty::unlockpt(&master).expect("unlockpt failed");
            let path = rustix::pty::ptsname(&master, Vec::new())
                .expect("ptsname failed")
                .to_string_lossy()
                .into_owned();
            let slave = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .expect("open slave");
            rustix::termios::tcsetwinsize(
                &slave,
                rustix::termios::Winsize {
                    ws_row: 24,
                    ws_col: 80,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                },
            )
            .expect("set PTY size");
            let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
            assert!(flags >= 0);
            assert_eq!(
                unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
                0
            );
            // Continuously drain master so `tcdrain(slave)` never blocks on macOS.
            let buffer = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let thread_buf = Arc::clone(&buffer);
            let thread_stop = Arc::clone(&stop);
            let reader = std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                while !thread_stop.load(Ordering::Acquire) {
                    let mut pfd = libc::pollfd {
                        fd: master.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let rc = unsafe { libc::poll(&mut pfd, 1, 20) };
                    if rc <= 0 || (pfd.revents & libc::POLLIN) == 0 {
                        continue;
                    }
                    loop {
                        match master.read(&mut chunk) {
                            Ok(0) => return,
                            Ok(n) => {
                                let (lock, cvar) = &*thread_buf;
                                lock.lock().unwrap().extend_from_slice(&chunk[..n]);
                                cvar.notify_all();
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(_) => return,
                        }
                    }
                }
            });
            Self {
                _slave: slave,
                path,
                buffer,
                stop,
                reader: Some(reader),
            }
        }

        fn drain(&self) -> Vec<u8> {
            let idle = Duration::from_millis(60);
            let (lock, cvar) = &*self.buffer;
            let mut guard = lock.lock().unwrap();
            loop {
                let before = guard.len();
                let (next, _) = cvar.wait_timeout(guard, idle).unwrap();
                guard = next;
                if guard.len() == before {
                    return std::mem::take(&mut *guard);
                }
            }
        }
    }

    impl Drop for DrainingPty {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    let pty_a = DrainingPty::open();
    let pty_b = DrainingPty::open();

    let registry = super::SecondaryTtyRegistry::default();
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let input = neovm_core::host_input::HostInputPublisher::install(&mut context);
    let mut factory = super::SecondaryTtyFactory::new(registry.clone(), input, QuitRequest::new());

    let req_a = neovm_core::emacs_core::terminal::pure::TtyFrameOpenRequest::new(
        11,
        neovm_core::window::FrameId(201),
        pty_a.path.clone(),
        "xterm-256color".to_string(),
    )
    .expect("request A");
    let req_b = neovm_core::emacs_core::terminal::pure::TtyFrameOpenRequest::new(
        12,
        neovm_core::window::FrameId(202),
        pty_b.path.clone(),
        "xterm-256color".to_string(),
    )
    .expect("request B");

    let mut host_a = factory.open_tty(req_a).expect("open A").host;
    let _host_b = factory.open_tty(req_b).expect("open B").host;
    let _ = pty_a.drain();
    let _ = pty_b.drain();

    // 1. Suspend A: only A receives <RESET-A> before te, B receives nothing.
    host_a.suspend_tty(b"<RESET-A>").expect("suspend A");
    let out_a = pty_a.drain();
    let out_b = pty_b.drain();
    let reset_pos = find_subslice(&out_a, b"<RESET-A>").expect("A reset marker");
    let te_pos = find_subslice(&out_a, b"\x1b[?1049l").expect("A te");
    assert!(
        reset_pos < te_pos,
        "reset strings must precede te: {out_a:?}"
    );
    assert!(
        out_b.is_empty(),
        "B must not receive A's suspend bytes: {out_b:?}"
    );

    // 2. Resume A: only A receives <SET-A> after ti/ks, B receives nothing.
    host_a.resume_tty(b"<SET-A>").expect("resume A");
    let out_a = pty_a.drain();
    let out_b = pty_b.drain();
    let ti_pos = find_subslice(&out_a, b"\x1b[?1049h").expect("A ti");
    let set_pos = find_subslice(&out_a, b"<SET-A>").expect("A set marker");
    assert!(ti_pos < set_pos, "set strings must follow ti: {out_a:?}");
    assert!(
        out_b.is_empty(),
        "B must not receive A's resume bytes: {out_b:?}"
    );

    // 3. Delete A (`remove`): only A receives <DEL-A> before te, B receives nothing.
    host_a.delete_terminal(b"<DEL-A>").expect("delete A");
    let out_a = pty_a.drain();
    let out_b = pty_b.drain();
    let del_pos = find_subslice(&out_a, b"<DEL-A>").expect("A delete marker");
    let te_pos = find_subslice(&out_a, b"\x1b[?1049l").expect("A delete te");
    assert!(
        del_pos < te_pos,
        "delete reset strings must precede te: {out_a:?}"
    );
    assert!(
        out_b.is_empty(),
        "B must not receive A's delete bytes: {out_b:?}"
    );

    // 4. Exit (`close_all_with`): remaining terminal B receives its own reset bytes.
    registry.close_all_with(|id| format!("<EXIT-{id}>").into_bytes());
    let out_a = pty_a.drain();
    let out_b = pty_b.drain();
    assert!(
        out_a.is_empty(),
        "deleted A must not receive exit bytes: {out_a:?}"
    );
    let exit_pos = find_subslice(&out_b, b"<EXIT-12>").expect("B exit marker");
    let te_pos = find_subslice(&out_b, b"\x1b[?1049l").expect("B exit te");
    assert!(
        exit_pos < te_pos,
        "exit reset strings must precede te: {out_b:?}"
    );
}
