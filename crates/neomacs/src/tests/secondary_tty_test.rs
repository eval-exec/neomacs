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
    let (tx, rx) = crossbeam_channel::bounded(8);
    let (session, opened_size, _) =
        super::SecondaryTtySession::open(&request, tx, None, QuitRequest::new())
            .expect("open secondary TTY");

    assert_eq!(opened_size.columns(), 119);
    assert_eq!(opened_size.rows(), 37);
    master.write_all(b"z").expect("write PTY input");
    let event = rx
        .recv_timeout(Duration::from_secs(2))
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
    let (tx, _rx) = crossbeam_channel::bounded(8);
    let (mut session, _, _) =
        super::SecondaryTtySession::open(&request, tx, None, QuitRequest::new()).unwrap();
    drop(slave);
    for _ in 0..2 {
        super::super::tty_output::paint_to(
            &mut session.rif,
            &mut session.device.file,
            &session.device.capabilities,
        )
        .expect("padded secondary render must use the attached device snapshot");
        session.suspend().unwrap();
        session.resume().unwrap();
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
        device.suspend().expect("suspend implicit-baud TTY");
        pty.assert_modes(&original);
        device.suspend().expect("suspend already suspended TTY");
        pty.assert_modes(&original);
        device.resume().expect("resume implicit-baud TTY");
        pty.assert_modes(&raw);
        device.resume().expect("resume already active TTY");
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
    device.suspend().expect("suspend distinct-speed TTY");
    pty.assert_modes(&original);
    device.resume().expect("resume distinct-speed TTY");
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
    assert!(device.suspend().is_err(), "screen leave must fail to write");
    pty.assert_modes(&original);
    drop(device);
    pty.assert_modes(&original);
}
