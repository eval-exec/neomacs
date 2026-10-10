use super::*;
use std::io::{self, Write};
use std::sync::mpsc;
use std::time::Duration;

/// Exercise the installed subscriber and guard against a genuinely unread PTY,
/// in fresh processes so the global subscriber cannot contaminate other tests.
#[cfg(target_os = "linux")]
#[test]
fn gui_logging_returns_with_unread_pty_and_optional_file() {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::process::{Command, Stdio};
    use std::time::Instant;

    const CHILD: &str = "NEOMACS_TEST_GUI_LOG_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // The parent passes the saturated PTY slave as stdin. Redirect only
        // after libtest's startup output, and exit before its final stdout.
        // SAFETY: 0 and 1 are live inherited descriptors in this owned child.
        assert_eq!(unsafe { libc::dup2(0, 1) }, 1);
        let guard = init(LogTarget::GuiStdout);
        let record = "x".repeat(8192);
        for _ in 0..4096 {
            tracing::info!("{record}");
        }
        assert!(guard.gui_rejected_records() > 0);
        drop(guard);
        eprintln!("GUI-LOG-PRODUCER-AND-GUARD-RETURNED");
        std::process::exit(0);
    }

    for file_output in [false, true] {
        let (mut master, mut slave) = (-1, -1);
        // SAFETY: valid output pointers; null optional name/termios/winsize.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        // SAFETY: openpty returned two fresh descriptors owned by this test.
        let (_master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        use std::os::fd::AsRawFd;
        let fd = slave.as_raw_fd();
        // SAFETY: fd remains owned and live for every operation below.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        let bytes = [b'x'; 4096];
        let mut saturated = false;
        for _ in 0..4096 {
            // SAFETY: bytes covers the supplied length; fd is the PTY slave.
            let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
            if n < 0 {
                assert_eq!(
                    io::Error::last_os_error().raw_os_error(),
                    Some(libc::EAGAIN)
                );
                saturated = true;
                break;
            }
        }
        assert!(saturated, "fixture did not saturate its unread PTY");
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFL, flags) }, 0);
        let dir = tempfile::tempdir().unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "logging::tests::gui_logging_returns_with_unread_pty_and_optional_file",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("RUST_LOG", "info")
            .env_remove("NEOMACS_LOG_TO_FILE")
            .env_remove("NEOMACS_LOG_FILE")
            .stdin(Stdio::from(slave))
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if file_output {
            command.env("NEOMACS_LOG_FILE", dir.path().join("diagnostics.log"));
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("GUI producer/guard blocked with optional file={file_output}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        use std::io::Read;
        let mut stderr = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(status.success(), "{stderr}");
        assert!(stderr.contains("GUI-LOG-PRODUCER-AND-GUARD-RETURNED"));
    }
}

#[test]
fn gui_file_guard_does_not_wait_for_full_blocked_appender() {
    struct BlockedFile {
        entered: Option<mpsc::Sender<()>>,
        release: mpsc::Receiver<()>,
        done: mpsc::Sender<()>,
    }
    impl Write for BlockedFile {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Drop for BlockedFile {
        fn drop(&mut self) {
            let _ = self.done.send(());
        }
    }
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    let (mut writer, worker) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .buffered_lines_limit(1)
        .lossy(true)
        .finish(BlockedFile {
            entered: Some(entered_tx),
            release: release_rx,
            done: done_tx,
        });
    let guard = FileGuard::new(worker, LogTarget::GuiStdout).unwrap();
    writer.write_all(b"blocked\n").unwrap();
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    writer.write_all(b"queued\n").unwrap();
    let (dropped_tx, dropped) = mpsc::channel();
    let owner = std::thread::spawn(move || {
        drop(guard);
        dropped_tx.send(()).unwrap();
    });
    let returned_while_blocked = dropped.recv_timeout(Duration::from_millis(100)).is_ok();
    release.send(()).unwrap();
    owner.join().unwrap();
    drop(writer);
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        returned_while_blocked,
        "GUI file guard blocked on full file appender"
    );
}
