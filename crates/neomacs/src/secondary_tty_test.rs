#[cfg(unix)]
#[test]
fn opened_secondary_tty_reads_bytes_for_its_own_frame_and_uses_device_size() {
    use std::ffi::CStr;
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    let master_fd = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
    assert!(master_fd >= 0, "posix_openpt failed");
    let mut master = unsafe { File::from_raw_fd(master_fd) };
    assert_eq!(unsafe { libc::grantpt(master_fd) }, 0, "grantpt failed");
    assert_eq!(unsafe { libc::unlockpt(master_fd) }, 0, "unlockpt failed");
    let mut slave_name = vec![0i8; 1024];
    assert_eq!(
        unsafe { libc::ptsname_r(master_fd, slave_name.as_mut_ptr(), slave_name.len()) },
        0,
        "ptsname_r failed"
    );
    let slave_name = unsafe { CStr::from_ptr(slave_name.as_ptr()) }
        .to_str()
        .expect("PTY path should be UTF-8")
        .to_owned();
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open slave");
    let size = libc::winsize {
        ws_row: 37,
        ws_col: 119,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    assert_eq!(
        unsafe { libc::ioctl(master_fd, libc::TIOCSWINSZ as _, &size) },
        0,
        "set PTY size"
    );
    let original_modes = terminal_modes(slave.as_raw_fd());
    drop(slave);

    let request = neovm_core::emacs_core::terminal::pure::TtyFrameOpenRequest::new(
        7,
        neovm_core::window::FrameId(123),
        slave_name.clone(),
        "xterm-256color".to_string(),
    )
    .expect("valid request");
    let (tx, rx) = crossbeam_channel::bounded(8);
    let (session, opened_size, _) =
        super::SecondaryTtySession::open(&request, tx, None, Arc::new(AtomicBool::new(false)))
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
    let restored_slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("reopen restored PTY slave");
    let restored_modes = terminal_modes(restored_slave.as_raw_fd());
    assert_eq!(restored_modes.c_iflag, original_modes.c_iflag);
    assert_eq!(restored_modes.c_oflag, original_modes.c_oflag);
    assert_eq!(restored_modes.c_cflag, original_modes.c_cflag);
    assert_eq!(restored_modes.c_lflag, original_modes.c_lflag);
    assert_eq!(restored_modes.c_cc, original_modes.c_cc);
}

#[cfg(unix)]
fn terminal_modes(fd: std::os::fd::RawFd) -> libc::termios {
    let mut modes = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(unsafe { libc::tcgetattr(fd, &mut modes) }, 0, "tcgetattr");
    modes
}
