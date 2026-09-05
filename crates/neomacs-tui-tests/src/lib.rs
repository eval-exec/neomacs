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
//!   virtual screen. With `NEOMACS_TUI_RECORD=on`, each session also writes an
//!   asciicast v3 recording under `target/tui-recordings`.
//!
//! - [`emacs_key`] translates Emacs key descriptions (`"C-x"`, `"M-x"`,
//!   `"RET"`) into the raw bytes a terminal would send.
//!
//! - [`compare_displays`] compares the complete visible display through one
//!   exact contract: geometry, exact text, resolved RGB colors, full style
//!   classes, wrapping, and cursor state.
//!
//! - [`diff_screens`] remains available for tests that intentionally inspect
//!   raw terminal cells or exact palette values.

use std::ffi::OsString;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::{Duration, Instant};

mod launch;
mod pty_output;
mod recording;

pub use launch::TuiLaunch;
use pty_output::{PtyOutputEvent, PtyOutputPump};
pub use recording::TuiRecordingScope;
use recording::{RecordingIdentity, RecordingPolicy, SessionRecording, TerminalSize};

// ── Session ──────────────────────────────────────────────────────────

/// Default terminal size for tests.
pub const COLS: u16 = 160;
pub const ROWS: u16 = 50;

/// One `--eval` argv element that silences GNU's async native-comp chatter
/// (jit compilation, warning reports, and any compiler subprocess) so the
/// oracle screen stays focused on the behavior under test.
///
/// Every form here must evaluate cleanly on ANY GNU build. An error in the
/// startup `--eval` aborts GNU's startup sequence before the *scratch*
/// message is inserted, painting an empty buffer whose echo area holds the
/// error -- every pair comparison against that session then fails (CI's
/// `emacs-nox` 29.3, where `warning-suppress-types` is void because
/// warnings.el is not preloaded and comp-run.el -- which `(defvar
/// warning-suppress-types)` -- only loads in native-comp builds). Hence the
/// `boundp` guard; `set` on an unbound symbol is safe.
pub const QUIET_NATIVE_COMP_EVAL: &str = "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent)(when(boundp'warning-suppress-types)(push'(native-compiler)warning-suppress-types))(mapc'kill-process(process-list)))";

/// Initial terminal identity and geometry for a TUI process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiTerminalConfig {
    terminal_type: String,
    rows: u16,
    columns: u16,
}

impl TuiTerminalConfig {
    pub fn new(terminal_type: impl Into<String>, rows: u16, columns: u16) -> Self {
        assert!(rows != 0, "TUI terminal rows must be non-zero");
        assert!(columns != 0, "TUI terminal columns must be non-zero");
        Self {
            terminal_type: terminal_type.into(),
            rows,
            columns,
        }
    }
}

impl Default for TuiTerminalConfig {
    fn default() -> Self {
        Self::new("screen-256color", ROWS, COLS)
    }
}

/// Result of driving a TUI process until it exits or exhausts its budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiProcessOutcome {
    Exited,
    TimedOut,
}

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
    _owner: tempfile::TempDir,
    path: PathBuf,
}

impl TuiTempDirectory {
    /// Create an isolated fixture root with a recognizable name prefix.
    pub fn new(prefix: &str) -> Self {
        let owner = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("create TUI fixture temp directory");
        let path = owner.path().to_path_buf();
        Self {
            _owner: owner,
            path,
        }
    }

    /// Create an isolated fixture directory beneath a private owned parent.
    ///
    /// Use this when a program displays metadata for `..`: activity in the
    /// system temporary directory cannot then perturb the observable parent.
    pub fn new_with_private_parent(prefix: &str, directory_name: impl AsRef<Path>) -> Self {
        let owner = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("create TUI fixture parent directory");
        let path = owner.path().join(directory_name);
        std::fs::create_dir(&path).expect("create nested TUI fixture directory");
        Self {
            _owner: owner,
            path,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
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
    output_pump: PtyOutputPump,
    _child: std::process::Child,
    parser: vt100::Parser,
    recent_output: Vec<u8>,
    recording: SessionRecording,
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

    /// Spawn a structured process under a caller-provided recording scope.
    ///
    /// Package suites use this to group both editor recordings under the
    /// package scenario rather than under harness implementation details.
    pub fn spawn_launch_in_scope(launch: TuiLaunch, name: &str, scope: TuiRecordingScope) -> Self {
        Self::spawn_launch_with_scope_terminal_and_erase_char(
            launch,
            name,
            scope,
            TuiTerminalConfig::default(),
            PtyEraseChar::TerminalDefault,
        )
    }

    /// Spawn a structured process with an explicit terminal type and initial
    /// geometry.
    ///
    /// Direct terminal probes use this path so they retain the same process,
    /// lifecycle, and recording behavior as ordinary parity sessions.
    pub fn spawn_launch_on_terminal(
        launch: TuiLaunch,
        name: &str,
        terminal: TuiTerminalConfig,
    ) -> Self {
        Self::spawn_launch_with_scope_terminal_and_erase_char(
            launch,
            name,
            TuiRecordingScope::current(),
            terminal,
            PtyEraseChar::TerminalDefault,
        )
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
        Self::spawn_launch_with_scope_terminal_and_erase_char(
            launch,
            name,
            TuiRecordingScope::current(),
            TuiTerminalConfig::default(),
            erase,
        )
    }

    fn spawn_launch_with_scope_terminal_and_erase_char(
        launch: TuiLaunch,
        name: &str,
        scope: TuiRecordingScope,
        terminal: TuiTerminalConfig,
        erase: PtyEraseChar,
    ) -> Self {
        let policy = RecordingPolicy::parse(std::env::var_os(NEOMACS_TUI_RECORD).as_deref())
            .unwrap_or_else(|message| panic!("{message}"));
        let root = tui_recording_root();
        Self::spawn_launch_with_recording(
            launch,
            name,
            terminal,
            erase,
            policy,
            &root,
            scope.session(name),
        )
    }

    fn spawn_launch_with_recording(
        launch: TuiLaunch,
        name: &str,
        terminal: TuiTerminalConfig,
        erase: PtyEraseChar,
        recording_policy: RecordingPolicy,
        recording_root: &Path,
        recording_identity: RecordingIdentity,
    ) -> Self {
        let (pty, pts) = pty_process::blocking::open().expect("open pty");
        pty.resize(pty_process::Size::new(terminal.rows, terminal.columns))
            .expect("resize pty");
        set_pty_erase_char(&pts, erase);
        let recording = SessionRecording::start(
            recording_policy,
            recording_root,
            recording_identity,
            &terminal.terminal_type,
            TerminalSize::new(terminal.rows, terminal.columns),
        );

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
            .env("TERM", &terminal.terminal_type)
            .env("COLUMNS", terminal.columns.to_string())
            .env("LINES", terminal.rows.to_string())
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

        // Start draining before the child can emit its first byte. GNU makes
        // the shared slave description nonblocking while checking input, so
        // even startup output can otherwise overrun the PTY queue.
        let output_pump = PtyOutputPump::start(&pty, name).expect("start PTY output pump");
        let child = command.spawn(pts).expect("spawn");

        let parser = vt100::Parser::new(terminal.rows, terminal.columns, 0);

        TuiSession {
            pty,
            output_pump,
            _child: child,
            parser,
            recent_output: Vec::new(),
            recording,
            home,
            _tmp: tmp,
            name: name.to_string(),
        }
    }

    #[cfg(test)]
    fn spawn_launch_for_recording_test(
        launch: TuiLaunch,
        name: &str,
        terminal: TuiTerminalConfig,
        policy: RecordingPolicy,
        root: &Path,
        identity: RecordingIdentity,
    ) -> Self {
        Self::spawn_launch_with_recording(
            launch,
            name,
            terminal,
            PtyEraseChar::TerminalDefault,
            policy,
            root,
            identity,
        )
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
        let launch = TuiLaunch::new("emacs")
            .args(["-nw", "-Q", "-no-comp-spawn", QUIET_NATIVE_COMP_EVAL])
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
    /// `IDLE_CUTOFF` *after at least one byte has arrived*, or
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
        /// Each channel wait lasts at most this long before we re-check idle /
        /// max-deadline conditions.
        const RECEIVE_SLICE: Duration = Duration::from_millis(50);
        let max_deadline = Instant::now() + max_timeout;
        let mut last_activity: Option<Instant> = None;
        loop {
            self.recording.flush_if_due();

            // Drain everything the independent reader has already observed
            // before deciding that the child is idle.
            loop {
                match self.output_pump.try_recv() {
                    Ok(event) => match self.apply_output_event(event) {
                        PtyReadProgress::Activity(observed_at) => {
                            last_activity = Some(
                                last_activity.map_or(observed_at, |last| last.max(observed_at)),
                            );
                        }
                        PtyReadProgress::Finished => return,
                    },
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                }
            }

            let now = Instant::now();
            if now >= max_deadline {
                break;
            }
            if let Some(last) = last_activity
                && now.duration_since(last) >= IDLE_CUTOFF
            {
                break;
            }
            let wait = max_deadline
                .saturating_duration_since(now)
                .min(RECEIVE_SLICE);
            match self.output_pump.recv_timeout(wait) {
                Ok(event) => match self.apply_output_event(event) {
                    PtyReadProgress::Activity(observed_at) => {
                        last_activity =
                            Some(last_activity.map_or(observed_at, |last| last.max(observed_at)));
                    }
                    PtyReadProgress::Finished => return,
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn apply_output_event(&mut self, event: PtyOutputEvent) -> PtyReadProgress {
        match event {
            PtyOutputEvent::Data { observed_at, bytes } => {
                self.recording.output_at(observed_at, &bytes);
                self.recent_output.extend_from_slice(&bytes);
                if self.recent_output.len() > 262_144 {
                    let drain = self.recent_output.len() - 262_144;
                    self.recent_output.drain(..drain);
                }
                self.parser.process(&bytes);
                PtyReadProgress::Activity(observed_at)
            }
            PtyOutputEvent::Closed => PtyReadProgress::Finished,
            PtyOutputEvent::Failed(error) => {
                eprintln!("{} PTY output reader stopped: {error}", self.name);
                PtyReadProgress::Finished
            }
        }
    }

    fn drain_pending_output(&mut self) {
        loop {
            match self.output_pump.try_recv() {
                Ok(event) => {
                    if matches!(self.apply_output_event(event), PtyReadProgress::Finished) {
                        return;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty)
                | Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
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
        self.recording.input(data);
    }

    /// Paste text through the terminal's bracketed-paste protocol.
    ///
    /// This is intentionally distinct from [`Self::send`]: control bytes in
    /// pasted text are input data, while the same bytes sent as keystrokes can
    /// invoke commands.  Both editors enable bracketed paste during terminal
    /// startup, so requiring that mode here catches callers that paste before
    /// the terminal handshake has completed.
    pub fn paste(&mut self, text: &str) {
        assert!(
            self.screen().bracketed_paste(),
            "{} has not enabled terminal bracketed-paste mode",
            self.name
        );
        self.send(b"\x1b[200~");
        self.send(text.as_bytes());
        self.send(b"\x1b[201~");
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

    /// Drain the PTY while the child runs, stopping when it exits or when the
    /// supplied budget is exhausted.
    ///
    /// A timed-out child is terminated and reaped before this returns. Keeping
    /// reads and lifecycle management in the harness prevents a verbose child
    /// from deadlocking on a full PTY and ensures direct probes are recorded.
    pub fn run_to_completion(&mut self, max_timeout: Duration) -> TuiProcessOutcome {
        const READ_SLICE: Duration = Duration::from_millis(100);

        let deadline = Instant::now() + max_timeout;
        loop {
            if let Some(status) = self._child.try_wait().expect("wait on TUI process") {
                self.read(READ_SLICE);
                self.recording.finish(exit_status_code(status));
                return TuiProcessOutcome::Exited;
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = self._child.kill();
                let status = self._child.wait().ok();
                self.read(READ_SLICE);
                self.recording
                    .finish(status.map(exit_status_code).unwrap_or(1));
                return TuiProcessOutcome::TimedOut;
            }

            self.read(remaining.min(READ_SLICE));
        }
    }

    /// Resize the underlying PTY and the virtual terminal parser.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.pty
            .resize(pty_process::Size::new(rows, cols))
            .expect("resize pty");
        self.parser.screen_mut().set_size(rows, cols);
        self.recording.resize(TerminalSize::new(rows, cols));
    }

    /// Add a named navigation point to the terminal recording.
    pub fn mark_recording(&mut self, label: &str) {
        self.recording.marker(label);
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

    /// Clear the accumulated raw PTY output captured by [`Self::read`].
    pub fn clear_recent_output(&mut self) {
        self.recent_output.clear();
    }

    /// Borrow the recent raw PTY output captured by [`Self::read`].
    pub fn recent_output(&self) -> &[u8] {
        &self.recent_output
    }

    /// Return the asciicast artifact path when recording is enabled.
    pub fn recording_path(&self) -> Option<&Path> {
        self.recording.path()
    }

    /// Return the isolated HOME directory used for this session.
    pub fn home_dir(&self) -> &std::path::Path {
        self.home.path()
    }

    /// Return the isolated temporary directory used for this session.
    pub fn temp_dir(&self) -> &std::path::Path {
        self._tmp.path()
    }
}

const NEOMACS_TUI_NEOMACS_BIN: &str = "NEOMACS_TUI_NEOMACS_BIN";
const NEOMACS_TUI_RECORD: &str = "NEOMACS_TUI_RECORD";
const NEOMACS_TUI_RECORD_DIR: &str = "NEOMACS_TUI_RECORD_DIR";

fn tui_recording_root() -> PathBuf {
    let workspace = workspace_root();
    match std::env::var_os(NEOMACS_TUI_RECORD_DIR).map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        Some(path) => workspace.join(path),
        None => workspace.join("target/tui-recordings"),
    }
}

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

fn exit_status_code(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;

    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}

impl Drop for TuiSession {
    fn drop(&mut self) {
        let status = match self._child.try_wait() {
            Ok(Some(status)) => Some(status),
            _ => {
                let _ = self._child.kill();
                self._child.wait().ok()
            }
        };
        self.output_pump.shutdown();
        self.drain_pending_output();
        self.recording
            .finish(status.map(exit_status_code).unwrap_or(1));
    }
}

enum PtyReadProgress {
    Activity(Instant),
    Finished,
}

// ── Key translation ──────────────────────────────────────────────────

/// A named key whose terminal representation is independent of modifiers.
///
/// Keeping these names closed prevents a description such as `M-SPC` from
/// silently degrading to the first character of `SPC` (`ESC s`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalNamedKey {
    Return,
    Tab,
    Escape,
    Space,
    Delete,
    Backspace,
}

impl TerminalNamedKey {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "RET" | "Enter" => Some(Self::Return),
            "TAB" => Some(Self::Tab),
            "ESC" => Some(Self::Escape),
            "SPC" => Some(Self::Space),
            "DEL" => Some(Self::Delete),
            "BS" => Some(Self::Backspace),
            _ => None,
        }
    }

    const fn terminal_byte(self) -> u8 {
        match self {
            Self::Return => b'\r',
            Self::Tab => b'\t',
            Self::Escape => 0x1b,
            Self::Space => b' ',
            Self::Delete => 0x7f,
            Self::Backspace => 0x08,
        }
    }
}

/// The two encodings a terminal can use for a Control-modified character.
///
/// ASCII only assigns C0 bytes to a small, closed set of keys. Printable
/// punctuation outside that set must use xterm's `modifyOtherKeys` protocol;
/// applying the alphabetic control-byte formula to it silently wraps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalControlEncoding {
    C0(u8),
    ModifyOtherKeys(u8),
}

impl TerminalControlEncoding {
    fn for_ascii(character: char) -> Option<Self> {
        let character = character.to_ascii_lowercase();
        let byte = u8::try_from(character).ok()?;
        let encoding = match character {
            '@' => Self::C0(0),
            'a'..='z' => Self::C0(byte - b'a' + 1),
            '[' => Self::C0(0x1b),
            '\\' => Self::C0(0x1c),
            ']' => Self::C0(0x1d),
            '^' => Self::C0(0x1e),
            '_' => Self::C0(0x1f),
            ' '..='~' => Self::ModifyOtherKeys(byte),
            _ => return None,
        };
        Some(encoding)
    }

    fn terminal_bytes(self, meta: bool) -> Vec<u8> {
        match self {
            Self::C0(byte) => {
                if meta {
                    vec![0x1b, byte]
                } else {
                    vec![byte]
                }
            }
            Self::ModifyOtherKeys(byte) => {
                // GNU lisp/term/xterm.el installs this legacy form and CSI-u.
                // The modifier parameter is 5 for Control, 7 for Control+Meta.
                format!("\x1b[27;{};{byte}~", if meta { 7 } else { 5 }).into_bytes()
            }
        }
    }
}

fn single_character(name: &str) -> Option<char> {
    let mut characters = name.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}

/// Translate an Emacs-style key name to the bytes a terminal sends.
///
/// Supports: `C-x`, `M-x`, `C-M-x`, `RET`, `TAB`, `ESC`, `SPC`,
/// `DEL`, and plain characters.
pub fn emacs_key(key: &str) -> Vec<u8> {
    if let Some(named) = TerminalNamedKey::parse(key) {
        return vec![named.terminal_byte()];
    }

    match key {
        "C-SPC" | "C-@" => return vec![0x00],
        "C-M-SPC" | "C-M-@" => return vec![0x1b, 0x00],
        "C-/" | "C-_" => return vec![0x1f],
        "C-M-/" | "C-M-_" => return vec![0x1b, 0x1f],
        "F10" | "f10" => return vec![0x1b, b'[', b'2', b'1', b'~'],
        "UP" | "<up>" => return vec![0x1b, b'[', b'A'],
        "DOWN" | "<down>" => return vec![0x1b, b'[', b'B'],
        "RIGHT" | "<right>" => return vec![0x1b, b'[', b'C'],
        "LEFT" | "<left>" => return vec![0x1b, b'[', b'D'],
        _ => {}
    }

    // C-M-x  →  ESC + Ctrl(x)
    if let Some(encoding) = key
        .strip_prefix("C-M-")
        .and_then(single_character)
        .and_then(TerminalControlEncoding::for_ascii)
    {
        return encoding.terminal_bytes(true);
    }
    // C-x  →  Ctrl(x)
    if let Some(encoding) = key
        .strip_prefix("C-")
        .and_then(single_character)
        .and_then(TerminalControlEncoding::for_ascii)
    {
        return encoding.terminal_bytes(false);
    }
    // M-x  →  ESC x. Named keys must be decoded as a complete token;
    // taking only the first character made M-SPC indistinguishable from M-s.
    if let Some(base) = key.strip_prefix("M-") {
        if let Some(named) = TerminalNamedKey::parse(base) {
            return vec![0x1b, named.terminal_byte()];
        }
        if base.chars().count() == 1 {
            let mut encoded = vec![0x1b];
            encoded.extend_from_slice(base.as_bytes());
            return encoded;
        }
    }

    // Plain character or multi-byte
    key.as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::{
        TuiLaunch, TuiProcessOutcome, TuiSession, TuiTempDirectory, TuiTerminalConfig, emacs_key,
        neomacs_binary_path_from_override,
        recording::{RecordingIdentity, RecordingPolicy},
    };
    use std::ffi::OsString;
    use std::io::{Read as _, Write as _};

    #[test]
    fn private_parent_temp_directory_exposes_a_nested_owned_directory() {
        let directory = TuiTempDirectory::new_with_private_parent("tui-private-parent-", "listing");
        let exposed = directory.path().to_path_buf();
        let owner = exposed
            .parent()
            .expect("nested fixture directory should have a private parent")
            .to_path_buf();

        assert_eq!(
            exposed.file_name().and_then(|name| name.to_str()),
            Some("listing")
        );
        assert!(exposed.is_dir());
        assert!(owner.is_dir());

        drop(directory);

        assert!(!owner.exists(), "private parent survived fixture drop");
    }
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

    const NONBLOCKING_PTY_FIXTURE: &str = "NEOMACS_TUI_NONBLOCKING_PTY_FIXTURE";

    /// Model GNU's TTY descriptor setup: stdin/stdout are dup'd from one PTY
    /// slave open-file description, so setting O_NONBLOCK while polling stdin
    /// also makes terminal output nonblocking (`src/keyboard.c:8256`).
    #[test]
    fn nonblocking_pty_output_fixture() {
        if std::env::var_os(NONBLOCKING_PTY_FIXTURE).is_none() {
            return;
        }

        print!("fixture-ready\n");
        std::io::stdout().flush().expect("flush fixture readiness");
        let mut input = [0_u8; 1];
        std::io::stdin()
            .read_exact(&mut input)
            .expect("read fixture trigger");

        let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
        assert!(flags >= 0, "read fixture descriptor flags");
        assert_eq!(
            unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK,) },
            0,
            "make the shared slave description nonblocking",
        );

        let block = [b'x'; 512];
        for _ in 0..128 {
            // Deliberately mirror GNU `tty_write_glyphs_1`: output is attempted
            // once and a short/EAGAIN write is not retried.
            unsafe {
                libc::write(libc::STDOUT_FILENO, block.as_ptr().cast(), block.len());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        unsafe {
            libc::write(
                libc::STDOUT_FILENO,
                b"fixture-complete\n".as_ptr().cast(),
                b"fixture-complete\n".len(),
            );
        }
    }

    #[test]
    fn tui_session_drains_nonblocking_output_between_client_observations() {
        let launch = TuiLaunch::new(std::env::current_exe().expect("current test executable"))
            .args([
                "--exact",
                "tests::nonblocking_pty_output_fixture",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(NONBLOCKING_PTY_FIXTURE, "1");
        let mut session = TuiSession::spawn_launch(launch, "NONBLOCKING-OUTPUT");
        session.read_until(Duration::from_secs(2), |grid| {
            grid.iter().any(|row| row.contains("fixture-ready"))
        });
        session.clear_recent_output();

        session.send(b"x\n");
        // Test clients do real work between observations. The transport must
        // keep draining independently during that interval, like a terminal.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            session.run_to_completion(Duration::from_secs(2)),
            TuiProcessOutcome::Exited,
        );

        assert!(
            session
                .recent_output()
                .windows(b"fixture-complete".len())
                .any(|window| window == b"fixture-complete"),
            "the PTY queue filled while the client was not calling read; the nonblocking child lost its output tail",
        );
    }

    #[test]
    fn tui_session_records_the_pty_interaction_at_its_public_artifact_path() {
        let artifacts = tempfile::tempdir().expect("create recording root");
        let launch = TuiLaunch::new("sh").args([
            "-c",
            "printf ready; IFS= read -r line; printf 'done:%s' \"$line\"",
        ]);
        let mut session = TuiSession::spawn_launch_for_recording_test(
            launch,
            "GNU",
            TuiTerminalConfig::new("xterm-256color", 24, 80),
            RecordingPolicy::On,
            artifacts.path(),
            RecordingIdentity::new("neomacs-tui-tests", "pty interaction", "GNU"),
        );
        session.read(Duration::from_secs(1));
        session.send(b"go\n");
        session.resize(30, 90);
        session.mark_recording("command complete");
        assert_eq!(
            session.run_to_completion(Duration::from_secs(1)),
            TuiProcessOutcome::Exited
        );
        let path = session
            .recording_path()
            .expect("recording path")
            .to_path_buf();

        drop(session);

        let lines = std::fs::read_to_string(path)
            .expect("read session cast")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid event"))
            .collect::<Vec<serde_json::Value>>();
        assert_eq!(
            lines[0]["term"],
            serde_json::json!({"cols": 80, "rows": 24, "type": "xterm-256color"})
        );
        let events = &lines[1..];
        let output = events
            .iter()
            .filter(|event| event[1] == "o")
            .filter_map(|event| event[2].as_str())
            .collect::<String>();

        assert!(output.contains("ready"));
        assert!(output.contains("done:go"));
        assert!(
            events
                .iter()
                .any(|event| event[1] == "i" && event[2] == "go\n")
        );
        assert!(
            events
                .iter()
                .any(|event| event[1] == "r" && event[2] == "90x30")
        );
        assert!(
            events
                .iter()
                .any(|event| event[1] == "m" && event[2] == "command complete")
        );
        assert!(events.iter().any(|event| event[1] == "x"));
    }

    #[test]
    fn tui_session_recording_is_disabled_by_default() {
        let artifacts = tempfile::tempdir().expect("create recording root");
        let session = TuiSession::spawn_launch_for_recording_test(
            TuiLaunch::new("sh").args(["-c", "printf ignored"]),
            "NEO",
            TuiTerminalConfig::default(),
            RecordingPolicy::default(),
            artifacts.path(),
            RecordingIdentity::new("neomacs-tui-tests", "recording off", "NEO"),
        );

        assert_eq!(session.recording_path(), None);
        drop(session);
        assert!(
            std::fs::read_dir(artifacts.path())
                .expect("read recording root")
                .next()
                .is_none()
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
    fn emacs_key_maps_control_semicolon_to_modify_other_keys() {
        assert_eq!(emacs_key("C-;"), b"\x1b[27;5;59~".to_vec());
    }

    #[test]
    fn emacs_key_maps_meta_space_as_a_complete_named_key() {
        assert_eq!(emacs_key("M-SPC"), vec![0x1b, b' ']);
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

/// One exact terminal row in a display comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiRow(usize);

impl TuiRow {
    pub const fn absolute(row: usize) -> Self {
        Self(row)
    }
}

/// Values that differ only because the two editors run in isolated fixtures.
///
/// An environment says which concrete path spellings denote the same
/// test-owned resource.  The resulting comparison remains exact after those
/// declared values are mapped to a shared canonical token.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PairedDisplayEnvironment {
    paths: Vec<PairedPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PairedPath {
    gnu: PathBuf,
    neomacs: PathBuf,
}

#[derive(Debug, Clone, Copy)]
enum DisplayPeer {
    Gnu,
    Neomacs,
}

impl PairedDisplayEnvironment {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare two concrete paths as the GNU and Neomacs spellings of one
    /// logical test resource.
    #[must_use]
    pub fn with_path_pair(mut self, gnu: impl Into<PathBuf>, neomacs: impl Into<PathBuf>) -> Self {
        self.paths.push(PairedPath {
            gnu: gnu.into(),
            neomacs: neomacs.into(),
        });
        self
    }

    /// Capture the path values which the harness necessarily isolates for a
    /// paired editor session.
    #[must_use]
    pub fn from_sessions(gnu: &TuiSession, neomacs: &TuiSession) -> Self {
        Self::new()
            .with_path_pair(gnu.home_dir(), neomacs.home_dir())
            .with_path_pair(gnu.temp_dir(), neomacs.temp_dir())
    }

    fn normalize(&self, peer: DisplayPeer, text: &str) -> String {
        self.paths
            .iter()
            .enumerate()
            .fold(text.to_owned(), |normalized, (index, path)| {
                let concrete = match peer {
                    DisplayPeer::Gnu => &path.gnu,
                    DisplayPeer::Neomacs => &path.neomacs,
                };
                normalized.replace(
                    concrete.to_string_lossy().as_ref(),
                    &format!("<PAIRED-PATH-{index}>"),
                )
            })
    }
}

/// A terminal's visible geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySize {
    pub rows: u16,
    pub columns: u16,
}

impl From<(u16, u16)> for DisplaySize {
    fn from((rows, columns): (u16, u16)) -> Self {
        Self { rows, columns }
    }
}

/// One terminal cell in row/column coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayCell {
    pub row: u16,
    pub column: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorVisibility {
    Visible,
    Hidden,
}

impl CursorVisibility {
    fn from_hidden(hidden: bool) -> Self {
        if hidden { Self::Hidden } else { Self::Visible }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// How a paired display comparison treats terminal colors.
///
/// `StyleTopology` compares only which cells share a face-like style.
/// `ExactTerminalValues` compares the literal terminal color representation.
/// `ResolvedRgb` additionally compares the colors those styles visibly paint;
/// indexed colors are resolved through the fixed test-terminal palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayColorContract {
    StyleTopology,
    ExactTerminalValues,
    ResolvedRgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayColorRepresentation {
    TerminalEncoding,
    ResolvedRgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayColorCoverage {
    StyleTopology,
    VisiblePaint,
    TerminalState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayColorPolicy {
    representation: DisplayColorRepresentation,
    coverage: DisplayColorCoverage,
}

impl DisplayColorContract {
    const fn policy(self) -> DisplayColorPolicy {
        match self {
            Self::StyleTopology => DisplayColorPolicy {
                representation: DisplayColorRepresentation::ResolvedRgb,
                coverage: DisplayColorCoverage::StyleTopology,
            },
            Self::ExactTerminalValues => DisplayColorPolicy {
                representation: DisplayColorRepresentation::TerminalEncoding,
                coverage: DisplayColorCoverage::TerminalState,
            },
            Self::ResolvedRgb => DisplayColorPolicy {
                representation: DisplayColorRepresentation::ResolvedRgb,
                coverage: DisplayColorCoverage::VisiblePaint,
            },
        }
    }
}

/// The terminal color state selected for comparison at one cell.
///
/// Under the visible-paint policy, `foreground` is absent for an ordinary blank
/// cell because its retained terminal foreground is write history. The exact
/// terminal-state policy retains that value as `Some`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayCellColors {
    pub foreground: Option<DisplayColor>,
    pub background: DisplayColor,
}

impl DisplayColorPolicy {
    fn normalize(self, color: DisplayColor) -> DisplayColor {
        match self.representation {
            DisplayColorRepresentation::TerminalEncoding => color,
            DisplayColorRepresentation::ResolvedRgb => color.resolve_xterm_256(),
        }
    }

    fn cell_colors(self, cell: &vt100::Cell) -> Option<DisplayCellColors> {
        match self.coverage {
            DisplayColorCoverage::StyleTopology => None,
            DisplayColorCoverage::TerminalState => Some(DisplayCellColors {
                foreground: Some(self.normalize(cell.fgcolor().into())),
                background: self.normalize(cell.bgcolor().into()),
            }),
            DisplayColorCoverage::VisiblePaint => Some(DisplayCellColors {
                foreground: (!is_visually_blank(cell) || cell.underline() || cell.inverse())
                    .then(|| self.normalize(cell.fgcolor().into())),
                background: self.normalize(cell.bgcolor().into()),
            }),
        }
    }
}

impl From<vt100::Color> for DisplayColor {
    fn from(color: vt100::Color) -> Self {
        match color {
            vt100::Color::Default => Self::Default,
            vt100::Color::Idx(index) => Self::Indexed(index),
            vt100::Color::Rgb(red, green, blue) => Self::Rgb(red, green, blue),
        }
    }
}

impl DisplayColor {
    /// Resolve `screen-256color` indices through xterm's standard palette.
    ///
    /// Default foreground/background remain typed as `Default`: a PTY does not
    /// advertise the embedding terminal's configurable default colors, so
    /// inventing RGB values for them would create false visual equivalences.
    fn resolve_xterm_256(self) -> Self {
        let Self::Indexed(index) = self else {
            return self;
        };
        let (red, green, blue) = neomacs_display_protocol::xterm_256_rgb(index);
        Self::Rgb(red, green, blue)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DisplayAttributes {
    foreground: DisplayColor,
    background: DisplayColor,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

impl From<&vt100::Cell> for DisplayAttributes {
    fn from(cell: &vt100::Cell) -> Self {
        Self {
            foreground: cell.fgcolor().into(),
            background: cell.bgcolor().into(),
            bold: cell.bold(),
            dim: cell.dim(),
            italic: cell.italic(),
            underline: cell.underline(),
            inverse: cell.inverse(),
        }
    }
}

impl DisplayAttributes {
    fn for_color_policy(mut self, policy: DisplayColorPolicy) -> Self {
        self.foreground = policy.normalize(self.foreground);
        self.background = policy.normalize(self.background);
        self
    }
}

fn is_visually_blank(cell: &vt100::Cell) -> bool {
    !cell.is_wide_continuation() && cell.contents().chars().all(|character| character == ' ')
}

/// The attributes which can actually paint one terminal cell.
///
/// GNU clears to end of line after selecting only the background component of
/// the active face (`tty_clear_end_of_line` calls `tty_background_highlight`).
/// Terminal emulators nevertheless retain irrelevant foreground/weight state
/// on those blank cells.  Keeping blank and glyph paint as distinct enum
/// variants prevents terminal write history from masquerading as a rendered
/// face difference while making every visibly meaningful blank style explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum VisibleCellStyle {
    Glyph(DisplayAttributes),
    Blank(VisibleBlankStyle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum VisibleBlankStyle {
    Background(DisplayColor),
    Underlined {
        foreground: DisplayColor,
        background: DisplayColor,
        bold: bool,
        dim: bool,
    },
    Inverted(DisplayAttributes),
}

impl From<&vt100::Cell> for VisibleCellStyle {
    fn from(cell: &vt100::Cell) -> Self {
        let attributes = DisplayAttributes::from(cell);
        if !is_visually_blank(cell) {
            return Self::Glyph(attributes);
        }

        let visible = if attributes.inverse {
            // Inverse video makes the nominal foreground the painted
            // background, so none of its attributes are safely discardable.
            VisibleBlankStyle::Inverted(attributes)
        } else if attributes.underline {
            // Underline paints ink even beneath a space.  Italic cannot affect
            // an empty cell, but foreground intensity can affect the line.
            VisibleBlankStyle::Underlined {
                foreground: attributes.foreground,
                background: attributes.background,
                bold: attributes.bold,
                dim: attributes.dim,
            }
        } else {
            VisibleBlankStyle::Background(attributes.background)
        };
        Self::Blank(visible)
    }
}

impl VisibleCellStyle {
    fn for_color_policy(cell: &vt100::Cell, policy: DisplayColorPolicy) -> Self {
        match Self::from(cell) {
            Self::Glyph(attributes) => Self::Glyph(attributes.for_color_policy(policy)),
            Self::Blank(VisibleBlankStyle::Background(background)) => {
                Self::Blank(VisibleBlankStyle::Background(policy.normalize(background)))
            }
            Self::Blank(VisibleBlankStyle::Underlined {
                foreground,
                background,
                bold,
                dim,
            }) => Self::Blank(VisibleBlankStyle::Underlined {
                foreground: policy.normalize(foreground),
                background: policy.normalize(background),
                bold,
                dim,
            }),
            Self::Blank(VisibleBlankStyle::Inverted(attributes)) => Self::Blank(
                VisibleBlankStyle::Inverted(attributes.for_color_policy(policy)),
            ),
        }
    }
}

/// One semantic difference between the GNU Emacs and Neomacs displays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayDifference {
    Geometry {
        gnu: DisplaySize,
        neomacs: DisplaySize,
    },
    TextRow {
        row: TuiRow,
        gnu: String,
        neomacs: String,
    },
    StyleClass {
        cell: DisplayCell,
        gnu_class: DisplayCell,
        neomacs_class: DisplayCell,
    },
    Colors {
        cell: DisplayCell,
        contract: DisplayColorContract,
        gnu: DisplayCellColors,
        neomacs: DisplayCellColors,
    },
    RowWrap {
        row: TuiRow,
        gnu: bool,
        neomacs: bool,
    },
    CursorPosition {
        gnu: DisplayCell,
        neomacs: DisplayCell,
    },
    CursorVisibility {
        gnu: CursorVisibility,
        neomacs: CursorVisibility,
    },
}

/// The complete result of comparing two terminal displays.
#[derive(Debug, PartialEq, Eq)]
pub struct DisplayReport {
    unexpected: Vec<DisplayDifference>,
}

impl DisplayReport {
    pub fn is_satisfied(&self) -> bool {
        self.unexpected.is_empty()
    }

    pub fn unexpected(&self) -> &[DisplayDifference] {
        &self.unexpected
    }
}

/// Compare the complete visible state of two virtual terminal screens.
pub fn compare_displays(gnu: &vt100::Screen, neomacs: &vt100::Screen) -> DisplayReport {
    compare_displays_with_environment(gnu, neomacs, None, DisplayColorContract::ResolvedRgb)
}

/// Compare complete visible state under an explicit terminal-color contract.
pub fn compare_displays_with_color_contract(
    gnu: &vt100::Screen,
    neomacs: &vt100::Screen,
    color_contract: DisplayColorContract,
) -> DisplayReport {
    compare_displays_with_environment(gnu, neomacs, None, color_contract)
}

/// Compare two displays after canonicalizing only values declared by a
/// paired fixture environment.
pub fn compare_displays_in_environment(
    gnu: &vt100::Screen,
    neomacs: &vt100::Screen,
    environment: &PairedDisplayEnvironment,
) -> DisplayReport {
    compare_displays_with_environment(
        gnu,
        neomacs,
        Some(environment),
        DisplayColorContract::ResolvedRgb,
    )
}

/// Compare the displays of a paired editor session under their automatically
/// captured isolated HOME and TMPDIR mappings.
pub fn compare_session_displays(gnu: &TuiSession, neomacs: &TuiSession) -> DisplayReport {
    let environment = PairedDisplayEnvironment::from_sessions(gnu, neomacs);
    compare_displays_in_environment(gnu.screen(), neomacs.screen(), &environment)
}

fn compare_displays_with_environment(
    gnu: &vt100::Screen,
    neomacs: &vt100::Screen,
    environment: Option<&PairedDisplayEnvironment>,
    color_contract: DisplayColorContract,
) -> DisplayReport {
    let color_policy = color_contract.policy();
    let gnu_size = DisplaySize::from(gnu.size());
    let neomacs_size = DisplaySize::from(neomacs.size());
    let mut unexpected = Vec::new();
    if gnu_size != neomacs_size {
        unexpected.push(DisplayDifference::Geometry {
            gnu: gnu_size,
            neomacs: neomacs_size,
        });
    }
    for row in 0..gnu_size.rows.min(neomacs_size.rows) {
        let gnu_text = visible_row_text(gnu, row, gnu_size.columns);
        let neomacs_text = visible_row_text(neomacs, row, neomacs_size.columns);
        let text_matches = environment.is_some_and(|environment| {
            environment.normalize(DisplayPeer::Gnu, &gnu_text)
                == environment.normalize(DisplayPeer::Neomacs, &neomacs_text)
        }) || gnu_text == neomacs_text;
        if !text_matches {
            unexpected.push(DisplayDifference::TextRow {
                row: TuiRow::absolute(row.into()),
                gnu: gnu_text,
                neomacs: neomacs_text,
            });
        }
        let gnu_wrapped = gnu.row_wrapped(row);
        let neomacs_wrapped = neomacs.row_wrapped(row);
        if gnu_wrapped != neomacs_wrapped {
            unexpected.push(DisplayDifference::RowWrap {
                row: TuiRow::absolute(row.into()),
                gnu: gnu_wrapped,
                neomacs: neomacs_wrapped,
            });
        }
    }
    let mut gnu_style_origins = std::collections::HashMap::new();
    let mut neomacs_style_origins = std::collections::HashMap::new();
    for row in 0..gnu_size.rows.min(neomacs_size.rows) {
        for column in 0..gnu_size.columns.min(neomacs_size.columns) {
            let at = DisplayCell { row, column };
            let gnu_cell = gnu.cell(row, column).expect("cell inside GNU geometry");
            let neomacs_cell = neomacs
                .cell(row, column)
                .expect("cell inside Neomacs geometry");
            let gnu_class = *gnu_style_origins
                .entry(VisibleCellStyle::for_color_policy(gnu_cell, color_policy))
                .or_insert(at);
            let neomacs_class = *neomacs_style_origins
                .entry(VisibleCellStyle::for_color_policy(
                    neomacs_cell,
                    color_policy,
                ))
                .or_insert(at);
            if gnu_class != neomacs_class {
                unexpected.push(DisplayDifference::StyleClass {
                    cell: at,
                    gnu_class,
                    neomacs_class,
                });
            }
            if let (Some(gnu_colors), Some(neomacs_colors)) = (
                color_policy.cell_colors(gnu_cell),
                color_policy.cell_colors(neomacs_cell),
            ) && gnu_colors != neomacs_colors
            {
                unexpected.push(DisplayDifference::Colors {
                    cell: at,
                    contract: color_contract,
                    gnu: gnu_colors,
                    neomacs: neomacs_colors,
                });
            }
        }
    }
    let (gnu_cursor_row, gnu_cursor_column) = gnu.cursor_position();
    let (neomacs_cursor_row, neomacs_cursor_column) = neomacs.cursor_position();
    let gnu_cursor = DisplayCell {
        row: gnu_cursor_row,
        column: gnu_cursor_column,
    };
    let neomacs_cursor = DisplayCell {
        row: neomacs_cursor_row,
        column: neomacs_cursor_column,
    };
    if gnu_cursor != neomacs_cursor {
        unexpected.push(DisplayDifference::CursorPosition {
            gnu: gnu_cursor,
            neomacs: neomacs_cursor,
        });
    }
    let gnu_cursor_visibility = CursorVisibility::from_hidden(gnu.hide_cursor());
    let neomacs_cursor_visibility = CursorVisibility::from_hidden(neomacs.hide_cursor());
    if gnu_cursor_visibility != neomacs_cursor_visibility {
        unexpected.push(DisplayDifference::CursorVisibility {
            gnu: gnu_cursor_visibility,
            neomacs: neomacs_cursor_visibility,
        });
    }
    DisplayReport { unexpected }
}

/// Return the visible text of one row, independent of terminal write history.
///
/// `vt100::Screen::contents_between` preserves written trailing spaces while
/// omitting never-written trailing cells.  Those states are observably
/// different on the wire but display the same blank cells.  `ExactDisplay`
/// compares the rendered display (and separately compares every cell's visible
/// colors and style class), so it canonicalizes only trailing ASCII blanks.
/// Tests that need exact write-state parity use [`RawTerminalSnapshot`] instead.
fn visible_row_text(screen: &vt100::Screen, row: u16, columns: u16) -> String {
    screen
        .contents_between(row, 0, row, columns)
        .trim_end_matches(' ')
        .to_string()
}

#[cfg(test)]
mod exact_display_tests {
    use super::*;

    fn screen(rows: u16, cols: u16, bytes: &[u8]) -> vt100::Parser {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(bytes);
        parser
    }

    #[test]
    fn exact_display_rejects_different_terminal_geometry() {
        let gnu = screen(3, 8, b"same");
        let neo = screen(4, 8, b"same");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(report.unexpected().iter().any(|difference| matches!(
            difference,
            DisplayDifference::Geometry {
                gnu: DisplaySize {
                    rows: 3,
                    columns: 8
                },
                neomacs: DisplaySize {
                    rows: 4,
                    columns: 8
                }
            }
        )));
    }

    #[test]
    fn exact_display_rejects_a_different_text_row() {
        let gnu = screen(2, 8, b"alpha");
        let neo = screen(2, 8, b"alpHa");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert_eq!(
            report.unexpected(),
            &[DisplayDifference::TextRow {
                row: TuiRow::absolute(0),
                gnu: "alpha".to_string(),
                neomacs: "alpHa".to_string(),
            }]
        );
    }

    #[test]
    fn paired_environment_normalizes_only_its_declared_session_paths() {
        let gnu = screen(2, 80, b"Wrote /tmp/tui-home-gnu-ABC123/example.txt");
        let neo = screen(2, 80, b"Wrote /tmp/tui-home-neo-XYZ789/example.txt");
        let environment = PairedDisplayEnvironment::new()
            .with_path_pair("/tmp/tui-home-gnu-ABC123", "/tmp/tui-home-neo-XYZ789");

        let raw = compare_displays(gnu.screen(), neo.screen());
        let normalized = compare_displays_in_environment(gnu.screen(), neo.screen(), &environment);

        assert_eq!(raw.unexpected().len(), 1);
        assert!(normalized.is_satisfied(), "{normalized:#?}");
    }

    #[test]
    fn exact_display_treats_written_and_unwritten_blank_cells_as_same_display() {
        let gnu = screen(1, 8, b"abc");
        let neo = screen(1, 8, b"abc   \x1b[1;4H");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(report.is_satisfied(), "{report:#?}");
    }

    #[test]
    fn exact_display_includes_text_attributes_in_face_classes() {
        let gnu = screen(1, 4, b"\x1b[31mA\x1b[1mB");
        let neo = screen(1, 4, b"\x1b[32mAB");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(report.unexpected().iter().any(|difference| matches!(
            difference,
            DisplayDifference::StyleClass {
                cell: DisplayCell { row: 0, column: 1 },
                gnu_class: DisplayCell { row: 0, column: 1 },
                neomacs_class: DisplayCell { row: 0, column: 0 },
            }
        )));
    }

    #[test]
    fn exact_display_requires_rgb_equality_beyond_style_topology() {
        let gnu = screen(1, 4, b"\x1b[38;2;255;0;0mAB\x1b[0m");
        let neo = screen(1, 4, b"\x1b[38;2;0;255;0mAB\x1b[0m");

        let report = compare_displays(gnu.screen(), neo.screen());
        let topology_only = compare_displays_with_color_contract(
            gnu.screen(),
            neo.screen(),
            DisplayColorContract::StyleTopology,
        );

        assert!(report.unexpected().iter().any(|difference| matches!(
            difference,
            DisplayDifference::Colors {
                cell: DisplayCell { row: 0, column: 0 },
                contract: DisplayColorContract::ResolvedRgb,
                gnu: DisplayCellColors {
                    foreground: Some(DisplayColor::Rgb(255, 0, 0)),
                    background: DisplayColor::Default,
                },
                neomacs: DisplayCellColors {
                    foreground: Some(DisplayColor::Rgb(0, 255, 0)),
                    background: DisplayColor::Default,
                },
            }
        )));
        assert!(topology_only.is_satisfied(), "{topology_only:#?}");
    }

    #[test]
    fn resolved_rgb_equates_xterm_index_with_rgb_but_exact_terminal_values_do_not() {
        let indexed = screen(1, 2, b"\x1b[38;5;196mA\x1b[0m");
        let rgb = screen(1, 2, b"\x1b[38;2;255;0;0mA\x1b[0m");

        let resolved = compare_displays(indexed.screen(), rgb.screen());
        let terminal_values = compare_displays_with_color_contract(
            indexed.screen(),
            rgb.screen(),
            DisplayColorContract::ExactTerminalValues,
        );

        assert!(resolved.is_satisfied(), "{resolved:#?}");
        assert!(
            terminal_values
                .unexpected()
                .iter()
                .any(|difference| matches!(
                    difference,
                    DisplayDifference::Colors {
                        cell: DisplayCell { row: 0, column: 0 },
                        contract: DisplayColorContract::ExactTerminalValues,
                        gnu: DisplayCellColors {
                            foreground: Some(DisplayColor::Indexed(196)),
                            background: DisplayColor::Default,
                        },
                        neomacs: DisplayCellColors {
                            foreground: Some(DisplayColor::Rgb(255, 0, 0)),
                            background: DisplayColor::Default,
                        },
                    }
                ))
        );
    }

    #[test]
    fn exact_display_ignores_foreground_only_state_on_blank_cells() {
        // GNU's `tty_clear_end_of_line` clears while the current face's
        // foreground is still active.  A renderer may reset that foreground
        // first; with the default background the two blank remainders are
        // visually identical even though their terminal cells differ.
        let gnu = screen(1, 8, b"\x1b[31mabc\x1b[K");
        let neo = screen(1, 8, b"\x1b[31mabc\x1b[0m\x1b[K");

        let report = compare_displays(gnu.screen(), neo.screen());
        let terminal_values = compare_displays_with_color_contract(
            gnu.screen(),
            neo.screen(),
            DisplayColorContract::ExactTerminalValues,
        );

        assert!(report.is_satisfied(), "{report:#?}");
        assert!(
            terminal_values
                .unexpected()
                .iter()
                .any(|difference| matches!(
                    difference,
                    DisplayDifference::Colors {
                        cell: DisplayCell { row: 0, column: 3 },
                        contract: DisplayColorContract::ExactTerminalValues,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn exact_display_retains_visible_background_state_on_blank_cells() {
        let gnu = screen(1, 8, b"\x1b[1;2H\x1b[31mabc\x1b[44m   ");
        let neo = screen(1, 8, b"\x1b[1;2H\x1b[32mabc\x1b[0m   ");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(report.unexpected().iter().any(|difference| matches!(
            difference,
            DisplayDifference::StyleClass {
                cell: DisplayCell { row: 0, column: 4 },
                ..
            }
        )));
    }

    #[test]
    fn resolved_rgb_uses_resolved_colors_for_style_class_boundaries() {
        let indexed = screen(1, 3, b"\x1b[38;5;196mAB\x1b[0m");
        let mixed = screen(1, 3, b"\x1b[38;2;255;0;0mA\x1b[38;5;196mB\x1b[0m");

        let report = compare_displays(indexed.screen(), mixed.screen());

        assert!(report.is_satisfied(), "{report:#?}");
    }

    #[test]
    fn exact_display_retains_visible_underline_state_on_blank_cells() {
        let gnu = screen(1, 8, b"\x1b[1;2H\x1b[31mabc\x1b[4m   ");
        let neo = screen(1, 8, b"\x1b[1;2H\x1b[32mabc\x1b[0m   ");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(report.unexpected().iter().any(|difference| matches!(
            difference,
            DisplayDifference::StyleClass {
                cell: DisplayCell { row: 0, column: 4 },
                ..
            }
        )));
    }

    #[test]
    fn exact_display_rejects_different_soft_wrap_state() {
        let gnu = screen(2, 4, b"abcde");
        let neo = screen(2, 4, b"abcd\x1b[2;1He");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(report.unexpected().contains(&DisplayDifference::RowWrap {
            row: TuiRow::absolute(0),
            gnu: true,
            neomacs: false,
        }));
    }

    #[test]
    fn exact_display_rejects_a_different_cursor_position() {
        let gnu = screen(2, 8, b"abc");
        let neo = screen(2, 8, b"abc\x1b[1;1H");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(
            report
                .unexpected()
                .contains(&DisplayDifference::CursorPosition {
                    gnu: DisplayCell { row: 0, column: 3 },
                    neomacs: DisplayCell { row: 0, column: 0 },
                })
        );
    }

    #[test]
    fn exact_display_rejects_different_cursor_visibility() {
        let gnu = screen(2, 8, b"abc\x1b[?25l");
        let neo = screen(2, 8, b"abc");

        let report = compare_displays(gnu.screen(), neo.screen());

        assert!(
            report
                .unexpected()
                .contains(&DisplayDifference::CursorVisibility {
                    gnu: CursorVisibility::Hidden,
                    neomacs: CursorVisibility::Visible,
                })
        );
    }
}
