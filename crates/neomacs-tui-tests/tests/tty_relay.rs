#![cfg(unix)]
//! Exercise the real Unix PTY readers while Lisp is executing, without timing assertions.

use neomacs_tui_tests::{TuiProcessOutcome, TuiSession};
use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

const DEADLOCK_CAP: Duration = Duration::from_secs(30);
const READ_SLICE: Duration = Duration::from_millis(50);
const SETUP_READY: &[u8] = b"\x1b]777;tty-relay-setup\x07";
const BUSY_READY: &[u8] = b"\x1b]777;tty-relay-busy\x07";
const QUIT_ONCE: &[u8] = b"\x1b]777;tty-relay-quit=1\x07";
const RESPONSE: &[u8] = b"\x1b]777;tty-relay-response=1,quit=1\x07";
const AUX_READY: &[u8] = b"\x1b]777;tty-relay-aux-ready\x07";
const AUX_RESPONSE: &[u8] = b"\x1b]777;tty-relay-aux-target-and-live\x07";
const AUX_CLOSED: &[u8] = b"\x1b]777;tty-relay-aux-closed\x07";
const AUX_RESIZED: &[u8] = b"\x1b]777;tty-relay-aux-width=120\x07";
/// Drawn past the opening width; the Lisp below builds it with `concat` so
/// only the redrawn buffer text can put it on the auxiliary terminal.
const PAST_OLD_EDGE: &[u8] = b"PASTOLDEDGE";

// OSC markers are checked on the raw stream. Their escaped spellings in the
// minibuffer cannot satisfy the check by merely being echoed while typed.
const BUSY_SETUP: &str = r#"
  (setq tty-relay-primary-frame (selected-frame)
        tty-relay-primary-terminal (frame-terminal)
        tty-relay-quit-count 0
        tty-relay-response-count 0)
  (defun tty-relay-busy-command ()
    (interactive)
    (condition-case nil
        (let ((announce t))
          (while t
            (when announce
              (setq announce nil)
              (send-string-to-terminal "\e]777;tty-relay-busy\a"
                                       tty-relay-primary-terminal))))
      (quit
       (setq tty-relay-quit-count (1+ tty-relay-quit-count))
       (send-string-to-terminal
        (format "\e]777;tty-relay-quit=%d\a" tty-relay-quit-count)
        tty-relay-primary-terminal))))
  (defun tty-relay-response-command ()
    (interactive)
    (setq tty-relay-response-count (1+ tty-relay-response-count))
    (send-string-to-terminal
     (format "\e]777;tty-relay-response=%d,quit=%d\a"
             tty-relay-response-count tty-relay-quit-count)
     tty-relay-primary-terminal))
  (global-set-key (kbd "C-c b") #'tty-relay-busy-command)
  (global-set-key (kbd "C-c r") #'tty-relay-response-command)
"#;

fn occurrences(bytes: &[u8], marker: &[u8]) -> usize {
    bytes
        .windows(marker.len())
        .filter(|part| *part == marker)
        .count()
}

fn boot() -> TuiSession {
    let mut editor = TuiSession::neomacs("");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };
    editor.read_until(DEADLOCK_CAP, ready);
    assert!(ready(&editor.text_grid()), "TTY startup did not complete");
    editor
}

fn evaluate(editor: &mut TuiSession, form: &str) {
    editor.send_key("M-:");
    let ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    editor.read_until(DEADLOCK_CAP, ready);
    assert!(ready(&editor.text_grid()), "eval minibuffer did not open");
    editor.paste(form);
    editor.send_key("RET");
}

fn wait_primary_marker(
    editor: &mut TuiSession,
    marker: &[u8],
    mut auxiliary: Option<&mut AuxiliaryTty>,
) {
    let deadline = Instant::now() + DEADLOCK_CAP;
    while occurrences(editor.recent_output(), marker) == 0 {
        if let Some(tty) = auxiliary.as_deref_mut() {
            tty.drain_output();
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "missing raw TTY marker {marker:?}");
        editor.read(remaining.min(READ_SLICE));
    }
}

fn finish(mut editor: TuiSession) {
    editor.send_keys("C-x C-c");
    assert_eq!(
        editor.run_to_completion(DEADLOCK_CAP),
        TuiProcessOutcome::Exited
    );
}

#[test]
fn busy_lisp_quits_from_raw_cg() {
    let mut editor = boot();
    evaluate(
        &mut editor,
        &format!(
            "(progn {BUSY_SETUP} (send-string-to-terminal \"\\e]777;tty-relay-setup\\a\" tty-relay-primary-terminal))"
        ),
    );
    wait_primary_marker(&mut editor, SETUP_READY, None);
    editor.clear_recent_output();

    editor.send(b"\x03b");
    // This marker is emitted inside the endless Lisp loop, not before its call.
    wait_primary_marker(&mut editor, BUSY_READY, None);
    editor.send(b"\x07");
    wait_primary_marker(&mut editor, QUIT_ONCE, None);
    editor.send(b"\x03r");
    wait_primary_marker(&mut editor, RESPONSE, None);
    assert_eq!(occurrences(editor.recent_output(), BUSY_READY), 1);
    assert_eq!(occurrences(editor.recent_output(), QUIT_ONCE), 1);
    assert_eq!(occurrences(editor.recent_output(), RESPONSE), 1);
    finish(editor);
}

/// Test-local OS state: owns both PTY ends, carries no Lisp values, and is used
/// only on the fixture thread. Nonblocking mode affects its own master alone.
#[derive(Debug)]
struct AuxiliaryTty {
    master: File,
    _slave: OwnedFd,
    name: String,
    output: Vec<u8>,
}

impl AuxiliaryTty {
    fn new() -> Self {
        let mut master = -1;
        let mut slave = -1;
        let mut name = [0 as libc::c_char; 1024];
        let mut size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: writable output storage and a valid size; null termios uses
        // platform defaults. On success openpty returns two distinct owned fds.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    name.as_mut_ptr(),
                    std::ptr::null_mut(),
                    &mut size,
                )
            },
            0,
            "open auxiliary PTY: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: success above transfers ownership of both new descriptors.
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        // SAFETY: slave is the other distinct descriptor returned by openpty.
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        for fd in [&master, &slave] {
            // SAFETY: the descriptor remains owned and valid across each fcntl.
            let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
            assert!(flags >= 0, "get auxiliary descriptor flags");
            // SAFETY: preserve existing flags and prevent inheritance by editor exec.
            assert_eq!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC) },
                0,
                "set auxiliary close-on-exec"
            );
        }
        // SAFETY: this is the fixture's live master, not an editor-owned fd.
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0, "get auxiliary master flags");
        // SAFETY: preserve existing flags; only the fixture reads this master.
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0,
            "set auxiliary master nonblocking"
        );
        // SAFETY: successful openpty wrote a NUL-terminated name into the buffer.
        let name = unsafe { CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .expect("PTY name UTF-8")
            .to_owned();
        Self {
            master: File::from(master),
            _slave: slave,
            name,
            output: Vec::new(),
        }
    }

    fn drain_output(&mut self) {
        let mut bytes = [0; 8192];
        loop {
            match self.master.read(&mut bytes) {
                Ok(0) => return,
                Ok(count) => self.output.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
                Err(error) => panic!("read auxiliary PTY: {error}"),
            }
        }
    }

    /// Resize the terminal the way a terminal emulator does: on its master.
    fn resize(&self, columns: u16, rows: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: the master is a live owned PTY descriptor and size is valid.
        assert_eq!(
            unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) },
            0,
            "resize auxiliary PTY: {}",
            std::io::Error::last_os_error()
        );
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master
            .write_all(bytes)
            .expect("write auxiliary key bytes");
    }
}

#[test]
fn auxiliary_input_survives_busy_primary() {
    // Created first, dropped last: an assertion failure reaps the editor via
    // TuiSession before closing either test-owned auxiliary descriptor.
    let mut auxiliary = AuxiliaryTty::new();
    let mut editor = boot();
    evaluate(
        &mut editor,
        &format!(
            r#"(progn {BUSY_SETUP}
          (setq tty-relay-aux-frame
                (make-terminal-frame (list (cons 'tty {:?}) '(tty-type . "screen-256color"))))
          (setq tty-relay-aux-terminal (frame-terminal tty-relay-aux-frame))
          (defun tty-relay-aux-command ()
            (interactive)
            (unless (and (eq (selected-frame) tty-relay-aux-frame)
                         (frame-live-p tty-relay-primary-frame)
                         (frame-live-p tty-relay-aux-frame)
                         (terminal-live-p tty-relay-primary-terminal)
                         (terminal-live-p tty-relay-aux-terminal)
                         (= tty-relay-quit-count 1))
              (error "Auxiliary command target or terminal lifetime changed"))
            (send-string-to-terminal "\e]777;tty-relay-aux-target-and-live\a"
                                     tty-relay-primary-terminal))
          (defun tty-relay-close-aux-command ()
            (interactive)
            (select-frame tty-relay-primary-frame)
            (delete-frame tty-relay-aux-frame t)
            (unless (and (not (frame-live-p tty-relay-aux-frame))
                         (not (terminal-live-p tty-relay-aux-terminal))
                         (terminal-live-p tty-relay-primary-terminal))
              (error "Auxiliary terminal did not close independently"))
            (send-string-to-terminal "\e]777;tty-relay-aux-closed\a"
                                     tty-relay-primary-terminal))
          (global-set-key (kbd "C-c a") #'tty-relay-aux-command)
          (global-set-key (kbd "C-c d") #'tty-relay-close-aux-command)
          (send-string-to-terminal "\e]777;tty-relay-aux-ready\a" tty-relay-aux-terminal)
          (select-frame tty-relay-primary-frame)
          (send-string-to-terminal "\e]777;tty-relay-setup\a" tty-relay-primary-terminal))"#,
            auxiliary.name
        ),
    );
    wait_primary_marker(&mut editor, SETUP_READY, Some(&mut auxiliary));
    let deadline = Instant::now() + DEADLOCK_CAP;
    while occurrences(&auxiliary.output, AUX_READY) == 0 {
        auxiliary.drain_output();
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "auxiliary terminal did not become ready"
        );
        editor.read(remaining.min(READ_SLICE));
    }
    editor.clear_recent_output();
    editor.send(b"\x03b");
    wait_primary_marker(&mut editor, BUSY_READY, Some(&mut auxiliary));
    auxiliary.send(b"\x07");
    wait_primary_marker(&mut editor, QUIT_ONCE, Some(&mut auxiliary));
    auxiliary.send(b"\x03a");
    wait_primary_marker(&mut editor, AUX_RESPONSE, Some(&mut auxiliary));
    assert_eq!(occurrences(editor.recent_output(), QUIT_ONCE), 1);
    assert_eq!(occurrences(editor.recent_output(), AUX_RESPONSE), 1);

    editor.send(b"\x03d");
    wait_primary_marker(&mut editor, AUX_CLOSED, Some(&mut auxiliary));
    finish(editor);
}

#[test]
fn auxiliary_frame_draws_past_its_opening_width_after_resize() {
    let mut auxiliary = AuxiliaryTty::new();
    let mut editor = boot();
    evaluate(
        &mut editor,
        &format!(
            r#"(progn {BUSY_SETUP}
          (setq tty-relay-aux-frame
                (make-terminal-frame (list (cons 'tty {:?}) '(tty-type . "screen-256color"))))
          (defun tty-relay-aux-wide-line-command ()
            (interactive)
            (switch-to-buffer (get-buffer-create "tty-relay-wide"))
            (erase-buffer)
            (insert (make-string 100 ?-) (concat "PASTOLD" "EDGE"))
            (send-string-to-terminal
             (format "\e]777;tty-relay-aux-width=%d\a" (frame-width))
             tty-relay-primary-terminal))
          (global-set-key (kbd "C-c w") #'tty-relay-aux-wide-line-command)
          (send-string-to-terminal "\e]777;tty-relay-aux-ready\a" (frame-terminal tty-relay-aux-frame))
          (select-frame tty-relay-primary-frame)
          (send-string-to-terminal "\e]777;tty-relay-setup\a" tty-relay-primary-terminal))"#,
            auxiliary.name
        ),
    );
    wait_primary_marker(&mut editor, SETUP_READY, Some(&mut auxiliary));
    let deadline = Instant::now() + DEADLOCK_CAP;
    while occurrences(&auxiliary.output, AUX_READY) == 0 {
        auxiliary.drain_output();
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "auxiliary terminal did not become ready"
        );
        editor.read(remaining.min(READ_SLICE));
    }

    // Opened at 80x24. The input reader observes the new size before it reads
    // the key bytes, so the command runs on a frame that is already 120 wide.
    auxiliary.resize(120, 30);
    auxiliary.output.clear();
    auxiliary.send(b"\x03w");
    wait_primary_marker(&mut editor, AUX_RESIZED, Some(&mut auxiliary));
    // A grid left at 80 columns clips the marker at column 100 on every redraw.
    let deadline = Instant::now() + DEADLOCK_CAP;
    while occurrences(&auxiliary.output, PAST_OLD_EDGE) == 0 {
        auxiliary.drain_output();
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "auxiliary frame never drew past its opening width"
        );
        editor.read(remaining.min(READ_SLICE));
    }
    finish(editor);
}
