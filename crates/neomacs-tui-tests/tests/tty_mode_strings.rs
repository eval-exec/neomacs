#![cfg(unix)]
//! Issue #835: `tty-mode-reset-strings` and `tty-mode-set-strings` terminal
//! parameters must be emitted on TTY exit, suspend, and resume in GNU's byte
//! order.
//!
//! GNU Emacs 31.1 (`src/term.c:169-247`):
//! - `tty_reset_terminal_modes` emits `tty-mode-reset-strings` FIRST (in list
//!   order, skipping non-strings), before `ke`, `ve`, `te`.
//! - `tty_set_terminal_modes` emits `tty-mode-set-strings` LAST (in list
//!   order, skipping non-strings), after `ti` and `ks`.
//! - Both run via `reset_sys_modes`/`init_sys_modes` from exit (`C-x C-c`),
//!   `suspend-emacs` (`C-z`), `suspend-tty`, `resume-tty`, and `delete-terminal`.

use crate::support::{eval_expression_one, settle_session};
use neomacs_tui_tests::{
    QUIET_MX_SUGGESTION_EVAL, QUIET_NATIVE_COMP_EVAL, TuiLaunch, TuiProcessOutcome, TuiSession,
    TuiTerminalConfig,
};
use std::ffi::CStr;
use std::fs::File;
use std::io::Read as _;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const TERM_TYPE: &str = "xterm-256color";
const MOUSE_ENABLE: &[u8] = b"\x1b[?1000h\x1b[?1003h\x1b[?1006h";
const MOUSE_DISABLE: &[u8] = b"\x1b[?1006l\x1b[?1003l\x1b[?1000l";
const BRACKETED_PASTE_ENABLE: &[u8] = b"\x1b[?2004h";
const BRACKETED_PASTE_DISABLE: &[u8] = b"\x1b[?2004l";
const TERM_TI: &[u8] = b"\x1b[?1049h";
const TERM_TE: &[u8] = b"\x1b[?1049l";
const TERM_KS: &[u8] = b"\x1b[?1h\x1b=";
const TERM_KE: &[u8] = b"\x1b[?1l\x1b>";
const TERM_VE: &[u8] = b"\x1b[?12l\x1b[?25h";

const SETUP_EL: &str = r#"(progn
  (xterm-mouse-mode 1)
  (push "<TTY-MODE-RESET-1>" (terminal-parameter nil 'tty-mode-reset-strings))
  (push 42 (terminal-parameter nil 'tty-mode-reset-strings))
  (push "<TTY-MODE-RESET-2>" (terminal-parameter nil 'tty-mode-reset-strings))
  (push "<TTY-MODE-SET-1>" (terminal-parameter nil 'tty-mode-set-strings))
  (push 99 (terminal-parameter nil 'tty-mode-set-strings))
  (push "<TTY-MODE-SET-2>" (terminal-parameter nil 'tty-mode-set-strings)))"#;

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_subslice_from(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    find_subslice(&haystack[start..], needle).map(|offset| start + offset)
}

fn count_subslice(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn resolve_true_program() -> PathBuf {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("true");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    for candidate in ["/usr/bin/true", "/bin/true"] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return path;
        }
    }
    panic!("cannot locate `true` binary on PATH, /usr/bin/true, or /bin/true");
}

struct SecondaryPty {
    _slave: File,
    path: String,
    buffer: Arc<(Mutex<Vec<u8>>, Condvar)>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl SecondaryPty {
    fn open() -> Self {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        let mut name = [0 as libc::c_char; 256];
        let mut size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: initialized output slots, fixed name buffer, and winsize.
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
            "open secondary test PTY: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: openpty returned two live owned descriptors and a NUL-terminated path.
        let mut master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let path = unsafe { CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .expect("UTF-8 PTY slave path")
            .to_owned();
        // SAFETY: master is a live descriptor; set nonblocking for the drain thread.
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        // Drain the master asynchronously so `tcdrain(slave)` during
        // `suspend-tty` / `delete-terminal` never blocks on macOS (XNU waits
        // for the master to consume queued output before `tcdrain` returns).
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
                // SAFETY: single initialized pollfd for owned master fd.
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

    fn drain(&mut self, idle_timeout_ms: i32) -> Vec<u8> {
        let idle = Duration::from_millis(idle_timeout_ms.max(1) as u64);
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

    fn wait_for(&mut self, needle: &[u8], timeout: Duration) -> Vec<u8> {
        let deadline = Instant::now() + timeout;
        let (lock, cvar) = &*self.buffer;
        let mut guard = lock.lock().unwrap();
        while find_subslice(&guard, needle).is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, _) = cvar.wait_timeout(guard, remaining).unwrap();
            guard = next;
        }
        drop(guard);
        self.drain(150)
    }
}

impl Drop for SecondaryPty {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn spawn_editor_with(
    program: &Path,
    label: &str,
    is_gnu: bool,
    eval_expr: Option<&str>,
    extra_env: &[(&str, &Path)],
) -> TuiSession {
    let mut launch = TuiLaunch::new(program.as_os_str())
        .with_uncertified_startup()
        .arg("-nw")
        .arg("-Q");
    if is_gnu {
        launch = launch
            .arg("-no-comp-spawn")
            .arg(QUIET_NATIVE_COMP_EVAL)
            .arg(QUIET_MX_SUGGESTION_EVAL);
    } else {
        launch = launch.arg(QUIET_MX_SUGGESTION_EVAL);
    }
    if let Some(expr) = eval_expr {
        launch = launch.arg("--eval").arg(expr);
    }
    for (key, value) in extra_env {
        launch = launch.env(key, value.as_os_str());
    }
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        label,
        TuiTerminalConfig::new(TERM_TYPE, 24, 80),
    );
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };
    session.read_until(Duration::from_secs(20), ready);
    assert!(
        ready(&session.text_grid()),
        "{label}: editor did not reach *scratch*:\n{}",
        session.text_grid().join("\n")
    );
    settle_session(&mut session);
    session
}

fn spawn_editor(program: &Path, label: &str, is_gnu: bool) -> TuiSession {
    spawn_editor_with(program, label, is_gnu, Some(SETUP_EL), &[])
}

fn assert_reset_order(output: &[u8], label: &str, phase: &str) -> usize {
    let reset2 = find_subslice(output, b"<TTY-MODE-RESET-2>").unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing <TTY-MODE-RESET-2> in PTY output: {:?}",
            String::from_utf8_lossy(output)
        )
    });
    let reset1 = find_subslice_from(output, reset2 + 1, b"<TTY-MODE-RESET-1>").unwrap_or_else(
        || {
            panic!(
                "{label} ({phase}): missing <TTY-MODE-RESET-1> after <TTY-MODE-RESET-2> in PTY output: {:?}",
                String::from_utf8_lossy(output)
            )
        },
    );
    let mouse_disable = find_subslice_from(output, reset1 + 1, MOUSE_DISABLE).unwrap_or_else(
        || {
            panic!(
                "{label} ({phase}): missing xterm mouse disable sequence after reset markers in PTY output: {:?}",
                String::from_utf8_lossy(output)
            )
        },
    );
    let ke = find_subslice_from(output, mouse_disable + MOUSE_DISABLE.len(), TERM_KE)
        .unwrap_or_else(|| {
            panic!(
                "{label} ({phase}): missing ke after tty-mode-reset-strings in PTY output: {:?}",
                String::from_utf8_lossy(output)
            )
        });
    let ve = find_subslice_from(output, ke + TERM_KE.len(), TERM_VE).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing ve after ke in PTY output: {:?}",
            String::from_utf8_lossy(output)
        )
    });
    let te = find_subslice_from(output, ve + TERM_VE.len(), TERM_TE).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing te after ve in PTY output: {:?}",
            String::from_utf8_lossy(output)
        )
    });
    let reset_end = te + TERM_TE.len();
    // `lisp/term/xterm.el` manages bracketed paste via `tty-mode-reset-strings`;
    // Rust must not emit a duplicate `\e[?2004l`.
    assert_eq!(
        count_subslice(&output[reset2..reset_end], BRACKETED_PASTE_DISABLE),
        1,
        "{label} ({phase}): expected exactly one bracketed-paste disable sequence in reset burst: {:?}",
        String::from_utf8_lossy(&output[reset2..reset_end])
    );
    reset_end
}

fn assert_set_order(output: &[u8], after: usize, label: &str, phase: &str) {
    let ti = find_subslice_from(output, after, TERM_TI).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing ti after te in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
    let ks = find_subslice_from(output, ti + TERM_TI.len(), TERM_KS).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing ks after ti in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
    let set2 =
        find_subslice_from(output, ks + TERM_KS.len(), b"<TTY-MODE-SET-2>").unwrap_or_else(|| {
            panic!(
                "{label} ({phase}): missing <TTY-MODE-SET-2> after ks in PTY output: {:?}",
                String::from_utf8_lossy(&output[after..])
            )
        });
    let set1 = find_subslice_from(output, set2 + 1, b"<TTY-MODE-SET-1>").unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing <TTY-MODE-SET-1> after <TTY-MODE-SET-2> in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
    let mouse_enable = find_subslice_from(output, set1 + 1, MOUSE_ENABLE).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing xterm mouse enable sequence after set markers in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
    // `lisp/term/xterm.el` manages bracketed paste via `tty-mode-set-strings`;
    // Rust must not emit a duplicate `\e[?2004h`.
    assert_eq!(
        count_subslice(&output[ti..], BRACKETED_PASTE_ENABLE),
        1,
        "{label} ({phase}): expected exactly one bracketed-paste enable sequence in resume burst: {:?}",
        String::from_utf8_lossy(&output[ti..mouse_enable + MOUSE_ENABLE.len()])
    );
}

fn wait_for_bytes(session: &mut TuiSession, needle: &[u8], timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if find_subslice(session.recent_output(), needle).is_some() {
            // Drain any trailing bytes from the same burst.
            session.read(Duration::from_millis(150));
            return;
        }
        session.read(Duration::from_millis(80));
    }
}

fn wait_for_file(path: &Path, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(content) = std::fs::read_to_string(path)
            && !content.is_empty()
        {
            return content;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    panic!("timed out waiting for result file {}", path.display());
}

fn decode_hex(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    assert_eq!(hex.len() % 2, 0, "odd-length hex output: {hex:?}");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex byte"))
        .collect()
}

fn run_real_sigtstp_stop_and_sigcont(program: &Path, label: &str, is_gnu: bool) {
    // Spawn a session-leader wrapper that places the editor in a non-orphaned
    // foreground process group (`setpgid` + `tcsetpgrp`) and waits with
    // `WUNTRACED`. Unlike an orphaned `setsid` PTY child, Linux and macOS
    // genuinely deliver `SIGTSTP` and stop the editor until `SIGCONT`.
    const DRIVER: &str = r#"
import fcntl, os, pty, select, signal, struct, sys, termios, time

argv = sys.argv[1:]
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
ctrl_r, ctrl_w = os.pipe()
res_r, res_w = os.pipe()

shell_pid = os.fork()
if shell_pid == 0:
    os.close(master)
    os.close(ctrl_w)
    os.close(res_r)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGTTOU, signal.SIG_IGN)
    signal.signal(signal.SIGTTIN, signal.SIG_IGN)
    signal.signal(signal.SIGTSTP, signal.SIG_IGN)
    editor_pid = os.fork()
    if editor_pid == 0:
        os.close(ctrl_r)
        os.close(res_w)
        os.setpgid(0, 0)
        os.dup2(slave, 0)
        os.dup2(slave, 1)
        os.dup2(slave, 2)
        if slave > 2:
            os.close(slave)
        for sig in (signal.SIGTSTP, signal.SIGTTIN, signal.SIGTTOU):
            signal.signal(sig, signal.SIG_DFL)
        os.execv(argv[0], argv)
        os._exit(127)
    try:
        os.setpgid(editor_pid, editor_pid)
    except PermissionError:
        pass
    os.tcsetpgrp(slave, editor_pid)
    while True:
        _, status = os.waitpid(editor_pid, os.WUNTRACED)
        if os.WIFSTOPPED(status):
            sig = os.WSTOPSIG(status)
            os.write(res_w, f"STOPPED:{sig}\n".encode())
            line = b""
            while not line.endswith(b"\n"):
                chunk = os.read(ctrl_r, 64)
                if not chunk:
                    os._exit(2)
                line += chunk
            os.tcsetpgrp(slave, editor_pid)
            os.killpg(editor_pid, signal.SIGCONT)
        elif os.WIFEXITED(status):
            os.write(res_w, f"EXITED:{os.WEXITSTATUS(status)}\n".encode())
            os._exit(0)
        elif os.WIFSIGNALED(status):
            os.write(res_w, f"SIGNALED:{os.WTERMSIG(status)}\n".encode())
            os._exit(1)

os.close(slave)
os.close(ctrl_r)
os.close(res_w)

def drain_master(idle_sec):
    out = bytearray()
    while True:
        r, _, _ = select.select([master], [], [], idle_sec)
        if not r:
            break
        try:
            chunk = os.read(master, 65536)
        except OSError:
            break
        if not chunk:
            break
        out.extend(chunk)
    return bytes(out)

# Wait for *scratch* readiness.
boot = bytearray()
deadline = time.monotonic() + 20.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master], [], [], 0.2)
    if r:
        chunk = os.read(master, 65536)
        if not chunk:
            break
        boot.extend(chunk)
        if b"*scratch*" in boot and b"This buffer is for text that is not saved" in boot:
            break
else:
    sys.stderr.write("editor did not reach *scratch*\n")
    sys.exit(3)

drain_master(0.3)

# Send C-z and collect all PTY output produced BEFORE SIGCONT.
os.write(master, b"\x1a")
before_cont = bytearray()
res_buf = b""
deadline = time.monotonic() + 15.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master, res_r], [], [], 0.2)
    if master in r:
        try:
            chunk = os.read(master, 65536)
            if chunk:
                before_cont.extend(chunk)
        except OSError:
            pass
    if res_r in r:
        chunk = os.read(res_r, 1024)
        if chunk:
            res_buf += chunk
            if b"\n" in res_buf:
                break
else:
    sys.stderr.write(f"editor did not stop on SIGTSTP; res={res_buf!r}\n")
    sys.exit(4)

stop_line, res_buf = res_buf.split(b"\n", 1)
if stop_line != f"STOPPED:{int(signal.SIGTSTP)}".encode():
    sys.stderr.write(f"unexpected stop event: {stop_line!r}\n")
    sys.exit(5)

before_cont.extend(drain_master(0.25))

# Now resume the stopped process group with SIGCONT and collect resume bytes.
os.write(ctrl_w, b"CONT\n")
after_cont = bytearray()
needle = b"\x1b[?1000h\x1b[?1003h\x1b[?1006h"
deadline = time.monotonic() + 15.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master], [], [], 0.2)
    if r:
        try:
            chunk = os.read(master, 65536)
            if chunk:
                after_cont.extend(chunk)
                if needle in after_cont:
                    break
        except OSError:
            break
after_cont.extend(drain_master(0.2))

# Exit cleanly with C-x C-c.
os.write(master, b"\x18\x03")
deadline = time.monotonic() + 15.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master, res_r], [], [], 0.2)
    if master in r:
        try:
            os.read(master, 65536)
        except OSError:
            pass
    if res_r in r:
        chunk = os.read(res_r, 1024)
        if chunk:
            res_buf += chunk
            if b"\n" in res_buf:
                break
else:
    sys.stderr.write("editor did not exit after SIGCONT + C-x C-c\n")
    sys.exit(6)

exit_line, _ = res_buf.split(b"\n", 1)
os.waitpid(shell_pid, 0)
if exit_line != b"EXITED:0":
    sys.stderr.write(f"unexpected exit line: {exit_line!r}\n")
    sys.exit(7)

print(before_cont.hex())
print(after_cont.hex())
"#;

    let temp = tempfile::tempdir().expect("tempdir for real SIGTSTP test");
    let home = temp.path().join("home");
    let tmp = temp.path().join("tmp");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&tmp).unwrap();

    let mut cmd = Command::new("python3");
    cmd.arg("-c").arg(DRIVER).arg(program).arg("-nw").arg("-Q");
    if is_gnu {
        cmd.arg("-no-comp-spawn")
            .arg(QUIET_NATIVE_COMP_EVAL)
            .arg(QUIET_MX_SUGGESTION_EVAL);
    } else {
        cmd.arg(QUIET_MX_SUGGESTION_EVAL);
    }
    cmd.arg("--eval")
        .arg(SETUP_EL)
        .env("TERM", TERM_TYPE)
        .env("HOME", &home)
        .env("TMPDIR", &tmp)
        .env_remove("LANGUAGE")
        .env_remove("LC_CTYPE")
        .env_remove("EMACSLOADPATH");

    let output = cmd.output().expect("spawn python3 job-control driver");
    assert!(
        output.status.success(),
        "{label}: real SIGTSTP driver failed (status {:?}):\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 hex lines");
    let mut lines = stdout.lines();
    let before_cont = decode_hex(lines.next().expect("before_cont hex line"));
    let after_cont = decode_hex(lines.next().expect("after_cont hex line"));

    // Reset strings must arrive BEFORE SIGCONT while stopped on SIGTSTP,
    // and set strings must NOT appear until AFTER SIGCONT.
    assert_reset_order(&before_cont, label, "real SIGTSTP before SIGCONT");
    assert!(
        find_subslice(&before_cont, MOUSE_ENABLE).is_none(),
        "{label}: mouse enable arrived before SIGCONT: {:?}",
        String::from_utf8_lossy(&before_cont)
    );
    assert!(
        find_subslice(&before_cont, b"<TTY-MODE-SET-2>").is_none(),
        "{label}: tty-mode-set-strings arrived before SIGCONT: {:?}",
        String::from_utf8_lossy(&before_cont)
    );
    assert_set_order(&after_cont, 0, label, "real SIGCONT resume");
}

fn check_exit(program: &Path, label: &str, is_gnu: bool) {
    let mut session = spawn_editor(program, label, is_gnu);
    session.clear_recent_output();
    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit after C-x C-c:\n{}",
        session.text_grid().join("\n")
    );
    assert_reset_order(session.recent_output(), label, "exit C-x C-c");
}

fn check_suspend_emacs_and_resume(program: &Path, label: &str, is_gnu: bool) {
    let mut session = spawn_editor(program, label, is_gnu);
    session.clear_recent_output();
    session.send_key("C-z");
    wait_for_bytes(&mut session, MOUSE_ENABLE, Duration::from_secs(10));
    let output = session.recent_output().to_vec();
    let after_te = assert_reset_order(&output, label, "C-z suspend");
    assert_set_order(&output, after_te, label, "C-z resume");

    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit cleanly after C-z resume"
    );
}

fn check_suspend_tty_and_resume_tty(program: &Path, label: &str, is_gnu: bool) {
    let mut session = spawn_editor(program, label, is_gnu);
    session.clear_recent_output();
    eval_expression_one(&mut session, "(progn (suspend-tty) (resume-tty))");
    wait_for_bytes(&mut session, MOUSE_ENABLE, Duration::from_secs(10));
    let output = session.recent_output().to_vec();
    let after_te = assert_reset_order(&output, label, "suspend-tty");
    assert_set_order(&output, after_te, label, "resume-tty");

    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit cleanly after suspend-tty/resume-tty"
    );
}

fn check_suspend_emacs_hooks_cannot_suspend_and_bad_arg(
    program: &Path,
    label: &str,
    is_gnu: bool,
    true_program: &Path,
) -> String {
    let temp = tempfile::tempdir().expect("tempdir for suspend-emacs parity test");
    let result_path = temp.path().join("result.sexp");
    let mut session = spawn_editor_with(
        program,
        label,
        is_gnu,
        Some(SETUP_EL),
        &[("SHELL", true_program)],
    );
    session.clear_recent_output();
    let expr = format!(
        "(let ((events nil) (suspend-hook nil) (suspend-resume-hook nil) (cannot-suspend t)) \
           (add-hook 'suspend-hook (lambda () (push 'suspend events))) \
           (add-hook 'suspend-resume-hook (lambda () (push 'resume events))) \
           (let* ((bad (condition-case err (suspend-emacs 123) (error err))) \
                  (after-bad (copy-sequence events)) \
                  (ret (suspend-emacs)) \
                  (after-subshell (nreverse events))) \
             (with-temp-file {:?} \
               (insert (prin1-to-string (list bad after-bad ret after-subshell))))))",
        result_path.to_str().expect("UTF-8 result path")
    );
    eval_expression_one(&mut session, &expr);
    let recorded = wait_for_file(&result_path, Duration::from_secs(10));
    wait_for_bytes(&mut session, MOUSE_ENABLE, Duration::from_secs(10));
    let output = session.recent_output().to_vec();
    let after_te = assert_reset_order(&output, label, "cannot-suspend subshell");
    assert_set_order(&output, after_te, label, "cannot-suspend resume");

    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit cleanly after cannot-suspend subshell"
    );
    recorded
}

fn check_suspend_emacs_with_suspended_second_tty(
    program: &Path,
    label: &str,
    is_gnu: bool,
) -> String {
    let temp = tempfile::tempdir().expect("tempdir for second-tty suspend-emacs test");
    let result_path = temp.path().join("result.sexp");
    let mut secondary = SecondaryPty::open();
    let mut session = spawn_editor_with(program, label, is_gnu, None, &[]);
    let expr = format!(
        "(let* ((f1 (selected-frame)) \
                (f2 (make-terminal-frame '((tty . {:?}) (tty-type . {:?})))) \
                (t2 (frame-terminal f2))) \
           (suspend-tty t2) \
           (select-frame f1) \
           (let ((res (condition-case err (suspend-emacs) (error err)))) \
             (delete-terminal t2) \
             (with-temp-file {:?} (insert (prin1-to-string res)))))",
        secondary.path,
        TERM_TYPE,
        result_path.to_str().expect("UTF-8 result path")
    );
    eval_expression_one(&mut session, &expr);
    let recorded = wait_for_file(&result_path, Duration::from_secs(10));
    let _ = secondary.drain(50);

    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit cleanly after second-tty suspend-emacs check"
    );
    recorded
}

fn check_secondary_tty_delete_and_exit_isolation(program: &Path, label: &str, is_gnu: bool) {
    let mut secondary = SecondaryPty::open();
    let mut session = spawn_editor_with(program, label, is_gnu, None, &[]);

    let setup_expr = format!(
        "(progn \
           (setq test-primary-frame (selected-frame)) \
           (push \"<PRIMARY-RESET>\" (terminal-parameter nil 'tty-mode-reset-strings)) \
           (setq test-secondary-frame \
                 (make-terminal-frame '((tty . {:?}) (tty-type . {:?})))) \
           (push \"<SECONDARY-RESET>\" \
                 (terminal-parameter (frame-terminal test-secondary-frame) \
                                     'tty-mode-reset-strings)) \
           (select-frame test-primary-frame))",
        secondary.path, TERM_TYPE
    );
    eval_expression_one(&mut session, &setup_expr);
    // Drain initial secondary TTY startup bytes before deleting it.
    let _ = secondary.wait_for(TERM_TI, Duration::from_secs(10));
    let _ = secondary.drain(150);
    session.clear_recent_output();

    eval_expression_one(
        &mut session,
        "(delete-terminal (frame-terminal test-secondary-frame))",
    );
    let sec_delete = secondary.wait_for(TERM_TE, Duration::from_secs(10));
    let sec_reset = find_subslice(&sec_delete, b"<SECONDARY-RESET>").unwrap_or_else(|| {
        panic!(
            "{label}: secondary PTY missing <SECONDARY-RESET> on delete-terminal: {:?}",
            String::from_utf8_lossy(&sec_delete)
        )
    });
    let sec_ke = find_subslice_from(&sec_delete, sec_reset + 1, TERM_KE).unwrap_or_else(|| {
        panic!(
            "{label}: secondary PTY missing ke after <SECONDARY-RESET>: {:?}",
            String::from_utf8_lossy(&sec_delete)
        )
    });
    let sec_te =
        find_subslice_from(&sec_delete, sec_ke + TERM_KE.len(), TERM_TE).unwrap_or_else(|| {
            panic!(
                "{label}: secondary PTY missing te after ke: {:?}",
                String::from_utf8_lossy(&sec_delete)
            )
        });
    assert!(sec_reset < sec_te);
    assert!(
        find_subslice(&sec_delete, b"<PRIMARY-RESET>").is_none(),
        "{label}: secondary PTY must not receive primary reset string"
    );
    session.read(Duration::from_millis(150));
    let pri_during_delete = session.recent_output().to_vec();
    assert!(
        find_subslice(&pri_during_delete, b"<SECONDARY-RESET>").is_none(),
        "{label}: primary PTY must not receive secondary reset string"
    );
    assert!(
        find_subslice(&pri_during_delete, b"<PRIMARY-RESET>").is_none(),
        "{label}: primary PTY must not reset when deleting secondary terminal"
    );

    session.clear_recent_output();
    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit after secondary delete-terminal"
    );
    let pri_exit = session.recent_output();
    let pri_reset = find_subslice(pri_exit, b"<PRIMARY-RESET>").unwrap_or_else(|| {
        panic!(
            "{label}: primary PTY missing <PRIMARY-RESET> on exit: {:?}",
            String::from_utf8_lossy(pri_exit)
        )
    });
    let pri_te = find_subslice_from(pri_exit, pri_reset + 1, TERM_TE).unwrap_or_else(|| {
        panic!(
            "{label}: primary PTY missing te after <PRIMARY-RESET> on exit: {:?}",
            String::from_utf8_lossy(pri_exit)
        )
    });
    assert!(pri_reset < pri_te);
    assert!(
        find_subslice(pri_exit, b"<SECONDARY-RESET>").is_none(),
        "{label}: primary PTY exit must not contain secondary reset string"
    );
}

fn check_cannot_suspend_real_subshell_survives_sigint(program: &Path, label: &str, is_gnu: bool) {
    let sh = Path::new("/bin/sh");
    assert!(sh.is_file(), "/bin/sh must exist for subshell test");
    let mut session = spawn_editor_with(program, label, is_gnu, Some(SETUP_EL), &[("SHELL", sh)]);
    session.clear_recent_output();

    // Submit `(let ((cannot-suspend t)) (suspend-emacs))` without waiting for
    // minibuffer exit, because the subshell owns the terminal until it exits.
    session.send_key("M-:");
    let prompt_ready = |grid: &[String]| grid.last().is_some_and(|row| row.contains("Eval:"));
    session.read_until(Duration::from_secs(8), prompt_ready);
    session.read(Duration::from_millis(200));
    session.paste("(let ((cannot-suspend t)) (suspend-emacs))");
    session.send_key("RET");

    wait_for_bytes(&mut session, TERM_TE, Duration::from_secs(10));

    // Use `printf` concatenation so the expected marker cannot be satisfied by
    // cooked-mode TTY input echo alone: `/bin/sh` must actually execute it.
    session.send(b"printf '%s-%s\\n' NEOMACS SUBSHELL_READY\n");
    wait_for_bytes(
        &mut session,
        b"NEOMACS-SUBSHELL_READY",
        Duration::from_secs(10),
    );

    // Send C-c (0x03) AND explicitly deliver SIGINT/SIGQUIT/SIGTERM to `$PPID`
    // (the waiting editor): some `/bin/sh` builds (like interactive bash) move
    // themselves into a new foreground pgrp, which shields `$PPID` from PTY C-c
    // while non-job-control shells (`dash`, POSIX `sh`) leave the editor in the
    // foreground pgrp. GNU `sys_subshell` ignores SIGINT/SIGQUIT/SIGTERM/SIGIO
    // in the parent until the subshell exits (`src/sysdep.c:674-688,747-753`).
    session.send(b"\x03");
    session.read(Duration::from_millis(250));
    session.send(
        b"kill -INT $PPID; kill -QUIT $PPID; kill -TERM $PPID; printf '%s-%s\\n' NEOMACS AFTER_SIGNALS\nexit 0\n",
    );
    wait_for_bytes(&mut session, MOUSE_ENABLE, Duration::from_secs(10));

    let output = session.recent_output().to_vec();
    let after_te = assert_reset_order(&output, label, "cannot-suspend /bin/sh");
    let ready_pos = find_subslice_from(&output, after_te, b"NEOMACS-SUBSHELL_READY")
        .unwrap_or_else(|| {
            panic!(
                "{label}: /bin/sh did not execute READY printf: {:?}",
                String::from_utf8_lossy(&output[after_te..])
            )
        });
    let after_sigint = find_subslice_from(&output, ready_pos + 1, b"NEOMACS-AFTER_SIGNALS")
        .unwrap_or_else(|| {
            panic!(
                "{label}: /bin/sh did not execute AFTER_SIGNALS printf after signals: {:?}",
                String::from_utf8_lossy(&output[ready_pos..])
            )
        });
    assert_set_order(
        &output,
        after_sigint,
        label,
        "cannot-suspend /bin/sh resume",
    );

    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: editor did not survive subshell signals and exit cleanly"
    );
}

fn check_emergency_escape_mode_strings(program: &Path, label: &str, is_gnu: bool) {
    // Non-orphaned job-control wrapper (same topology as `tty_emergency_escape.rs`)
    // so the 2nd C-g under `inhibit-quit` genuinely delivers `SIGTSTP`.
    const HELPER: &str = r#"import os, signal, sys, time
signal.signal(signal.SIGTTOU, signal.SIG_IGN)
signal.signal(signal.SIGTTIN, signal.SIG_IGN)
stopped_path, resume_path = sys.argv[1:3]
cmd = sys.argv[3:]
pid = os.fork()
if pid == 0:
    os.setpgid(0, 0)
    signal.signal(signal.SIGTTOU, signal.SIG_DFL)
    signal.signal(signal.SIGTTIN, signal.SIG_DFL)
    os.execvp(cmd[0], cmd)
try:
    os.setpgid(pid, pid)
except PermissionError:
    pass
os.tcsetpgrp(0, pid)
while True:
    _, st = os.waitpid(pid, os.WUNTRACED)
    if os.WIFSTOPPED(st):
        os.tcsetpgrp(0, os.getpgrp())
        with open(stopped_path, "w") as f:
            f.write(str(os.WSTOPSIG(st)))
        while not os.path.exists(resume_path):
            time.sleep(0.01)
        os.tcsetpgrp(0, pid)
        os.kill(-pid, signal.SIGCONT)
        continue
    sys.exit(0 if os.WIFEXITED(st) and os.WEXITSTATUS(st) == 0 else 1)
"#;

    let temp = tempfile::tempdir().expect("tempdir for emergency-escape mode-strings test");
    let helper = temp.path().join("job_control.py");
    let script = temp.path().join("probe.el");
    let ready = temp.path().join("ready");
    let done = temp.path().join("done");
    let stopped = temp.path().join("stopped");
    let resume = temp.path().join("resume");
    std::fs::write(&helper, HELPER).expect("write job-control helper");

    let probe = format!(
        "{SETUP_EL}\n\
         (let ((inhibit-quit t))\n\
           (with-temp-file {:?} (insert \"ready\\n\"))\n\
           (while (not (file-exists-p {:?}))\n\
             (sleep-for 0.02))\n\
           (setq quit-flag nil))\n\
         (kill-emacs 0)\n",
        ready.to_str().expect("UTF-8 ready path"),
        done.to_str().expect("UTF-8 done path"),
    );
    std::fs::write(&script, probe).expect("write probe script");

    let mut launch = TuiLaunch::new("python3")
        .with_uncertified_startup()
        .arg("-I")
        .arg(&helper)
        .arg(&stopped)
        .arg(&resume)
        .arg(program)
        .arg("-nw")
        .arg("-Q");
    if is_gnu {
        launch = launch
            .arg("-no-comp-spawn")
            .arg(QUIET_NATIVE_COMP_EVAL)
            .arg(QUIET_MX_SUGGESTION_EVAL);
    } else {
        launch = launch.arg(QUIET_MX_SUGGESTION_EVAL);
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", temp.path())
        .env("TMPDIR", temp.path());

    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        label,
        TuiTerminalConfig::new(TERM_TYPE, 24, 80),
    );
    let deadline = Instant::now() + Duration::from_secs(25);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "{label}: emergency-escape probe did not reach inhibit-quit loop"
        );
        session.read(Duration::from_millis(40));
    }
    session.read(Duration::from_millis(150));
    session.clear_recent_output();

    // 1st C-g sets quit-flag; 2nd C-g enters emergency escape and stops via SIGTSTP.
    session.send(b"\x07");
    session.read(Duration::from_millis(150));
    session.send(b"\x07");
    let stop_deadline = Instant::now() + Duration::from_secs(15);
    while !stopped.exists() {
        assert!(
            Instant::now() < stop_deadline,
            "{label}: editor did not stop on 2nd C-g emergency escape; raw: {:?}",
            String::from_utf8_lossy(session.recent_output())
        );
        session.read(Duration::from_millis(40));
    }
    session.read(Duration::from_millis(200));
    let before_cont = session.recent_output().to_vec();
    assert_reset_order(&before_cont, label, "emergency escape before SIGCONT");
    assert!(
        find_subslice(&before_cont, MOUSE_ENABLE).is_none(),
        "{label}: mouse enable arrived before SIGCONT on emergency escape"
    );

    // Resume via SIGCONT and answer Auto-save=n, Abort=n -> Continuing...
    session.clear_recent_output();
    std::fs::write(&resume, "go\n").expect("signal SIGCONT");
    wait_for_bytes(
        &mut session,
        b"Auto-save? (y or n) ",
        Duration::from_secs(10),
    );
    session.send(b"n\n");
    wait_for_bytes(
        &mut session,
        b"Abort (and dump core)? (y or n) ",
        Duration::from_secs(10),
    );
    session.send(b"n\n");
    wait_for_bytes(&mut session, MOUSE_ENABLE, Duration::from_secs(10));
    let after_cont = session.recent_output().to_vec();
    assert_set_order(&after_cont, 0, label, "emergency escape resume");

    // Release the probe loop so the editor exits cleanly.
    std::fs::write(&done, "done\n").expect("release probe");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: emergency-escape probe did not exit cleanly"
    );
}

#[test]
fn tty_mode_reset_strings_emitted_on_exit_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_exit(&gnu, "GNU", true);
    check_exit(&neo, "Neomacs", false);
}

#[test]
fn tty_mode_strings_emitted_on_suspend_emacs_and_resume_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_suspend_emacs_and_resume(&gnu, "GNU", true);
    check_suspend_emacs_and_resume(&neo, "Neomacs", false);
}

#[test]
fn tty_mode_strings_emitted_on_suspend_tty_and_resume_tty_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_suspend_tty_and_resume_tty(&gnu, "GNU", true);
    check_suspend_tty_and_resume_tty(&neo, "Neomacs", false);
}

#[test]
fn tty_mode_strings_real_sigtstp_stop_and_sigcont_resume_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    run_real_sigtstp_stop_and_sigcont(&gnu, "GNU", true);
    run_real_sigtstp_stop_and_sigcont(&neo, "Neomacs", false);
}

#[test]
fn suspend_emacs_hooks_cannot_suspend_and_bad_arg_match_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    let true_program = resolve_true_program();
    let gnu_res =
        check_suspend_emacs_hooks_cannot_suspend_and_bad_arg(&gnu, "GNU", true, &true_program);
    let neo_res =
        check_suspend_emacs_hooks_cannot_suspend_and_bad_arg(&neo, "Neomacs", false, &true_program);
    assert_eq!(
        neo_res, gnu_res,
        "suspend-emacs hooks / cannot-suspend / bad-arg mismatch vs GNU"
    );
    assert_eq!(
        gnu_res,
        "((wrong-type-argument stringp 123) nil nil (suspend resume))"
    );
}

#[test]
fn suspend_emacs_errors_with_suspended_second_tty_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    let gnu_res = check_suspend_emacs_with_suspended_second_tty(&gnu, "GNU", true);
    let neo_res = check_suspend_emacs_with_suspended_second_tty(&neo, "Neomacs", false);
    assert_eq!(
        neo_res, gnu_res,
        "suspend-emacs error with suspended second TTY mismatch vs GNU"
    );
    assert_eq!(
        gnu_res,
        "(error \"There are other tty frames open; close them before suspending Emacs\")"
    );
}

#[test]
fn secondary_tty_delete_terminal_and_exit_isolate_reset_strings_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_secondary_tty_delete_and_exit_isolation(&gnu, "GNU", true);
    check_secondary_tty_delete_and_exit_isolation(&neo, "Neomacs", false);
}

#[test]
fn tty_mode_cannot_suspend_real_subshell_survives_sigint_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_cannot_suspend_real_subshell_survives_sigint(&gnu, "GNU", true);
    check_cannot_suspend_real_subshell_survives_sigint(&neo, "Neomacs", false);
}

fn run_suspend_emacs_resize_while_stopped(program: &Path, label: &str, is_gnu: bool) -> String {
    // Non-orphaned job-control wrapper that resizes the PTY from 24x80 to
    // 30x100 while the editor is stopped on SIGTSTP, then resumes it with
    // SIGCONT and records `(list (frame-width) (frame-height))` observed inside
    // `suspend-resume-hook` (GNU `src/keyboard.c:12455,12470-12474`).
    const DRIVER: &str = r#"
import fcntl, os, pty, select, signal, struct, sys, termios, time

result_path = sys.argv[1]
argv = sys.argv[2:]
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
ctrl_r, ctrl_w = os.pipe()
res_r, res_w = os.pipe()

shell_pid = os.fork()
if shell_pid == 0:
    os.close(master)
    os.close(ctrl_w)
    os.close(res_r)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGTTOU, signal.SIG_IGN)
    signal.signal(signal.SIGTTIN, signal.SIG_IGN)
    signal.signal(signal.SIGTSTP, signal.SIG_IGN)
    editor_pid = os.fork()
    if editor_pid == 0:
        os.close(ctrl_r)
        os.close(res_w)
        os.setpgid(0, 0)
        os.dup2(slave, 0)
        os.dup2(slave, 1)
        os.dup2(slave, 2)
        if slave > 2:
            os.close(slave)
        for sig in (signal.SIGTSTP, signal.SIGTTIN, signal.SIGTTOU):
            signal.signal(sig, signal.SIG_DFL)
        os.execv(argv[0], argv)
        os._exit(127)
    try:
        os.setpgid(editor_pid, editor_pid)
    except PermissionError:
        pass
    os.tcsetpgrp(slave, editor_pid)
    while True:
        _, status = os.waitpid(editor_pid, os.WUNTRACED)
        if os.WIFSTOPPED(status):
            sig = os.WSTOPSIG(status)
            os.write(res_w, f"STOPPED:{sig}\n".encode())
            line = b""
            while not line.endswith(b"\n"):
                chunk = os.read(ctrl_r, 64)
                if not chunk:
                    os._exit(2)
                line += chunk
            os.tcsetpgrp(slave, editor_pid)
            os.killpg(editor_pid, signal.SIGCONT)
        elif os.WIFEXITED(status):
            os.write(res_w, f"EXITED:{os.WEXITSTATUS(status)}\n".encode())
            os._exit(0)
        elif os.WIFSIGNALED(status):
            os.write(res_w, f"SIGNALED:{os.WTERMSIG(status)}\n".encode())
            os._exit(1)

os.close(ctrl_r)
os.close(res_w)

boot = bytearray()
deadline = time.monotonic() + 20.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master], [], [], 0.2)
    if r:
        chunk = os.read(master, 65536)
        if not chunk:
            break
        boot.extend(chunk)
        if b"*scratch*" in boot and b"This buffer is for text that is not saved" in boot:
            break
else:
    sys.stderr.write("editor did not reach *scratch*\n")
    sys.exit(3)

# Trigger C-z (suspend-emacs) and wait for real SIGTSTP stop.
os.write(master, b"\x1a")
res_buf = b""
deadline = time.monotonic() + 15.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master, res_r], [], [], 0.2)
    if master in r:
        try:
            os.read(master, 65536)
        except OSError:
            pass
    if res_r in r:
        chunk = os.read(res_r, 1024)
        if chunk:
            res_buf += chunk
            if b"\n" in res_buf:
                break
else:
    sys.stderr.write(f"editor did not stop on SIGTSTP; res={res_buf!r}\n")
    sys.exit(4)

stop_line, res_buf = res_buf.split(b"\n", 1)
if stop_line != f"STOPPED:{int(signal.SIGTSTP)}".encode():
    sys.stderr.write(f"unexpected stop event: {stop_line!r}\n")
    sys.exit(5)

# Resize the PTY while stopped, then resume with SIGCONT.
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
os.close(slave)
os.write(ctrl_w, b"CONT\n")

# Wait for suspend-resume-hook to write the observed dimensions.
deadline = time.monotonic() + 15.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master], [], [], 0.1)
    if r:
        try:
            os.read(master, 65536)
        except OSError:
            pass
    if os.path.exists(result_path) and os.path.getsize(result_path) > 0:
        break
else:
    sys.stderr.write("timed out waiting for suspend-resume-hook result file\n")
    sys.exit(6)

os.write(master, b"\x18\x03")
deadline = time.monotonic() + 15.0
while time.monotonic() < deadline:
    r, _, _ = select.select([master, res_r], [], [], 0.2)
    if master in r:
        try:
            os.read(master, 65536)
        except OSError:
            pass
    if res_r in r:
        chunk = os.read(res_r, 1024)
        if chunk:
            res_buf += chunk
            if b"\n" in res_buf:
                break
else:
    sys.stderr.write("editor did not exit cleanly after resume\n")
    sys.exit(7)

exit_line, _ = res_buf.split(b"\n", 1)
os.waitpid(shell_pid, 0)
if exit_line != b"EXITED:0":
    sys.stderr.write(f"unexpected exit line: {exit_line!r}\n")
    sys.exit(8)
"#;

    let temp = tempfile::tempdir().expect("tempdir for suspend-emacs resize test");
    let home = temp.path().join("home");
    let tmp = temp.path().join("tmp");
    let result_path = temp.path().join("resume-size.sexp");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&tmp).unwrap();

    let hook_el = format!(
        "(progn \
           (menu-bar-mode -1) \
           (add-hook 'suspend-resume-hook \
             (lambda () \
               (with-temp-file {:?} \
                 (insert (prin1-to-string (list (frame-width) (frame-height))))))))",
        result_path.to_str().expect("UTF-8 result path")
    );

    let mut cmd = Command::new("python3");
    cmd.arg("-c")
        .arg(DRIVER)
        .arg(&result_path)
        .arg(program)
        .arg("-nw")
        .arg("-Q");
    if is_gnu {
        cmd.arg("-no-comp-spawn")
            .arg(QUIET_NATIVE_COMP_EVAL)
            .arg(QUIET_MX_SUGGESTION_EVAL);
    } else {
        cmd.arg(QUIET_MX_SUGGESTION_EVAL);
    }
    cmd.arg("--eval")
        .arg(&hook_el)
        .env("TERM", TERM_TYPE)
        .env("HOME", &home)
        .env("TMPDIR", &tmp)
        .env_remove("LANGUAGE")
        .env_remove("LC_CTYPE")
        .env_remove("EMACSLOADPATH");

    let output = cmd.output().expect("spawn suspend-emacs resize driver");
    assert!(
        output.status.success(),
        "{label}: suspend-emacs resize driver failed (status {:?}):\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(&result_path).expect("read resume-size.sexp")
}

#[test]
fn tty_mode_strings_emitted_on_emergency_escape_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_emergency_escape_mode_strings(&gnu, "GNU", true);
    check_emergency_escape_mode_strings(&neo, "Neomacs", false);
}

#[test]
fn tty_mode_suspend_emacs_applies_resize_before_resume_hook_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    let gnu_size = run_suspend_emacs_resize_while_stopped(&gnu, "GNU", true);
    let neo_size = run_suspend_emacs_resize_while_stopped(&neo, "Neomacs", false);
    assert_eq!(
        neo_size, gnu_size,
        "suspend-resume-hook frame size mismatch vs GNU after PTY resize during SIGTSTP"
    );
    assert_eq!(gnu_size, "(100 30)");
}
