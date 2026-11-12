//! A lost secondary TTY deletes its terminal through GNU's internal noelisp
//! path, leaving the primary terminal alive. GNU queues the deletion hooks
//! in keyboard.c:8129, terminal.c:405 and frame.c:2710/3101, then drains the
//! LIFO queue in keyboard.c:4763 after the objects have become dead.
//! Run the paired real editors inside the test job's PID/memory sandbox.
#![cfg(all(target_os = "linux", feature = "release-pdump-tests"))]

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
enum SecondaryFrames {
    One,
    Two,
}

impl SecondaryFrames {
    // GNU a360712c9d frame.c:1438,1560 prepends each new frame to
    // Vframe_list. frame.h:1404-1407 walks from its head, so terminal.c:
    // 345-352 visits frame2 before frame1, pushing
    // each before/after hook (frame.c:2710-2713,3101-3104). The LIFO drain
    // in keyboard.c:4763-4767 therefore runs frame1's after/before first,
    // then frame2's after/before, then the terminal hook. These labels are
    // creation order, as confirmed by the attested two-frame transcript.
    fn expected_deletions(self) -> &'static str {
        match self {
            Self::One => "after nil\nbefore nil\nterminal nil\n",
            Self::Two => {
                "frame1 after nil\nframe1 before nil\nframe2 after nil\nframe2 before nil\nterminal nil\n"
            }
        }
    }
}

fn neomacs_binary() -> PathBuf {
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| neomacs_infra::crate_root!().join("../../target/release/neomacs"));
    assert!(
        binary.is_file(),
        "release-pdump-tests needs a release binary at {}; run cargo xtask fresh-build --release or set NEOMACS_GUI_TEST_BINARY",
        binary.display()
    );
    binary.canonicalize().expect("resolve the release binary")
}

struct Pty {
    master: Option<OwnedFd>,
    slave: Option<OwnedFd>,
    device: PathBuf,
    output: Vec<u8>,
}

impl Pty {
    fn open() -> Self {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty initializes both descriptors and reads the live size.
        let rc = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        assert_eq!(rc, 0, "openpty: {}", std::io::Error::last_os_error());
        // SAFETY: openpty returned two distinct owned descriptors on success.
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        for fd in [&master, &slave] {
            // The child must not inherit either master: keeping the secondary
            // master there would prevent the controller's close from hanging up.
            // SAFETY: fcntl operates on these live owned descriptors.
            let rc = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
            assert_eq!(rc, 0, "set CLOEXEC: {}", std::io::Error::last_os_error());
        }
        // SAFETY: fcntl reads/updates the owned master's flags.
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0, "read master flags");
        let rc =
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) };
        assert_eq!(rc, 0, "make master nonblocking");
        let device = std::fs::read_link(format!("/proc/self/fd/{}", slave.as_raw_fd()))
            .expect("resolve PTY device");
        Self {
            master: Some(master),
            slave: Some(slave),
            device,
            output: Vec::new(),
        }
    }

    fn drain(&mut self) {
        let Some(master) = &self.master else { return };
        let mut bytes = [0u8; 16384];
        for _ in 0..64 {
            // SAFETY: nonblocking read from a live descriptor into this buffer.
            let count =
                unsafe { libc::read(master.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
            if count > 0 {
                self.output.extend_from_slice(&bytes[..count as usize]);
            } else if count == 0 {
                return;
            } else {
                match std::io::Error::last_os_error().raw_os_error() {
                    Some(libc::EINTR) => continue,
                    Some(libc::EAGAIN) | Some(libc::EIO) => return,
                    _ => panic!("PTY read: {}", std::io::Error::last_os_error()),
                }
            }
        }
    }
}

/// Every editor is a new session/process-group leader; cleanup targets only
/// that owned group and reaps the child before its PTYs and directory drop.
struct EditorChild(Child);

impl EditorChild {
    fn terminate(&mut self) {
        // SAFETY: successful spawn completed setsid before exec, so this is
        // the process group owned by this controller, never the test runner's.
        unsafe { libc::kill(-(self.0.id() as libc::pid_t), libc::SIGKILL) };
    }
}

impl Drop for EditorChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            self.terminate();
        }
        let _ = self.0.wait();
    }
}

fn form_for(frames: SecondaryFrames, device: &Path) -> String {
    let (two, callbacks) = match frames {
        SecondaryFrames::One => ("nil", 3),
        SecondaryFrames::Two => ("t", 5),
    };
    let device = format!("{:?}", device.to_str().expect("UTF-8 PTY name"));
    format!(
        r#"(progn
  (setq auto-save-list-file-name nil
        probe-root (getenv "NEOMACS_TTY_HANGUP_DIR")
        probe-primary (selected-frame)
        probe-primary-terminal (frame-terminal probe-primary)
        probe-secondary nil probe-secondary-2 nil probe-two {two})
  (defun probe-write (name content)
    (write-region content nil (expand-file-name name probe-root) t 'silent))
  (defun probe-frame-log (phase frame)
    (probe-write "delete.log"
      (format "%s%s %S\n"
        (if probe-two (if (eq frame probe-secondary) "frame1 " "frame2 ") "")
        phase (frame-live-p frame))))
  (setq kill-emacs-hook
        (list (lambda () (probe-write "kill.log" "KILL\n")))
        delete-terminal-functions
        (list (lambda (terminal)
                (probe-write "delete.log"
                  (format "terminal %S\n" (terminal-live-p terminal)))))
        delete-frame-functions
        (list (lambda (frame) (probe-frame-log "before" frame)))
        after-delete-frame-functions
        (list (lambda (frame) (probe-frame-log "after" frame))))
  (setq probe-secondary
        (make-terminal-frame (list (cons 'tty {device}) (cons 'tty-type "xterm"))))
  (setq probe-secondary-terminal (frame-terminal probe-secondary))
  (when probe-two
    (setq probe-secondary-2
          (make-terminal-frame (list (cons 'tty {device}) (cons 'tty-type "xterm"))))
    (unless (eq probe-secondary-terminal (frame-terminal probe-secondary-2))
      (error "The secondary frames must share one terminal")))
  (run-with-timer 0.1 0.1
    (lambda ()
      (let ((file (expand-file-name "delete.log" probe-root)))
        (when (and (file-exists-p file)
                   (with-temp-buffer (insert-file-contents file)
                     (>= (count-lines (point-min) (point-max)) {callbacks})))
          (probe-write "alive.log"
            (format "ALIVE primary-frame=%S primary-terminal=%S kill-log-absent=%S secondary-frame=%S secondary-frame2=%S secondary-terminal=%S\n"
              (frame-live-p probe-primary)
              (terminal-live-p probe-primary-terminal)
              (not (file-exists-p (expand-file-name "kill.log" probe-root)))
              (frame-live-p probe-secondary)
              (if probe-two (frame-live-p probe-secondary-2) nil)
              (terminal-live-p probe-secondary-terminal)))
          (setq kill-emacs-hook nil)
          (kill-emacs 0)))))
  (probe-write "ready.log" "READY\n"))"#
    )
}

#[derive(Debug)]
struct Captured {
    form: String,
    status: ExitStatus,
    ready: bool,
    timed_out: bool,
    primary_output: String,
    secondary_output: String,
    delete_log: Option<String>,
    alive_log: Option<String>,
    kill_log: Option<String>,
}

impl Captured {
    fn print(&self, engine: &str) {
        println!(
            "{engine} FORM:\n{}\n{engine} PRIMARY PTY:\n{}\n{engine} SECONDARY PTY:\n{}\n{engine} RESULT: status={:?} ready={} timeout={}\nDELETE={:?}\nALIVE={:?}\nKILL={:?}",
            self.form,
            self.primary_output,
            self.secondary_output,
            self.status,
            self.ready,
            self.timed_out,
            self.delete_log,
            self.alive_log,
            self.kill_log
        );
    }
}

fn artifact(directory: &Path, name: &str) -> Option<String> {
    match std::fs::read_to_string(directory.join(name)) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("read {name}: {error}"),
    }
}

fn run_editor(engine: &str, binary: &Path, frames: SecondaryFrames) -> Captured {
    let directory = tempfile::tempdir().expect("create TTY probe directory");
    let home = directory.path().join("home");
    std::fs::create_dir(&home).expect("create isolated HOME");
    let mut primary = Pty::open();
    let mut secondary = Pty::open();
    let form = form_for(frames, &secondary.device);
    println!("FORM {engine}:\n{form}");
    let primary_slave = primary.slave.as_ref().expect("primary slave").as_raw_fd();
    let mut command = Command::new(binary);
    command
        .current_dir(directory.path())
        .env_remove("RUST_LOG")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("TERM", "xterm")
        .env("HOME", home)
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .env("EMACSNATIVELOADPATH", "/dev/null")
        .env("NEOMACS_TTY_HANGUP_DIR", directory.path())
        .args(["-nw", "-Q", "--eval", &form])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: only async-signal-safe syscalls execute between fork and exec.
    // CLOEXEC closes both masters and the original slave after stdio dup2.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() == -1 || libc::ioctl(primary_slave, libc::TIOCSCTTY, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            for fd in [0, 1, 2] {
                if libc::dup2(primary_slave, fd) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = EditorChild(command.spawn().expect("spawn editor with controlling PTY"));
    drop(primary.slave.take());
    // Keep the secondary slave until READY: an unopened PTY master can
    // otherwise return EIO before the editor has opened the second terminal.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut ready = false;
    let mut timed_out = false;
    let status = loop {
        primary.drain();
        secondary.drain();
        if !ready && artifact(directory.path(), "ready.log").as_deref() == Some("READY\n") {
            ready = true;
            drop(secondary.slave.take());
            // Hang up only the secondary terminal; the primary master stays
            // owned and drained until the editor has exited normally.
            drop(secondary.master.take());
        }
        if let Some(status) = child.0.try_wait().expect("poll editor") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child.terminate();
            break child.0.wait().expect("reap stalled editor");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    primary.drain();
    secondary.drain();
    Captured {
        form,
        status,
        ready,
        timed_out,
        primary_output: String::from_utf8_lossy(&primary.output).into_owned(),
        secondary_output: String::from_utf8_lossy(&secondary.output).into_owned(),
        delete_log: artifact(directory.path(), "delete.log"),
        alive_log: artifact(directory.path(), "alive.log"),
        kill_log: artifact(directory.path(), "kill.log"),
    }
}

fn assert_hangup_matches_gnu(frames: SecondaryFrames) {
    let gnu = crate::common::oracle_emacs_path();
    let neomacs = neomacs_binary();
    let reference = run_editor("GNU", &gnu, frames);
    let ours = run_editor("Neomacs", &neomacs, frames);
    // Print both complete forms, PTY outputs and hook artifacts before any
    // behavioral assertion, so a RED run preserves the attested comparison.
    reference.print("GNU");
    ours.print("Neomacs");
    for (engine, capture) in [("GNU", &reference), ("Neomacs", &ours)] {
        assert!(!capture.timed_out, "{engine} timed out: {capture:#?}");
        assert!(capture.ready, "{engine} did not reach READY: {capture:#?}");
        assert_eq!(capture.status.code(), Some(0), "{engine}: {capture:#?}");
        assert_eq!(
            capture.kill_log, None,
            "{engine} ran kill-emacs-hook: {capture:#?}"
        );
        assert_eq!(
            capture.alive_log.as_deref(),
            Some(
                "ALIVE primary-frame=t primary-terminal=t kill-log-absent=t secondary-frame=nil secondary-frame2=nil secondary-terminal=nil\n"
            ),
            "{engine}: {capture:#?}"
        );
        assert_eq!(
            capture.delete_log.as_deref(),
            Some(frames.expected_deletions()),
            "{engine}: {capture:#?}"
        );
    }
    assert_eq!(ours.delete_log, reference.delete_log);
}

#[test]
fn lost_secondary_tty_defers_hooks_until_frame_and_terminal_are_dead() {
    assert_hangup_matches_gnu(SecondaryFrames::One);
}

#[test]
fn lost_secondary_tty_deletes_both_frames_with_gnu_hook_order() {
    assert_hangup_matches_gnu(SecondaryFrames::Two);
}
