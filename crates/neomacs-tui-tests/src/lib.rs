#![cfg(unix)]

//! TUI comparison test harness for Neomacs vs GNU Emacs.
//!
//! Spawns both editors in isolated pseudo-terminals, feeds identical
//! keystrokes, and compares the rendered screen cell by cell using the
//! `vt100` virtual terminal emulator.
//!
//! # Architecture
//!
//! - [`TuiSession`] wraps a child process in a PTY with a `vt100::Parser`.
//!   Call [`TuiSession::send`] to type keys and [`TuiSession::read`] to
//!   advance the parser. [`TuiSession::screen`] returns the current
//!   virtual screen.
//!
//! - [`emacs_key`] translates Emacs key descriptions (`"C-x"`, `"M-x"`,
//!   `"RET"`) into the raw bytes a terminal would send.
//!
//! - [`diff_screens`] compares two `vt100::Screen` snapshots and returns
//!   a list of [`CellDiff`] entries for every mismatched cell.
//!
//! - [`diff_screens_text`] is a simpler text-only comparison that ignores
//!   face attributes and normalises product names.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod launch;

pub use launch::TuiLaunch;

// ── Session ──────────────────────────────────────────────────────────

/// Default terminal size for tests.
pub const COLS: u16 = 160;
pub const ROWS: u16 = 50;

/// The ERASE character a test PTY reports to the editor it hosts.
///
/// This is the byte `stty -a` shows as `erase` and the one GNU publishes as
/// `tty-erase-char` (`init_sys_modes`, src/sysdep.c:1130). It is not cosmetic:
/// `normal-erase-is-backspace-setup-frame` (lisp/simple.el:11093) turns the
/// mode on only when the terminal erases with `^H`, and the mode then
/// `key-translate`s `C-h` to `DEL`. A suite that only ever runs on the pty
/// default is blind to every behaviour that decision gates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PtyEraseChar {
    /// Leave the pty's own default, which on Linux is DEL (`^?`, 0x7f). Most
    /// real terminals are configured this way, and it leaves
    /// `normal-erase-is-backspace-mode` off.
    TerminalDefault,
    /// Erase with Backspace (`^H`, 0x08), the configuration under which GNU
    /// enables `normal-erase-is-backspace-mode` so Backspace deletes a
    /// character instead of opening the help prefix.
    Backspace,
}

/// Apply [`PtyEraseChar`] to the pty slave before the child is spawned, so the
/// editor's first `tcgetattr` already sees it.
fn set_pty_erase_char(pts: &pty_process::blocking::Pts, erase: PtyEraseChar) {
    let byte = match erase {
        // Leaving the default untouched keeps every existing test's terminal
        // byte-for-byte what it was before this option existed.
        PtyEraseChar::TerminalDefault => return,
        PtyEraseChar::Backspace => 0x08,
    };
    let fd = std::os::fd::AsRawFd::as_raw_fd(pts);
    // SAFETY: `fd` is the pts we are about to hand to the child; tcgetattr
    // only fills the termios out-parameter and tcsetattr only reads it.
    unsafe {
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(
            libc::tcgetattr(fd, termios.as_mut_ptr()),
            0,
            "tcgetattr on the test pty"
        );
        let mut termios = termios.assume_init();
        termios.c_cc[libc::VERASE] = byte;
        assert_eq!(
            libc::tcsetattr(fd, libc::TCSANOW, &termios),
            0,
            "tcsetattr on the test pty"
        );
    }
}

fn wait_for_pty_writable(pty: &pty_process::blocking::Pty, timeout: Duration) {
    let timeout_ms = timeout.as_millis().min(50) as i32;
    let fd = std::os::fd::AsRawFd::as_raw_fd(pty);
    unsafe {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLOUT,
            revents: 0,
        };
        let _ = libc::poll(&mut pfd, 1, timeout_ms);
    }
}

/// A test-owned temporary directory that removes its whole tree on drop.
///
/// Keep this value alive for as long as any editor may access a path beneath
/// it. Returning a bare [`PathBuf`] from a fixture constructor loses that
/// ownership fact and leaves the directory behind after the test exits.
pub struct TuiTempDirectory {
    directory: tempfile::TempDir,
}

impl TuiTempDirectory {
    /// Create an isolated fixture root with a recognizable name prefix.
    pub fn new(prefix: &str) -> Self {
        let directory = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("create TUI fixture temp directory");
        Self { directory }
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }
}

impl Deref for TuiTempDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl AsRef<Path> for TuiTempDirectory {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

/// A test-owned file whose private parent directory is removed on drop.
///
/// The wrapper dereferences to the file path, while retaining the directory
/// guard that makes cleanup unconditional during ordinary test unwinding.
pub struct TuiTempFile {
    _directory: TuiTempDirectory,
    path: PathBuf,
}

impl TuiTempFile {
    pub fn new(prefix: &str, file_name: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Self {
        let directory = TuiTempDirectory::new(prefix);
        let path = directory.join(file_name);
        std::fs::write(&path, contents).expect("write TUI temporary fixture file");
        Self {
            _directory: directory,
            path,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Deref for TuiTempFile {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl AsRef<Path> for TuiTempFile {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

/// Whether a session directory belongs to the harness or its caller.
///
/// The enum couples the path with its cleanup policy. An owned directory
/// cannot be represented without its RAII guard, while a borrowed directory
/// can never accidentally enter the cleanup path.
enum SessionDirectory {
    Owned(TuiTempDirectory),
    Borrowed(PathBuf),
}

impl SessionDirectory {
    fn for_launch(path: Option<PathBuf>, kind: &str, name: &str) -> Self {
        match path {
            Some(path) => Self::Borrowed(path),
            None => {
                let safe_name = name
                    .chars()
                    .map(|ch| {
                        if ch.is_ascii_alphanumeric() {
                            ch.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect::<String>();
                Self::Owned(TuiTempDirectory::new(&format!(
                    "neomacs-tui-test-{kind}-{safe_name}-"
                )))
            }
        }
    }

    fn path(&self) -> &Path {
        match self {
            Self::Owned(directory) => directory.path(),
            Self::Borrowed(path) => path,
        }
    }
}

/// A TUI editor session running inside an isolated PTY.
pub struct TuiSession {
    pty: pty_process::blocking::Pty,
    _child: std::process::Child,
    parser: vt100::Parser,
    recent_output: Vec<u8>,
    home: SessionDirectory,
    // Keep TMPDIR isolated per session: interactive Org chooses one of only
    // 1,000 babel-stable names there and cleans it from kill-emacs-hook. A
    // shared pool makes parallel/repeated tests contend on that finite space.
    _tmp: SessionDirectory,
    pub name: String,
}

impl TuiSession {
    /// Spawn `cmd` (e.g. `"emacs -nw -Q"`) in a new PTY.
    pub fn spawn(cmd: &str, name: &str) -> Self {
        Self::spawn_launch(TuiLaunch::from(cmd), name)
    }

    /// Spawn a structured process description in a new PTY.
    pub fn spawn_launch(launch: TuiLaunch, name: &str) -> Self {
        Self::spawn_launch_with_erase_char(launch, name, PtyEraseChar::TerminalDefault)
    }

    /// Spawn a structured process description in a new PTY whose ERASE
    /// character is ERASE.
    ///
    /// The ERASE byte is what `stty -a` reports and what GNU reads into
    /// `tty-erase-char` (`init_sys_modes`, src/sysdep.c:1130). It decides
    /// whether `normal-erase-is-backspace-mode` turns on, so a terminal that
    /// erases with `^H` exercises an entirely different key-translation path
    /// from the pty default of `^?`.
    pub fn spawn_launch_with_erase_char(
        launch: TuiLaunch,
        name: &str,
        erase: PtyEraseChar,
    ) -> Self {
        let (pty, pts) = pty_process::blocking::open().expect("open pty");
        pty.resize(pty_process::Size::new(ROWS, COLS))
            .expect("resize pty");
        set_pty_erase_char(&pts, erase);

        let supplied_home = launch.environment_value("HOME").map(PathBuf::from);
        let supplied_tmp = launch.environment_value("TMPDIR").map(PathBuf::from);
        let home = SessionDirectory::for_launch(supplied_home, "home", name);
        let tmp = SessionDirectory::for_launch(supplied_tmp, "tmp", name);
        if matches!(&home, SessionDirectory::Owned(_)) {
            std::fs::create_dir_all(home.path().join(".emacs.d"))
                .expect("create isolated tui test HOME");
        }

        let TuiLaunch {
            program,
            args,
            env: environment,
            env_remove: removed_environment,
            current_dir,
        } = launch;
        let mut command = pty_process::blocking::Command::new(program);
        for arg in args {
            command = command.arg(arg);
        }
        command = command
            .env("TERM", "screen-256color")
            .env("COLUMNS", COLS.to_string())
            .env("LINES", ROWS.to_string())
            // Prevent user config from interfering while also isolating
            // concurrent TUI tests from one another.
            .env("HOME", home.path())
            .env("TMPDIR", tmp.path());
        for var in [
            "RUST_LOG",
            "NEOMACS_LOG_FILE",
            "NEOMACS_LOG_TO_FILE",
            "NEOMACS_DUMP_TTY_GLYPHS",
        ] {
            if let Some(value) = std::env::var_os(var) {
                command = command.env(var, value);
            }
        }
        for name in removed_environment {
            command = command.env_remove(name);
        }
        for (name, value) in environment {
            command = command.env(name, value);
        }
        if let Some(current_dir) = current_dir {
            command = command.current_dir(current_dir);
        }

        let child = command.spawn(pts).expect("spawn");

        let parser = vt100::Parser::new(ROWS, COLS, 0);

        TuiSession {
            pty,
            _child: child,
            parser,
            recent_output: Vec::new(),
            home,
            _tmp: tmp,
            name: name.to_string(),
        }
    }

    /// Spawn GNU Emacs in TUI mode.
    pub fn gnu_emacs(extra_args: &str) -> Self {
        Self::gnu_emacs_with_erase_char(extra_args, PtyEraseChar::TerminalDefault)
    }

    /// Spawn GNU Emacs in TUI mode on a PTY whose ERASE character is ERASE.
    pub fn gnu_emacs_with_erase_char(extra_args: &str, erase: PtyEraseChar) -> Self {
        // Keep the GNU oracle focused on TUI behavior.  On NixOS the async
        // native compiler can fail after startup and pop *Warnings*, which
        // pollutes the rendered screen unrelated to the command under test.
        let quiet_native_comp = "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent)(push'(native-compiler)warning-suppress-types)(mapc'kill-process(process-list)))";
        let launch = TuiLaunch::new("emacs")
            .args(["-nw", "-Q", "-no-comp-spawn", quiet_native_comp])
            .args(extra_args.split_whitespace());
        Self::spawn_launch_with_erase_char(launch, "GNU", erase)
    }

    /// Spawn GNU Emacs in TUI mode WITHOUT `-Q`, loading the user's init
    /// file (e.g. Doom config).  Uses the real HOME so Doom is found.
    /// For face/theme comparison tests.
    pub fn gnu_emacs_with_init(extra_args: &str) -> Self {
        Self::gnu_emacs_with_init_args(extra_args.split_whitespace())
    }

    /// Structured-argument counterpart of [`Self::gnu_emacs_with_init`].
    /// Paths and Lisp forms remain distinct OS arguments rather than passing
    /// through whitespace tokenization.
    pub fn gnu_emacs_with_init_args<I, S>(extra_args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let real_home = PathBuf::from(std::env::var("HOME").expect("HOME"));
        let launch = TuiLaunch::new("emacs")
            .arg("-nw")
            .args(extra_args)
            .env("HOME", real_home.as_os_str());
        Self::spawn_launch(launch, "GNU")
    }

    /// Spawn Neomacs in TUI mode WITHOUT `-Q` so the user's init file
    /// (e.g. Doom Emacs config) is loaded.  Uses the real HOME.
    /// For face/theme tests.
    pub fn neomacs_with_init(extra_args: &str) -> Self {
        Self::neomacs_with_init_args(extra_args.split_whitespace())
    }

    /// Structured-argument counterpart of [`Self::neomacs_with_init`].
    pub fn neomacs_with_init_args<I, S>(extra_args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let workspace = workspace_root();
        let bin = neomacs_binary_path(&workspace);
        assert!(
            bin.exists(),
            "neomacs binary not found at {}",
            bin.display()
        );
        let real_home = PathBuf::from(std::env::var("HOME").expect("HOME"));
        let launch = TuiLaunch::new(bin.as_os_str())
            .arg("-nw")
            .args(extra_args)
            .env("HOME", real_home.as_os_str());
        Self::spawn_launch(launch, "NEO")
    }

    /// Spawn Neomacs in TUI mode.
    ///
    /// `NEOMACS_TUI_NEOMACS_BIN` can override the binary path. Otherwise, the
    /// harness uses `target/release/neomacs`.
    pub fn neomacs(extra_args: &str) -> Self {
        Self::neomacs_with_erase_char(extra_args, PtyEraseChar::TerminalDefault)
    }

    /// Spawn Neomacs in TUI mode on a PTY whose ERASE character is ERASE.
    pub fn neomacs_with_erase_char(extra_args: &str, erase: PtyEraseChar) -> Self {
        let workspace = workspace_root();
        let bin = neomacs_binary_path(&workspace);
        assert!(
            bin.exists(),
            "neomacs binary not found at {}\nRun `cargo build --release -p neomacs` \
             or set NEOMACS_TUI_NEOMACS_BIN.",
            bin.display()
        );
        let launch = TuiLaunch::new(bin.as_os_str())
            .args(["-nw", "-Q"])
            .args(extra_args.split_whitespace());
        Self::spawn_launch_with_erase_char(launch, "NEO", erase)
    }

    /// Read PTY output until the editor has been quiet for
    /// [`IDLE_CUTOFF`] *after at least one byte has arrived*, or
    /// `max_timeout` elapses — whichever comes first. Feeds whatever
    /// it reads into the vt100 parser.
    ///
    /// The `max_timeout` argument is a safety cap, not the expected
    /// runtime: a TUI editor that starts emitting within 100 ms and
    /// finishes within another 200 ms will return after ~300 ms, not
    /// after the full timeout. The "saw at least one byte" gate
    /// guards against returning immediately after a `send_keys()`
    /// that the editor hasn't yet begun to process.
    pub fn read(&mut self, max_timeout: Duration) {
        /// How long a PTY must be quiet *after* the first byte to
        /// count as settled. Tune up if editors start pausing
        /// mid-render longer than this.
        const IDLE_CUTOFF: Duration = Duration::from_millis(300);
        /// Each `poll()` call waits at most this long before we
        /// re-check idle / max-deadline conditions.
        const POLL_SLICE_MS: i32 = 50;
        let max_deadline = Instant::now() + max_timeout;
        let mut last_activity: Option<Instant> = None;
        let mut buf = [0u8; 65536];
        loop {
            let now = Instant::now();
            if now >= max_deadline {
                break;
            }
            if let Some(last) = last_activity
                && now.duration_since(last) >= IDLE_CUTOFF
            {
                break;
            }
            let fd = std::os::fd::AsRawFd::as_raw_fd(&self.pty);
            let ready = unsafe {
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                libc::poll(&mut pfd, 1, POLL_SLICE_MS) > 0 && (pfd.revents & libc::POLLIN) != 0
            };
            if ready {
                match self.pty.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        self.recent_output.extend_from_slice(&buf[..n]);
                        if self.recent_output.len() > 262_144 {
                            let drain = self.recent_output.len() - 262_144;
                            self.recent_output.drain(..drain);
                        }
                        self.parser.process(&buf[..n]);
                        last_activity = Some(Instant::now());
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
            }
        }
    }

    /// Send raw bytes to the PTY.
    pub fn send(&mut self, data: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut written = 0;

        while written < data.len() {
            match self.pty.write(&data[written..]) {
                Ok(0) => panic!(
                    "{} PTY write returned 0 after {written}/{} bytes",
                    self.name,
                    data.len()
                ),
                Ok(n) => written += n,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        panic!(
                            "{} PTY write timed out after {written}/{} bytes",
                            self.name,
                            data.len()
                        );
                    }
                    wait_for_pty_writable(
                        &self.pty,
                        deadline.saturating_duration_since(Instant::now()),
                    );
                }
                Err(e) => panic!(
                    "{} PTY write failed after {written}/{} bytes: {e}",
                    self.name,
                    data.len()
                ),
            }
        }
    }

    /// Like [`TuiSession::read`] but keep reading past idle gaps until
    /// `predicate` returns true on some row of the rendered grid, or
    /// `max_timeout` elapses. Useful when a command's legitimate
    /// render pipeline has mid-burst pauses longer than
    /// `IDLE_CUTOFF` (e.g. `view-hello-file` running format-decode →
    /// enriched-decode → view-mode setup) so plain idle-detection
    /// returns too eagerly.
    pub fn read_until<F>(&mut self, max_timeout: Duration, predicate: F)
    where
        F: Fn(&[String]) -> bool,
    {
        let deadline = Instant::now() + max_timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.read(remaining);
            if predicate(&self.text_grid()) {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
        }
    }

    /// Resize the underlying PTY and the virtual terminal parser.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.pty
            .resize(pty_process::Size::new(rows, cols))
            .expect("resize pty");
        self.parser.screen_mut().set_size(rows, cols);
    }

    /// Send an Emacs key description (e.g. `"C-x"`, `"M-x"`, `"RET"`).
    pub fn send_key(&mut self, key: &str) {
        self.send(&emacs_key(key));
    }

    /// Send a sequence of keys separated by spaces (e.g. `"C-x 2"`).
    pub fn send_keys(&mut self, keys: &str) {
        for part in keys.split_whitespace() {
            self.send_key(part);
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Get the current virtual terminal screen.
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Get the current virtual terminal dimensions as `(rows, cols)`.
    pub fn screen_size(&self) -> (u16, u16) {
        self.screen().size()
    }

    /// Get the text content of a single row (0-indexed).
    pub fn row_text(&self, row: u16) -> String {
        let (_, cols) = self.screen_size();
        self.screen().contents_between(row, 0, row, cols)
    }

    /// Get all rows as a Vec of strings.
    pub fn text_grid(&self) -> Vec<String> {
        let (rows, _) = self.screen_size();
        (0..rows).map(|r| self.row_text(r)).collect()
    }

    /// Clear the accumulated raw PTY output captured by [`read`].
    pub fn clear_recent_output(&mut self) {
        self.recent_output.clear();
    }

    /// Borrow the recent raw PTY output captured by [`read`].
    pub fn recent_output(&self) -> &[u8] {
        &self.recent_output
    }

    /// Return the isolated HOME directory used for this session.
    pub fn home_dir(&self) -> &std::path::Path {
        self.home.path()
    }
}

const NEOMACS_TUI_NEOMACS_BIN: &str = "NEOMACS_TUI_NEOMACS_BIN";

fn neomacs_binary_path(workspace: &Path) -> PathBuf {
    neomacs_binary_path_from_override(workspace, std::env::var_os(NEOMACS_TUI_NEOMACS_BIN))
}

/// The neomacs binary this test run drives, for suites that spawn the editor
/// themselves instead of through [`TuiSession`] -- e.g. one that runs it to
/// completion on a pty of a chosen TERM and reads a file it wrote.
pub fn neomacs_binary() -> PathBuf {
    neomacs_binary_path(&workspace_root())
}

fn workspace_root() -> PathBuf {
    std::env::var_os("NEXTEST_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_WORKSPACE_DIR")))
}

fn neomacs_binary_path_from_override(
    workspace: &Path,
    override_path: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(path) = override_path
        && !path.as_os_str().is_empty()
    {
        return PathBuf::from(path);
    }

    workspace.join("target").join("release").join("neomacs")
}

impl Drop for TuiSession {
    fn drop(&mut self) {
        // Best-effort kill
        let _ = self._child.kill();
        let _ = self._child.wait();
    }
}

// ── Key translation ──────────────────────────────────────────────────

/// Translate an Emacs-style key name to the bytes a terminal sends.
///
/// Supports: `C-x`, `M-x`, `C-M-x`, `RET`, `TAB`, `ESC`, `SPC`,
/// `DEL`, and plain characters.
pub fn emacs_key(key: &str) -> Vec<u8> {
    match key {
        "RET" | "Enter" => return vec![b'\r'],
        "TAB" => return vec![b'\t'],
        "ESC" => return vec![0x1b],
        "SPC" => return vec![b' '],
        "C-SPC" | "C-@" => return vec![0x00],
        "C-M-SPC" | "C-M-@" => return vec![0x1b, 0x00],
        "C-/" | "C-_" => return vec![0x1f],
        "C-M-/" | "C-M-_" => return vec![0x1b, 0x1f],
        "DEL" => return vec![0x7f],
        "BS" => return vec![0x08],
        "F10" | "f10" => return vec![0x1b, b'[', b'2', b'1', b'~'],
        "UP" | "<up>" => return vec![0x1b, b'[', b'A'],
        "DOWN" | "<down>" => return vec![0x1b, b'[', b'B'],
        "RIGHT" | "<right>" => return vec![0x1b, b'[', b'C'],
        "LEFT" | "<left>" => return vec![0x1b, b'[', b'D'],
        _ => {}
    }

    // C-M-x  →  ESC + Ctrl(x)
    if let Some(ch) = key.strip_prefix("C-M-").and_then(|s| s.chars().next()) {
        let ctrl = (ch.to_ascii_lowercase() as u8)
            .wrapping_sub(b'a')
            .wrapping_add(1);
        return vec![0x1b, ctrl];
    }
    // C-x  →  Ctrl(x)
    if let Some(ch) = key.strip_prefix("C-").and_then(|s| s.chars().next()) {
        if ch == '@' {
            return vec![0x00];
        }
        let ctrl = (ch.to_ascii_lowercase() as u8)
            .wrapping_sub(b'a')
            .wrapping_add(1);
        return vec![ctrl];
    }
    // M-x  →  ESC x
    if let Some(ch) = key.strip_prefix("M-").and_then(|s| s.chars().next()) {
        return vec![0x1b, ch as u8];
    }

    // Plain character or multi-byte
    key.as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::{TuiLaunch, TuiSession, emacs_key, neomacs_binary_path_from_override};
    use std::ffi::OsString;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    #[test]
    fn structured_launch_preserves_spaces_in_arguments_and_environment() {
        let launch = TuiLaunch::new("sh")
            .args(["-c", "printf '%s' \"$NEOMACS_TUI_STRUCTURED_VALUE\""])
            .env("NEOMACS_TUI_STRUCTURED_VALUE", "alpha beta");
        let mut session = TuiSession::spawn_launch(launch, "STRUCTURED");

        session.read_until(Duration::from_secs(2), |grid| {
            grid.iter().any(|row| row.contains("alpha beta"))
        });

        assert!(
            session
                .text_grid()
                .iter()
                .any(|row| row.contains("alpha beta"))
        );
    }

    #[test]
    fn structured_launch_never_deletes_a_caller_owned_home() {
        let external_home = tempfile::tempdir().expect("create caller-owned HOME");
        let sentinel = external_home.path().join("keep-me");
        std::fs::write(&sentinel, "owned by caller").expect("write HOME sentinel");
        let launch = TuiLaunch::new("sh")
            .args(["-c", "printf done"])
            .env("HOME", external_home.path().as_os_str());

        let mut session = TuiSession::spawn_launch(launch, "EXTERNAL-HOME");
        session.read(Duration::from_secs(1));
        drop(session);

        assert!(sentinel.is_file(), "TUI session deleted caller-owned HOME");
    }

    #[test]
    fn structured_launch_never_deletes_a_caller_owned_tmpdir() {
        let external_tmp = tempfile::tempdir().expect("create caller-owned TMPDIR");
        let sentinel = external_tmp.path().join("keep-me");
        std::fs::write(&sentinel, "owned by caller").expect("write TMPDIR sentinel");
        let launch = TuiLaunch::new("sh")
            .args(["-c", "printf done"])
            .env("TMPDIR", external_tmp.path().as_os_str());

        let mut session = TuiSession::spawn_launch(launch, "EXTERNAL-TMPDIR");
        session.read(Duration::from_secs(1));
        drop(session);

        assert!(
            sentinel.is_file(),
            "TUI session deleted caller-owned TMPDIR"
        );
    }

    #[test]
    fn structured_launch_removes_harness_owned_directories() {
        let mut session = TuiSession::spawn_launch(
            TuiLaunch::new("sh").args(["-c", "printf done"]),
            "OWNED-DIRECTORIES",
        );
        session.read(Duration::from_secs(1));
        let home = session.home.path().to_path_buf();
        let tmp = session._tmp.path().to_path_buf();

        drop(session);

        assert!(!home.exists(), "harness-owned HOME survived session drop");
        assert!(!tmp.exists(), "harness-owned TMPDIR survived session drop");
    }

    #[test]
    fn neomacs_binary_path_prefers_explicit_override() {
        let workspace = Path::new("/repo");
        let path = neomacs_binary_path_from_override(
            workspace,
            Some(OsString::from("/tmp/custom-neomacs")),
        );

        assert_eq!(path, PathBuf::from("/tmp/custom-neomacs"));
    }

    #[test]
    fn neomacs_binary_path_defaults_to_release_binary() {
        let workspace = Path::new("/repo");
        let path = neomacs_binary_path_from_override(workspace, None);

        assert_eq!(
            path,
            PathBuf::from("/repo")
                .join("target")
                .join("release")
                .join("neomacs")
        );
    }

    #[test]
    fn emacs_key_maps_control_space_to_terminal_nul() {
        assert_eq!(emacs_key("C-SPC"), vec![0x00]);
        assert_eq!(emacs_key("C-@"), vec![0x00]);
        assert_eq!(emacs_key("C-M-SPC"), vec![0x1b, 0x00]);
        assert_eq!(emacs_key("C-M-@"), vec![0x1b, 0x00]);
        assert_eq!(emacs_key("C-/"), vec![0x1f]);
        assert_eq!(emacs_key("C-_"), vec![0x1f]);
        assert_eq!(emacs_key("C-M-/"), vec![0x1b, 0x1f]);
        assert_eq!(emacs_key("C-M-_"), vec![0x1b, 0x1f]);
    }

    #[test]
    fn emacs_key_maps_f10_to_screen_terminfo_sequence() {
        assert_eq!(emacs_key("F10"), b"\x1b[21~".to_vec());
        assert_eq!(emacs_key("f10"), b"\x1b[21~".to_vec());
    }

    #[test]
    fn emacs_key_maps_arrow_keys_to_cursor_sequences() {
        assert_eq!(emacs_key("UP"), b"\x1b[A".to_vec());
        assert_eq!(emacs_key("DOWN"), b"\x1b[B".to_vec());
        assert_eq!(emacs_key("RIGHT"), b"\x1b[C".to_vec());
        assert_eq!(emacs_key("LEFT"), b"\x1b[D".to_vec());
    }

    #[test]
    fn vt100_parser_does_not_render_decscusr_cursor_shape_as_text() {
        let mut parser = vt100::Parser::new(2, 40, 0);
        parser.process(b"\x1b[1;1HList lines matching regexp: \x1b[6 q\x1b[?25h");

        let row = parser.screen().contents_between(0, 0, 0, 40);
        let trimmed = row.trim_end();
        if trimmed != "List lines matching regexp:" {
            let mut bytes = String::new();
            for byte in b"\x1b[1;1HList lines matching regexp: \x1b[6 q\x1b[?25h" {
                let _ = write!(&mut bytes, "{byte:02x} ");
            }
            panic!("unexpected row {trimmed:?} for bytes [{bytes}]");
        }
    }
}

// ── Screen diffing ───────────────────────────────────────────────────

/// Exact attributes currently active for newly drawn terminal cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawTerminalAttributes {
    pub foreground: vt100::Color,
    pub background: vt100::Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl RawTerminalAttributes {
    fn from_screen(screen: &vt100::Screen) -> Self {
        Self {
            foreground: screen.fgcolor(),
            background: screen.bgcolor(),
            bold: screen.bold(),
            dim: screen.dim(),
            italic: screen.italic(),
            underline: screen.underline(),
            inverse: screen.inverse(),
        }
    }

    fn from_cell(cell: &vt100::Cell) -> Self {
        Self {
            foreground: cell.fgcolor(),
            background: cell.bgcolor(),
            bold: cell.bold(),
            dim: cell.dim(),
            italic: cell.italic(),
            underline: cell.underline(),
            inverse: cell.inverse(),
        }
    }

    fn default_cell() -> Self {
        Self {
            foreground: vt100::Color::Default,
            background: vt100::Color::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }

    fn write_canonical_sgr(self, output: &mut String) {
        let mut codes = vec!["0".to_string()];
        if self.bold {
            codes.push("1".to_string());
        }
        if self.dim {
            codes.push("2".to_string());
        }
        if self.italic {
            codes.push("3".to_string());
        }
        if self.underline {
            codes.push("4".to_string());
        }
        if self.inverse {
            codes.push("7".to_string());
        }
        append_color_codes(&mut codes, self.foreground, 38);
        append_color_codes(&mut codes, self.background, 48);
        output.push_str("\x1b[");
        output.push_str(&codes.join(";"));
        output.push('m');
    }
}

fn append_color_codes(codes: &mut Vec<String>, color: vt100::Color, prefix: u8) {
    match color {
        vt100::Color::Default => {}
        vt100::Color::Idx(index) => {
            codes.push(prefix.to_string());
            codes.push("5".to_string());
            codes.push(index.to_string());
        }
        vt100::Color::Rgb(red, green, blue) => {
            codes.push(prefix.to_string());
            codes.push("2".to_string());
            codes.push(red.to_string());
            codes.push(green.to_string());
            codes.push(blue.to_string());
        }
    }
}

/// One absolute terminal row in an exact raw-state capture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawTerminalRow {
    pub row: u16,
    pub wrapped: bool,
    pub cells: Vec<vt100::Cell>,
}

/// Exact observable terminal state for a selected range of absolute rows.
///
/// Equality is deliberately stricter than the older grid comparators: it
/// preserves empty versus written-space cells, exact colors and attributes,
/// wide-cell flags, row wrapping, cursor state, dimensions, and all terminal
/// modes exposed by `vt100`. The ANSI and plain grids are review projections;
/// equality of this raw structure remains the parity authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawTerminalSnapshot {
    pub terminal_size: (u16, u16),
    pub captured_rows: std::ops::Range<u16>,
    pub scrollback: usize,
    pub cursor_position: (u16, u16),
    pub alternate_screen: bool,
    pub application_keypad: bool,
    pub application_cursor: bool,
    pub cursor_hidden: bool,
    pub bracketed_paste: bool,
    pub mouse_protocol_mode: vt100::MouseProtocolMode,
    pub mouse_protocol_encoding: vt100::MouseProtocolEncoding,
    pub active_attributes: RawTerminalAttributes,
    pub rows: Vec<RawTerminalRow>,
}

impl RawTerminalSnapshot {
    /// Capture every physical cell in the terminal, without normalization.
    #[must_use]
    pub fn capture_full_screen(screen: &vt100::Screen) -> Self {
        Self::capture_rows(screen, 0..screen.size().0)
    }

    /// Capture every physical cell in `rows`, without normalization.
    #[must_use]
    pub fn capture_rows(screen: &vt100::Screen, rows: std::ops::Range<u16>) -> Self {
        let terminal_size = screen.size();
        assert!(
            rows.start <= rows.end && rows.end <= terminal_size.0,
            "captured row range {rows:?} is outside terminal height {}",
            terminal_size.0,
        );

        let captured = rows
            .clone()
            .map(|row| RawTerminalRow {
                row,
                wrapped: screen.row_wrapped(row),
                cells: (0..terminal_size.1)
                    .map(|col| {
                        screen
                            .cell(row, col)
                            .unwrap_or_else(|| panic!("terminal cell ({row}, {col}) is absent"))
                            .clone()
                    })
                    .collect(),
            })
            .collect();

        Self {
            terminal_size,
            captured_rows: rows,
            scrollback: screen.scrollback(),
            cursor_position: screen.cursor_position(),
            alternate_screen: screen.alternate_screen(),
            application_keypad: screen.application_keypad(),
            application_cursor: screen.application_cursor(),
            cursor_hidden: screen.hide_cursor(),
            bracketed_paste: screen.bracketed_paste(),
            mouse_protocol_mode: screen.mouse_protocol_mode(),
            mouse_protocol_encoding: screen.mouse_protocol_encoding(),
            active_attributes: RawTerminalAttributes::from_screen(screen),
            rows: captured,
        }
    }

    /// Canonically re-encode the captured cells with ANSI SGR styling.
    ///
    /// This is intentionally derived from terminal state, not copied from the
    /// editor's original byte stream, because different command sequences can
    /// create the same exact terminal cells.
    #[must_use]
    pub fn ansi_grid(&self) -> String {
        let mut output = String::new();
        let default = RawTerminalAttributes::default_cell();

        for row in &self.rows {
            let mut active = default;
            for cell in &row.cells {
                if cell.is_wide_continuation() {
                    continue;
                }
                let attributes = RawTerminalAttributes::from_cell(cell);
                if attributes != active {
                    attributes.write_canonical_sgr(&mut output);
                    active = attributes;
                }
                if cell.has_contents() {
                    output.push_str(cell.contents());
                } else {
                    output.push(' ');
                }
            }
            output.push_str("\x1b[0m\n");
        }

        output
    }

    /// Render a control-free, fixed-cell view of the captured rows.
    ///
    /// `∅` is an unwritten cell, `␠` is a written space, and `›` is a
    /// wide-character continuation cell. These visible markers keep the view
    /// readable without normalizing terminal cells that compare differently.
    #[must_use]
    pub fn plain_grid(&self) -> String {
        let label_width = usize::max(2, self.terminal_size.0.saturating_sub(1).to_string().len());
        let mut output = String::new();

        for row in &self.rows {
            write_plain_row(&mut output, row, label_width);
        }

        output
    }

    fn differing_plain_rows(&self, neomacs: &Self) -> String {
        let label_width = usize::max(2, self.terminal_size.0.saturating_sub(1).to_string().len());
        let mut output = String::new();

        for (gnu_row, neo_row) in self.rows.iter().zip(&neomacs.rows) {
            if gnu_row == neo_row {
                continue;
            }

            output.push_str("GNU     ");
            write_plain_row(&mut output, gnu_row, label_width);
            output.push_str("Neomacs ");
            write_plain_row(&mut output, neo_row, label_width);
        }

        output
    }

    /// List every exact-state difference from the GNU snapshot to Neomacs.
    ///
    /// Consecutive cells with the same pair of states are reported as a
    /// coordinate range. This only compacts the diagnostic; no mismatch is
    /// ignored or treated as equal.
    #[must_use]
    pub fn exact_differences(&self, neomacs: &Self) -> Vec<String> {
        let mut differences = Vec::new();

        macro_rules! compare_field {
            ($field:ident) => {
                if self.$field != neomacs.$field {
                    differences.push(format!(
                        "{}: GNU {:?} | Neomacs {:?}",
                        stringify!($field),
                        self.$field,
                        neomacs.$field,
                    ));
                }
            };
        }

        compare_field!(terminal_size);
        compare_field!(captured_rows);
        compare_field!(scrollback);
        compare_field!(cursor_position);
        compare_field!(alternate_screen);
        compare_field!(application_keypad);
        compare_field!(application_cursor);
        compare_field!(cursor_hidden);
        compare_field!(bracketed_paste);
        compare_field!(mouse_protocol_mode);
        compare_field!(mouse_protocol_encoding);
        compare_field!(active_attributes);

        if self.rows.len() != neomacs.rows.len() {
            differences.push(format!(
                "row count: GNU {} | Neomacs {}",
                self.rows.len(),
                neomacs.rows.len(),
            ));
        }

        for (gnu_row, neo_row) in self.rows.iter().zip(&neomacs.rows) {
            if gnu_row.row != neo_row.row {
                differences.push(format!(
                    "row index: GNU {} | Neomacs {}",
                    gnu_row.row, neo_row.row,
                ));
            }
            if gnu_row.wrapped != neo_row.wrapped {
                differences.push(format!(
                    "row {} wrapped: GNU {} | Neomacs {}",
                    gnu_row.row, gnu_row.wrapped, neo_row.wrapped,
                ));
            }
            if gnu_row.cells.len() != neo_row.cells.len() {
                differences.push(format!(
                    "row {} cell count: GNU {} | Neomacs {}",
                    gnu_row.row,
                    gnu_row.cells.len(),
                    neo_row.cells.len(),
                ));
            }

            let mut col = 0;
            let common_cells = usize::min(gnu_row.cells.len(), neo_row.cells.len());
            while col < common_cells {
                if gnu_row.cells[col] == neo_row.cells[col] {
                    col += 1;
                    continue;
                }

                let start = col;
                let gnu_description = raw_cell_description(&gnu_row.cells[col]);
                let neo_description = raw_cell_description(&neo_row.cells[col]);
                col += 1;
                while col < common_cells
                    && gnu_row.cells[col] != neo_row.cells[col]
                    && raw_cell_description(&gnu_row.cells[col]) == gnu_description
                    && raw_cell_description(&neo_row.cells[col]) == neo_description
                {
                    col += 1;
                }

                let coordinate = if col == start + 1 {
                    format!("col {start}")
                } else {
                    format!("cols {start}..={}", col - 1)
                };
                differences.push(format!(
                    "row {} {coordinate}: GNU {gnu_description} | Neomacs {neo_description}",
                    gnu_row.row,
                ));
            }
        }

        differences
    }
}

fn write_plain_row(output: &mut String, row: &RawTerminalRow, label_width: usize) {
    output.push_str(&format!("{:>label_width$} |", row.row));
    for cell in &row.cells {
        if cell.is_wide_continuation() {
            output.push('›');
        } else if !cell.has_contents() {
            output.push('∅');
        } else if cell.contents() == " " {
            output.push('␠');
        } else {
            output.push_str(cell.contents());
        }
    }
    output.push('|');
    if row.wrapped {
        output.push_str(" ↩");
    }
    output.push('\n');
}

fn raw_cell_description(cell: &vt100::Cell) -> String {
    let mut attributes = Vec::new();
    if cell.bold() {
        attributes.push("bold");
    }
    if cell.dim() {
        attributes.push("dim");
    }
    if cell.italic() {
        attributes.push("italic");
    }
    if cell.underline() {
        attributes.push("underline");
    }
    if cell.inverse() {
        attributes.push("inverse");
    }

    format!(
        "contents={:?} fg={:?} bg={:?} attrs=[{}] wide={} continuation={}",
        cell.contents(),
        cell.fgcolor(),
        cell.bgcolor(),
        attributes.join(","),
        cell.is_wide(),
        cell.is_wide_continuation(),
    )
}

/// Assert exact raw terminal-state parity and report every mismatched range.
pub fn assert_raw_terminal_snapshots_eq(
    label: &str,
    gnu: &RawTerminalSnapshot,
    neomacs: &RawTerminalSnapshot,
) {
    let differences = gnu.exact_differences(neomacs);
    assert!(
        differences.is_empty(),
        "{label}: {} exact terminal-state difference(s):\n\
         Differing plain rows (comparison remains full-screen and exact):\n{}\
         Exact differences:\n{}",
        differences.len(),
        gnu.differing_plain_rows(neomacs),
        differences.join("\n"),
    );
}

/// A single cell difference between two screens.
#[derive(Debug)]
pub struct CellDiff {
    pub row: u16,
    pub col: u16,
    pub gnu_char: String,
    pub neo_char: String,
    pub gnu_fg: vt100::Color,
    pub neo_fg: vt100::Color,
    pub gnu_bg: vt100::Color,
    pub neo_bg: vt100::Color,
    pub kind: DiffKind,
}

#[derive(Debug, PartialEq)]
pub enum DiffKind {
    Char,
    Color,
    Both,
}

/// Face-parity comparison: return diffs for cells whose CHARACTERS already
/// match but whose colors differ, restricted to a row/column window.
///
/// Char-differing cells are skipped on purpose -- text parity is asserted by
/// the text-grid comparisons, and the mode line legitimately differs in
/// product name, which would otherwise drown the color signal. What remains
/// is pure face divergence: same glyph, different paint.
pub fn color_diffs_in(
    gnu: &vt100::Screen,
    neo: &vt100::Screen,
    rows: std::ops::Range<u16>,
    cols: std::ops::Range<u16>,
) -> Vec<CellDiff> {
    diff_screens(gnu, neo)
        .into_iter()
        .filter(|d| d.kind == DiffKind::Color && rows.contains(&d.row) && cols.contains(&d.col))
        .collect()
}

/// Render a compact human-readable report of color diffs for a panic message.
pub fn format_color_diffs(diffs: &[CellDiff], limit: usize) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for d in diffs.iter().take(limit) {
        let _ = writeln!(
            &mut out,
            "  ({:>2},{:>3}) {:?}: gnu fg={:?} bg={:?} | neo fg={:?} bg={:?}",
            d.row, d.col, d.gnu_char, d.gnu_fg, d.gnu_bg, d.neo_fg, d.neo_bg
        );
    }
    if diffs.len() > limit {
        let _ = writeln!(&mut out, "  ... and {} more", diffs.len() - limit);
    }
    out
}

/// Compare two screens cell by cell, returning all differences.
pub fn diff_screens(gnu: &vt100::Screen, neo: &vt100::Screen) -> Vec<CellDiff> {
    let mut diffs = Vec::new();
    for row in 0..ROWS {
        for col in 0..COLS {
            let gc = gnu.cell(row, col);
            let nc = neo.cell(row, col);
            let (gc, nc) = match (gc, nc) {
                (Some(g), Some(n)) => (g, n),
                _ => continue,
            };

            let char_diff = gc.contents() != nc.contents();
            let color_diff = gc.fgcolor() != nc.fgcolor() || gc.bgcolor() != nc.bgcolor();

            if char_diff || color_diff {
                diffs.push(CellDiff {
                    row,
                    col,
                    gnu_char: gc.contents().to_string(),
                    neo_char: nc.contents().to_string(),
                    gnu_fg: gc.fgcolor(),
                    neo_fg: nc.fgcolor(),
                    gnu_bg: gc.bgcolor(),
                    neo_bg: nc.bgcolor(),
                    kind: match (char_diff, color_diff) {
                        (true, true) => DiffKind::Both,
                        (true, false) => DiffKind::Char,
                        (false, true) => DiffKind::Color,
                        _ => unreachable!(),
                    },
                });
            }
        }
    }
    diffs
}

/// A row-level text difference.
#[derive(Debug)]
pub struct RowDiff {
    pub row: usize,
    pub gnu: String,
    pub neo: String,
}

/// Compare two text grids, normalising known product-name differences.
///
/// Returns only rows where meaningful differences remain after
/// replacing "GNU Emacs" ↔ "Neomacs" and stripping trailing whitespace.
pub fn diff_text_grids(gnu: &[String], neo: &[String]) -> Vec<RowDiff> {
    let mut diffs = Vec::new();
    let norm = |s: &str| -> String {
        s.replace("GNU Emacs", "EDITOR__")
            .replace("*GNU Emacs*", "*EDITOR__*")
            .replace("Neomacs", "EDITOR__")
            .replace("*Neomacs*", "*EDITOR__*")
            .trim_end()
            .to_string()
    };
    for (i, (g, n)) in gnu.iter().zip(neo.iter()).enumerate() {
        if norm(g) != norm(n) {
            diffs.push(RowDiff {
                row: i,
                gnu: g.trim_end().to_string(),
                neo: n.trim_end().to_string(),
            });
        }
    }
    diffs
}

/// Check whether a row difference is just boot-screen informational text
/// that we expect to differ (welcome message, copyright, etc.).
pub fn is_boot_info_row(gnu_text: &str, neo_text: &str) -> bool {
    let patterns = [
        "information about GNU",
        "Welcome to GNU",
        "tutorial",
        "Copyright",
        "Free Software",
        "warranty",
        "C-h C-a",
        "Appl",
    ];
    for p in &patterns {
        if gnu_text.contains(p) || neo_text.contains(p) {
            return true;
        }
    }
    false
}

// ── Strict contract-level grid comparison ────────────────────────────
//
// `diff_screens` above compares *raw* cells (char + exact terminal colour).
// Raw-colour equality is the WRONG strictness for the Neomacs↔GNU contract:
// the two editors map faces to terminal colours through different palettes/
// themes, so identical raw SGR is not required (Neomacs is free to differ
// "below the contract"). The contract is the *logical display*: the exact
// character grid, and *which logical face* applies to each cell.
//
// `compare_grids_strict` therefore compares:
//   1. the exact character at each cell (logical layout), and
//   2. face *identity* as a palette-independent CLASS PARTITION — two cells
//      share a class iff they share `(fg, bg)`, and the two grids must induce
//      the *same partition* over the compared cells. GNU using red where
//      Neomacs uses green is fine; GNU colouring two runs the same while
//      Neomacs colours them differently is a divergence.
// minus an explicit, shrinking allow-list of known parity gaps, with chrome /
// non-deterministic rows masked.

/// A cell allowed to diverge from GNU, with a human-readable reason. The
/// allow-list is the *visible, shrinking* parity backlog: a green run means
/// "no new divergence", not "no divergence".
#[derive(Debug, Clone)]
pub struct ExpectedDivergence {
    pub row: u16,
    pub col: u16,
    pub reason: &'static str,
}

/// Options for [`compare_grids_strict`] / [`assert_grids_strict`].
pub struct StrictGridOptions {
    /// Rows skipped entirely (chrome: mode-line, header-line, echo area, and
    /// any non-deterministic region).
    pub masked_rows: Vec<u16>,
    /// Restrict comparison to this row window (e.g. the text area). `None`
    /// compares every unmasked row.
    pub row_range: Option<std::ops::Range<u16>>,
    /// Compare face identity (the class partition) in addition to characters.
    pub compare_faces: bool,
    /// Cells permitted to differ.
    pub allow: Vec<ExpectedDivergence>,
}

impl Default for StrictGridOptions {
    fn default() -> Self {
        Self {
            masked_rows: Vec::new(),
            row_range: None,
            compare_faces: true,
            allow: Vec::new(),
        }
    }
}

/// What kind of strict (contract-level) divergence a cell exhibits.
#[derive(Debug, PartialEq, Eq)]
pub enum StrictDiffKind {
    /// Different character — a logical-layout violation.
    Char,
    /// Different face *identity* — the cell's colour-class partition differs
    /// from GNU's. Palette-independent: NOT a raw-colour comparison.
    FaceClass,
}

/// One unexpected (non-allow-listed) divergence.
#[derive(Debug)]
pub struct StrictDiff {
    pub row: u16,
    pub col: u16,
    pub kind: StrictDiffKind,
    pub gnu_char: String,
    pub neo_char: String,
}

/// Compare two screens strictly on the contract axis (exact chars + face-class
/// partition) over the unmasked text area, minus the allow-list. Returns the
/// unexpected divergences (empty == strict match).
pub fn compare_grids_strict(
    gnu: &vt100::Screen,
    neo: &vt100::Screen,
    opts: &StrictGridOptions,
) -> Vec<StrictDiff> {
    let is_allowed = |r: u16, c: u16| opts.allow.iter().any(|a| a.row == r && a.col == c);
    let in_range = |r: u16| {
        opts.row_range
            .as_ref()
            .is_none_or(|range| range.contains(&r))
    };

    // Per-grid canonical face-class labels, assigned by first appearance in a
    // fixed row-major traversal. Same traversal on both grids => labels agree
    // iff the partitions agree (palette-independent).
    let mut gnu_class: std::collections::HashMap<(String, String), usize> = Default::default();
    let mut neo_class: std::collections::HashMap<(String, String), usize> = Default::default();
    let mut diffs = Vec::new();

    for row in 0..ROWS {
        if opts.masked_rows.contains(&row) || !in_range(row) {
            continue;
        }
        for col in 0..COLS {
            let (gc, nc) = match (gnu.cell(row, col), neo.cell(row, col)) {
                (Some(g), Some(n)) => (g, n),
                _ => continue,
            };
            if is_allowed(row, col) {
                continue;
            }

            // Normalise blank-cell representation: GNU leaves trailing/blank
            // cells *unwritten* (vt100 returns ""), while Neomacs writes explicit
            // spaces. Both mean "blank", so treat "" and " " as equal — this is a
            // terminal-output optimisation, not a logical-display difference.
            let g_char = if gc.contents().is_empty() {
                " "
            } else {
                gc.contents()
            };
            let n_char = if nc.contents().is_empty() {
                " "
            } else {
                nc.contents()
            };
            if g_char != n_char {
                diffs.push(StrictDiff {
                    row,
                    col,
                    kind: StrictDiffKind::Char,
                    gnu_char: g_char.to_string(),
                    neo_char: n_char.to_string(),
                });
            }

            if opts.compare_faces {
                // Label classes by first appearance, but ANCHOR the
                // default/background class (Default fg + Default bg) to 0 in both
                // grids. Otherwise a single diverging colour shifts every later
                // label and makes blank runs cascade into thousands of false
                // diffs. With the anchor, only the genuinely re-partitioned
                // (coloured) cells are reported.
                let label = |classes: &mut std::collections::HashMap<(String, String), usize>,
                             fg: vt100::Color,
                             bg: vt100::Color|
                 -> usize {
                    let key = (format!("{fg:?}"), format!("{bg:?}"));
                    if key.0 == "Default" && key.1 == "Default" {
                        return 0;
                    }
                    let next = classes.len() + 1;
                    *classes.entry(key).or_insert(next)
                };
                let g_label = label(&mut gnu_class, gc.fgcolor(), gc.bgcolor());
                let n_label = label(&mut neo_class, nc.fgcolor(), nc.bgcolor());
                if g_label != n_label {
                    diffs.push(StrictDiff {
                        row,
                        col,
                        kind: StrictDiffKind::FaceClass,
                        gnu_char: gc.contents().to_string(),
                        neo_char: nc.contents().to_string(),
                    });
                }
            }
        }
    }
    diffs
}

/// Assert a strict (contract-level) grid match; panics with a readable dump of
/// the first divergences otherwise.
pub fn assert_grids_strict(
    label: &str,
    gnu: &vt100::Screen,
    neo: &vt100::Screen,
    opts: &StrictGridOptions,
) {
    let diffs = compare_grids_strict(gnu, neo, opts);
    if diffs.is_empty() {
        return;
    }
    use std::fmt::Write as _;
    let n_char = diffs
        .iter()
        .filter(|d| d.kind == StrictDiffKind::Char)
        .count();
    let n_face = diffs.len() - n_char;
    let mut msg = format!(
        "{label}: {} unexpected strict divergence(s) vs GNU ({n_char} char, {n_face} face-class):\n",
        diffs.len()
    );
    for d in diffs.iter().take(40) {
        let _ = writeln!(
            msg,
            "  ({:>2},{:>3}) {:?}: GNU {:?} / NEO {:?}",
            d.row, d.col, d.kind, d.gnu_char, d.neo_char
        );
    }
    if diffs.len() > 40 {
        let _ = writeln!(msg, "  … and {} more", diffs.len() - 40);
    }
    panic!("{msg}");
}

/// Pretty-print row diffs to stderr (useful in test assertions).
pub fn print_row_diffs(diffs: &[RowDiff]) {
    for d in diffs {
        eprintln!("  row {:2}:", d.row);
        eprintln!("    GNU: |{}|", d.gnu);
        eprintln!("    NEO: |{}|", d.neo);
    }
}

#[cfg(test)]
mod strict_grid_tests {
    use super::*;

    /// Render SGR-coloured `bytes` onto a full-size screen, starting at home.
    fn screen(bytes: &[u8]) -> vt100::Parser {
        let mut p = vt100::Parser::new(ROWS, COLS, 0);
        p.process(b"\x1b[H");
        p.process(bytes);
        p
    }

    #[test]
    fn face_class_is_palette_independent() {
        // Same partition {col0,col1},{col2}; different palettes (red/blue vs green/yellow).
        let gnu = screen(b"\x1b[31mAB\x1b[34mC\x1b[0m");
        let neo = screen(b"\x1b[32mAB\x1b[33mC\x1b[0m");
        let diffs = compare_grids_strict(gnu.screen(), neo.screen(), &StrictGridOptions::default());
        assert!(
            diffs.is_empty(),
            "same partition with a different palette must match; got {diffs:?}"
        );
    }

    #[test]
    fn face_class_catches_repartition_without_cascading() {
        // GNU separates C (blue) from A,B (red); NEO colours all three the same.
        let gnu = screen(b"\x1b[31mAB\x1b[34mC\x1b[0m");
        let neo = screen(b"\x1b[32mABC\x1b[0m");
        let diffs = compare_grids_strict(gnu.screen(), neo.screen(), &StrictGridOptions::default());
        // Exactly the C cell diverges — the (anchored) default/blank cells do NOT cascade.
        assert_eq!(
            diffs.len(),
            1,
            "only the repartitioned cell should diverge; got {diffs:?}"
        );
        assert_eq!(diffs[0].kind, StrictDiffKind::FaceClass);
        assert_eq!((diffs[0].row, diffs[0].col), (0, 2));
    }

    #[test]
    fn char_diff_caught_and_allowlist_suppresses() {
        let gnu = screen(b"hello");
        let neo = screen(b"hellX"); // differ at col 4
        let plain = compare_grids_strict(gnu.screen(), neo.screen(), &StrictGridOptions::default());
        let chars: Vec<_> = plain
            .iter()
            .filter(|d| d.kind == StrictDiffKind::Char)
            .collect();
        assert_eq!(chars.len(), 1, "one char diff expected; got {plain:?}");
        assert_eq!((chars[0].row, chars[0].col), (0, 4));

        // Allow-listing that cell makes the comparison clean.
        let opts = StrictGridOptions {
            allow: vec![ExpectedDivergence {
                row: 0,
                col: 4,
                reason: "intentional test divergence",
            }],
            ..Default::default()
        };
        assert!(compare_grids_strict(gnu.screen(), neo.screen(), &opts).is_empty());
    }

    #[test]
    fn blank_cell_representation_is_normalised() {
        // GNU leaves trailing cells unwritten (""); NEO writes explicit spaces.
        let gnu = screen(b"AB"); // cols 2.. unwritten -> ""
        let mut neo = vt100::Parser::new(ROWS, COLS, 0);
        neo.process(b"\x1b[HAB");
        neo.process(b"\x1b[1;3H   "); // overwrite cols 2..5 with spaces
        let diffs = compare_grids_strict(gnu.screen(), neo.screen(), &StrictGridOptions::default());
        assert!(
            diffs.is_empty(),
            "\"\" and \" \" blanks must compare equal; got {diffs:?}"
        );
    }
}
