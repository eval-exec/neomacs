//! Fatal-signal parity against the pinned GNU oracle (P4/P6).
//!
//! These run the real `neomacs` binary and GNU 31.1 on a pty whose child is
//! the session leader, so `tcgetpgrp (0) == getpgrp ()` holds and GNU's
//! fatal diagnostic is observable. The batch child visits a file, enables
//! auto-save-mode, modifies it, prints `probe-ready`, then sleeps; the test
//! sends the signal after readiness and records the raw wait status, stderr
//! and the auto-save/visited files.
//!
//! P4 scope (this file): SIGSEGV and SIGBUS. Rust std's stack-overflow
//! handler used to swallow an external `kill -SEGV`/`kill -BUS` (the process
//! stayed alive); the handler installed after std's must make the process die
//! by the signal with GNU's tty diagnostic and `Backtrace:` marker. The
//! orderly shutdown (emergency auto-save) for these signals lands with P3,
//! once threadsig's unified `shut_down_emacs` entry exists; until then these
//! tests assert the GNU death status and output markers, not `#file#`.
#![cfg(target_os = "linux")]

use std::os::fd::RawFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    neomacs_infra::crate_root!().join("../..")
}

fn neomacs_binary() -> PathBuf {
    let path = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/release/neomacs"));
    assert!(
        path.is_file(),
        "release-pdump-tests needs a release neomacs at {}; build it with \
         `cargo xtask fresh-build --release` or set NEOMACS_GUI_TEST_BINARY",
        path.display(),
    );
    path
}

/// The attested GNU reference: `GNU_EMACS_BINARY`, or the pinned oracle
/// resolved by the shared test helper.
fn gnu_binary() -> PathBuf {
    if let Some(binary) = std::env::var_os("GNU_EMACS_BINARY") {
        return PathBuf::from(binary);
    }
    crate::common::oracle_emacs_path()
}

#[derive(Debug)]
struct Captured {
    status: ExitStatus,
    output: String,
}

fn open_pty() -> (RawFd, RawFd) {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    size.ws_row = 24;
    size.ws_col = 80;
    // SAFETY: openpty writes the two descriptors; the termios pointer is null
    // as `openpty(3)` permits.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &mut size,
        )
    };
    assert_eq!(rc, 0, "openpty failed: {}", std::io::Error::last_os_error());
    // The master reads are polled, never blocking the test thread.
    // SAFETY: F_GETFL/F_SETFL on the owned master descriptor.
    unsafe {
        let flags = libc::fcntl(master, libc::F_GETFL);
        libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
    (master, slave)
}

/// Run `binary -Q --batch --eval FORM` with a pty as stdin/stdout/stderr and
/// as its controlling terminal; send `sig` once the child prints
/// `probe-ready`; return its raw status and complete output.
fn run_with_signal(binary: &Path, sig: libc::c_int) -> Captured {
    let directory = tempfile::tempdir().expect("create probe directory");
    let visited = directory.path().join("f.txt");
    std::fs::write(&visited, "base\n").expect("write visited file");
    let form = format!(
        "(progn (find-file {:?}) (auto-save-mode 1) (goto-char (point-max)) \
         (insert \"MODIFIED\") (message \"probe-ready\") (while t (sleep-for 1)))",
        visited.display().to_string(),
    );

    let (master, slave) = open_pty();
    let mut command = Command::new(binary);
    command
        .current_dir(directory.path())
        .env("RUST_LOG", "off")
        .args(["-Q", "--batch", "--eval", form.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: only async-signal-safe syscalls in the child before exec; the
    // slave fd is owned by this process until the parent closes it.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // Acquire the pty as the new session's controlling terminal; the
            // foreground process group becomes this session's, so GNU's
            // tcgetpgrp/getpgrp test is true.
            if libc::ioctl(slave, libc::TIOCSCTTY, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            for fd in [0, 1, 2] {
                if libc::dup2(slave, fd) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            if slave > 2 {
                libc::close(slave);
            }
            Ok(())
        });
    }
    let mut child = command.spawn().expect("spawn the editor under a pty");
    // The child has its own copy of the slave; the master must see EOF/EIO
    // when the child dies, so close the parent's slave.
    // SAFETY: the slave descriptor is owned by this function.
    unsafe { libc::close(slave) };

    let mut output = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut sent = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        // SAFETY: a nonblocking read of the owned master descriptor into a
        // live buffer.
        let read = unsafe { libc::read(master, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read > 0 {
            output.extend_from_slice(&buffer[..read as usize]);
            if !sent
                && output
                    .windows(b"probe-ready".len())
                    .any(|window| window == b"probe-ready")
            {
                sent = true;
                // SAFETY: kill(2) on the child pid with a valid signal.
                let rc = unsafe { libc::kill(child.id() as libc::pid_t, sig) };
                assert_eq!(
                    rc,
                    0,
                    "kill({sig}) failed: {}",
                    std::io::Error::last_os_error()
                );
            }
        }
        if let Some(status) = child.try_wait().expect("wait for the editor") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "the child never died after the signal; output so far:\n{}",
            String::from_utf8_lossy(&output),
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    // Drain what the pty still holds; Linux returns EIO once the slave side
    // is gone, which ends the loop.
    loop {
        // SAFETY: as above.
        let read = unsafe { libc::read(master, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read <= 0 {
            break;
        }
        output.extend_from_slice(&buffer[..read as usize]);
    }
    // SAFETY: the master descriptor is owned by this function.
    unsafe { libc::close(master) };
    assert!(sent, "the child never printed probe-ready");
    Captured {
        status,
        output: String::from_utf8_lossy(&output).into_owned(),
    }
}

fn fatal_error_line(output: &str) -> Option<&str> {
    // The pty's ONLCR turns the trailing newline into CRLF; compare the line
    // without the terminal's CR.
    output
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .find(|line| line.starts_with("Fatal error "))
}

/// SIGSEGV/SIGBUS must terminate the process BY THE SIGNAL with GNU's
/// diagnostic, in both engines. The old port stayed alive after either
/// signal because Rust std's stack-overflow handler treats a synthetic
/// delivery as a no-op and returns.
#[test]
fn external_segv_and_bus_kill_both_editors_the_same_way() {
    let neomacs = neomacs_binary();
    let gnu = gnu_binary();
    for (sig, description) in [
        (libc::SIGSEGV, "Fatal error 11: Segmentation fault"),
        (libc::SIGBUS, "Fatal error 7: Bus error"),
    ] {
        let reference = run_with_signal(&gnu, sig);
        let ours = run_with_signal(&neomacs, sig);

        for (engine, capture) in [("gnu", &reference), ("neomacs", &ours)] {
            assert_eq!(
                capture.status.signal(),
                Some(sig),
                "{engine}: expected death by signal {sig}, got {:?}\n{}",
                capture.status,
                capture.output,
            );
            assert_eq!(
                fatal_error_line(&capture.output),
                Some(description),
                "{engine}: missing GNU's tty diagnostic\n{}",
                capture.output,
            );
            assert!(
                capture.output.contains("Backtrace:"),
                "{engine}: missing the Backtrace: marker\n{}",
                capture.output,
            );
        }
        assert_eq!(
            fatal_error_line(&ours.output),
            fatal_error_line(&reference.output),
            "the Fatal error line must match the pinned oracle exactly",
        );
    }
}
