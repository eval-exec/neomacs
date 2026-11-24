#![cfg(unix)]
//! TTY seam oracle (MILESTONES M0; DESIGN §14.2 L3′; D38, R1 F2).
//!
//! The layout snapshot (`neomacs--frame-snapshot`) lays the frame out again
//! and never reaches `TtyRif` or the TTY writer
//! (`crates/neomacs/src/frame_layout.rs` `collect_snapshot_states` versus
//! `run_tty_rif_redisplay_to`), so a seam refactor can keep it identical while
//! changing what the terminal shows. This oracle records both, per step of one
//! fixed scenario, for the two TTY arrangements:
//!
//! * `seam_baseline_daemon_tty_client`: `neomacs --fg-daemon` plus a
//!   `neomacsclient -t` frame on a pty (a secondary termcap terminal);
//! * `seam_baseline_primary_tty`: a non-daemon `neomacs -nw` whose primary
//!   terminal is the pty, the only arrangement that installs the TTY popup
//!   host (R1 F1), so it alone runs the menu steps.
//!
//! `NEOMACS_SEAM_BASELINE=capture` records into `NEOMACS_SEAM_BASELINE_DIR`;
//! `compare` runs the scenario again into `<dir>/<arrangement>.actual/` and
//! fails listing every step whose parsed screen or layout snapshot differs.
//! The parsed screen is [`CanonicalScreenDump`]: what the terminal visibly
//! shows, plus its modes. The raw pty bytes of each step are kept for
//! diagnosis only and never compared: damage timing may legitimately vary
//! between implementations; the visible screen may not.
//!
//! Each step is observed in one order ([`observe`]): once the screen has been
//! unchanged for [`QUIESCENCE`], it is recorded as the step painted it; only
//! then is the layout snapshot taken. The snapshot form forces a redisplay,
//! which repaints the terminal (`run_frame_snapshot` in `neovm-display`'s
//! `xdisp`), so the screen is read again and must not have changed: a step
//! whose own redisplay left the terminal stale fails instead of being
//! recorded as the oracle's repaint.
//!
//! The scenario ends by leaving the terminal ([`EXIT`]): `C-x C-c` on the
//! frame, then the screen once the pty process has exited. Its modes show
//! whether the editor gave the terminal back as it found it (main screen,
//! keypad and cursor keys normal, bracketed paste off). The line discipline
//! the editor also restores (termios) is not part of the screen and is not
//! observed.
//!
//! Both tests are ignored by default. They drive the dev binaries `neomacs`
//! and `neomacsclient` in `$CARGO_TARGET_DIR/debug` (else
//! `<workspace>/target/debug`), not the release editor the rest of this suite
//! drives, with the bootstrap runtime image dumped beside them ([`REBUILD`]).
//! Those names hold whichever product cargo linked last. `cargo nextest run
//! -p neomacs` links its test build there: the dev-dependency features of
//! `crates/neomacs/Cargo.toml` (`redisplay-test-policy`, `test-support`)
//! compile other code paths into it, and `daemon_lifecycle`'s fixture dumps
//! the image with it. Nothing rebuilds the binaries for the oracle, so it
//! refuses, by the cargo dep-info file beside each (`<binary>.d`,
//! [`built_from_current_sources`]), a binary that `cargo build` did not link
//! last or that is older than any source the file names.
//! A dev build's dump fingerprint is a constant slot, so the editor loads an
//! image dumped by any build without complaint (`neomacs_infra::runtime_image`).
//! The oracle therefore asks the running editor which image it loaded and
//! which Lisp it has loaded, and refuses an image older than the editor or
//! than any of that Lisp, by the freshness rule the image fixture provisions
//! by ([`ReportedRuntime::fresh_image`]), and an image that a copy of another
//! build dumped ([`image_dumper`]). Every run directory records the editor's,
//! the client's and the image's sha256 and the binaries' `--version` in
//! `provenance`, which is never compared.

use neomacs_test_env::fresh_build_stamp::sha256_file;
use neomacs_test_env::{CanonicalEnvironment, CanonicalPin, CanonicalProfile, PinnedCommand};
use neomacs_tui_tests::{
    CanonicalScreenDump, TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig,
};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::{self, Write as _};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant, SystemTime};

const MODE_VARIABLE: &str = "NEOMACS_SEAM_BASELINE";
const DIRECTORY_VARIABLE: &str = "NEOMACS_SEAM_BASELINE_DIR";
const SCENARIO_FILE: &str = "scenario.el";
/// What a run drove; written into every run directory and never compared.
const PROVENANCE_FILE: &str = "provenance";
const EDITOR_LOG_FILE: &str = "editor.log";
/// Cargo's target directory, as `neomacs_infra::melpa_cache_root` reads it.
const TARGET_DIRECTORY_VARIABLE: &str = "CARGO_TARGET_DIR";

/// How to get a runtime the oracle accepts. The image recipe is
/// `neomacs_infra::runtime_image::provision_bootstrap_image`, which
/// `daemon_lifecycle` runs for its own editor.
const REBUILD: &str = "rebuild the dev binaries (`cargo build -p neomacs --bins --features \
     dbus-vendored`), then dump the bootstrap image beside the editor with the recipe of \
     `neomacs_infra::runtime_image::provision_bootstrap_image` (a copy of the editor named \
     `neomacs-temacs` runs `--batch -l loadup --temacs=pbootstrap` from the workspace root)";

/// The name the image recipe gives the copy of the editor that dumps the
/// image (`neovm_core::emacs_core::load::TEMACS_ROLE_BINARY_NAME`; this
/// crate's closure leaves neovm-core out).
const DUMPER_NAME: &str = "neomacs-temacs";

/// Asks the editor which runtime image it loaded (`pdumper-stats`, empty when
/// it bootstrapped from sources) and which Lisp files it has loaded
/// (`load-history`), one per line.
const RUNTIME_REPORT_FORM: &str = r#"(mapconcat #'identity (cons (or (cdr (assq 'dump-file-name (pdumper-stats))) "") (delq nil (mapcar (lambda (entry) (and (stringp (car entry)) (car entry))) load-history))) "\n")"#;

const TERMINAL_TYPE: &str = "xterm-256color";
const ROWS: u16 = 30;
const COLUMNS: u16 = 100;

/// A step is recorded once its visible screen has stayed unchanged this long.
const QUIESCENCE: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(50);
/// Debug binaries boot from the bootstrap image in seconds; this bound only
/// turns a hung start into a failure.
const STARTUP_BUDGET: Duration = Duration::from_secs(180);
const STEP_BUDGET: Duration = Duration::from_secs(60);
const EVAL_BUDGET: Duration = Duration::from_secs(60);

const ISOLATED_DIRECTORIES: [&str; 7] =
    ["home", "runtime", "config", "data", "cache", "state", "tmp"];
/// Variables that would point a client at another server or a display.
const REMOVED_VARIABLES: [&str; 5] = [
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "EMACS_SOCKET_NAME",
    "EMACS_SERVER_FILE",
    "ALTERNATE_EDITOR",
];

/// What one run of a test does with the baseline directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BaselineMode {
    /// Replace the recorded baseline with this run.
    Capture,
    /// Record this run beside the baseline and require it to be identical.
    Compare,
}

impl BaselineMode {
    fn from_environment() -> Self {
        match std::env::var(MODE_VARIABLE).as_deref() {
            Ok("capture") => Self::Capture,
            Ok("compare") => Self::Compare,
            other => panic!(
                "{MODE_VARIABLE} must be `capture` or `compare` (got {other:?}); \
                 {DIRECTORY_VARIABLE} names the baseline directory"
            ),
        }
    }
}

fn baseline_directory() -> PathBuf {
    let directory = std::env::var_os(DIRECTORY_VARIABLE)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{DIRECTORY_VARIABLE} must name the baseline directory"));
    assert!(
        directory.is_absolute(),
        "{DIRECTORY_VARIABLE} must be absolute: {}",
        directory.display()
    );
    directory
}

/// The two TTY arrangements whose output the seam refactors must preserve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Arrangement {
    /// `neomacs -Q --fg-daemon=seam`, then `neomacsclient -s seam -t` on the pty.
    DaemonTtyClient,
    /// `neomacs -Q -nw` on the pty, serving `-e` requests as `seam-nw`.
    PrimaryTty,
}

impl Arrangement {
    fn directory_name(self) -> &'static str {
        match self {
            Self::DaemonTtyClient => "daemon_tty_client",
            Self::PrimaryTty => "primary_tty",
        }
    }

    fn server_name(self) -> &'static str {
        match self {
            Self::DaemonTtyClient => "seam",
            Self::PrimaryTty => "seam-nw",
        }
    }

    fn runs(self, coverage: Coverage) -> bool {
        match coverage {
            Coverage::Both => true,
            Coverage::PrimaryTtyOnly => self == Self::PrimaryTty,
        }
    }
}

/// Which arrangements run a step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Coverage {
    Both,
    /// The TTY menu: a daemon's client frame has no menu bar to open.
    PrimaryTtyOnly,
}

/// A screen condition that ends a keyboard step.
#[derive(Clone, Copy, Debug)]
enum ScreenState {
    Shows(&'static str),
    Lacks(&'static str),
}

impl ScreenState {
    fn holds(self, grid: &[String]) -> bool {
        let shown = |text: &str| grid.iter().any(|row| row.contains(text));
        match self {
            Self::Shows(text) => shown(text),
            Self::Lacks(text) => !shown(text),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Action {
    /// Evaluate one form through `neomacsclient -s NAME -e FORM`.
    Eval(&'static str),
    /// Type keys on the pty and wait until the screen reaches a state.
    Keys {
        keys: &'static str,
        until: ScreenState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Observation {
    /// The parsed screen once quiescent, then the selected frame's
    /// `text-faces` snapshot. The snapshot form forces a redisplay, and the
    /// screen must not change under it ([`observe`]).
    ScreenAndLayout,
    /// The parsed screen only: a modal TTY menu is up, and evaluating the
    /// snapshot form would run a redisplay underneath it.
    Screen,
}

#[derive(Clone, Copy, Debug)]
struct Step {
    name: &'static str,
    coverage: Coverage,
    action: Action,
    observation: Observation,
}

/// The fixed scenario (MILESTONES M0). Step numbers are indices here, so an
/// arrangement that skips a step leaves a gap rather than renumbering.
const SCENARIO: &[Step] = &[
    // The mode line of a visited file must not depend on the checkout's VC
    // state (branch name, modification status).
    Step {
        name: "start",
        coverage: Coverage::Both,
        action: Action::Eval("(setq vc-handled-backends nil)"),
        observation: Observation::ScreenAndLayout,
    },
    Step {
        name: "insert",
        coverage: Coverage::Both,
        action: Action::Eval(
            r#"(with-current-buffer "*scratch*" (goto-char (point-max)) (insert "Seam baseline: text inserted into *scratch*.\n"))"#,
        ),
        observation: Observation::ScreenAndLayout,
    },
    Step {
        name: "split",
        coverage: Coverage::Both,
        action: Action::Eval("(split-window-below)"),
        observation: Observation::ScreenAndLayout,
    },
    Step {
        name: "find-file",
        coverage: Coverage::Both,
        action: Action::Eval(r#"(find-file (expand-file-name "COPYING" data-directory))"#),
        observation: Observation::ScreenAndLayout,
    },
    Step {
        name: "scroll-up",
        coverage: Coverage::Both,
        action: Action::Eval("(scroll-up-command)"),
        observation: Observation::ScreenAndLayout,
    },
    Step {
        name: "message",
        coverage: Coverage::Both,
        action: Action::Eval(r#"(message "Seam baseline: an echo-area message")"#),
        observation: Observation::ScreenAndLayout,
    },
    Step {
        name: "mode-line",
        coverage: Coverage::Both,
        action: Action::Eval(
            r#"(progn (setq-default mode-line-format '("SEAM " mode-line-buffer-identification " L%l C%c %p " mode-name)) (force-mode-line-update t))"#,
        ),
        observation: Observation::ScreenAndLayout,
    },
    // F10 is `menu-bar-open`.
    Step {
        name: "menu-open",
        coverage: Coverage::PrimaryTtyOnly,
        action: Action::Keys {
            keys: "F10",
            until: ScreenState::Shows("Visit New File"),
        },
        observation: Observation::Screen,
    },
    Step {
        name: "menu-dismissed",
        coverage: Coverage::PrimaryTtyOnly,
        action: Action::Keys {
            keys: "C-g",
            until: ScreenState::Lacks("Visit New File"),
        },
        observation: Observation::ScreenAndLayout,
    },
    Step {
        name: "unsplit",
        coverage: Coverage::Both,
        action: Action::Eval("(delete-other-windows)"),
        observation: Observation::ScreenAndLayout,
    },
];

/// The step after [`SCENARIO`] in both arrangements: type `keys` on the
/// frame, wait for the pty process to exit, and record the screen it leaves.
struct ExitStep {
    name: &'static str,
    keys: &'static str,
}

impl ExitStep {
    /// Numbered after the scenario's last step.
    fn stem(&self) -> String {
        format!("{:02}-{}", SCENARIO.len(), self.name)
    }
}

/// `C-x C-c` is `save-buffers-kill-terminal` (`lisp/files.el`). On the `-nw`
/// frame it kills the editor, which resets its terminal as it exits. On a
/// client frame it deletes only that client (`server-save-buffers-kill-terminal`
/// and `server-delete-client`, `lisp/server.el`, identical to GNU's), whose
/// terminal is reset as it is deleted; the daemon lives on until the oracle
/// stops it. GNU resets a tty in `reset_sys_modes` (`src/sysdep.c:1497`),
/// which `delete_tty` calls (`src/term.c:5016`) and exit reaches through
/// `reset_all_sys_modes` (`src/emacs.c:3150`); its hook
/// `tty_reset_terminal_modes` (`src/term.c:230`) ends keypad mode, restores
/// the cursor and leaves the full-screen mode. Source reading, UNATTESTED.
const EXIT: ExitStep = ExitStep {
    name: "exit",
    keys: "C-x C-c",
};

fn primary_server_form() -> String {
    format!(
        "(progn (setq server-name \"{}\") (server-start))",
        Arrangement::PrimaryTty.server_name()
    )
}

/// `scenario.el`: the scenario as recorded. Capture writes it from
/// [`SCENARIO`]; compare refuses to run a scenario that differs from it.
fn scenario_text() -> String {
    let mut text = String::from(
        ";;; Seam baseline scenario (MILESTONES M0).  Generated from\n\
         ;;; crates/neomacs-tui-tests/tests/seam_baseline.rs; compare refuses a changed scenario.\n",
    );
    let _ = writeln!(text, ";; terminal: {TERMINAL_TYPE} {ROWS}x{COLUMNS}");
    let _ = writeln!(
        text,
        ";; {}: neomacs -Q --fg-daemon={0}, then neomacsclient -s {0} -t",
        Arrangement::DaemonTtyClient.server_name()
    );
    let _ = writeln!(
        text,
        ";; {}: neomacs -Q -nw --eval '{}'",
        Arrangement::PrimaryTty.server_name(),
        primary_server_form()
    );
    let _ = writeln!(
        text,
        ";; each step: act; once the screen is unchanged for {QUIESCENCE:?}, record NN-step.screen"
    );
    let _ = writeln!(
        text,
        ";; screen+layout steps then record (neomacs--write-frame-snapshot FILE nil 'text-faces), which\n\
         ;; forces a redisplay; the screen must not change under it"
    );
    for (index, step) in SCENARIO.iter().enumerate() {
        let coverage = match step.coverage {
            Coverage::Both => "both",
            Coverage::PrimaryTtyOnly => "primary-tty only",
        };
        let observation = match step.observation {
            Observation::ScreenAndLayout => "screen+layout",
            Observation::Screen => "screen",
        };
        let _ = writeln!(
            text,
            ";; {index:02} {} [{coverage}] {observation}",
            step.name
        );
        match step.action {
            Action::Eval(form) => {
                let _ = writeln!(text, "{form}");
            }
            Action::Keys { keys, until } => {
                let _ = writeln!(text, ";; keys: {keys}; until the screen {until:?}");
            }
        }
    }
    let _ = writeln!(
        text,
        ";; {} [both] the screen the pty process leaves\n\
         ;; keys: {}; until the pty process exits; a daemon is then stopped with (kill-emacs)",
        EXIT.stem(),
        EXIT.keys
    );
    text
}

/// One scenario's private files, sockets and environment.
struct Isolation {
    root: tempfile::TempDir,
    environment: CanonicalEnvironment,
}

impl Isolation {
    fn new(arrangement: Arrangement) -> Self {
        let root = tempfile::Builder::new()
            .prefix(&format!("seam-{}-", arrangement.directory_name()))
            .tempdir()
            .expect("create the scenario's private root");
        for directory in ISOLATED_DIRECTORIES {
            let path = root.path().join(directory);
            std::fs::create_dir(&path).expect("create an isolated directory");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict an isolated directory");
        }
        let environment = CanonicalEnvironment::new(
            CanonicalProfile::Tui,
            root.path().join("home"),
            root.path().join("tmp"),
            neomacs_infra::workspace_root(),
        )
        .expect("canonical environment for the seam scenario");
        Self { root, environment }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn socket(&self, server: &str) -> PathBuf {
        self.path("runtime").join("emacs").join(server)
    }

    /// The XDG base directories, each private to this scenario.
    fn xdg(&self) -> [(CanonicalPin, PathBuf); 5] {
        [
            (CanonicalPin::XdgRuntimeDir, self.path("runtime")),
            (CanonicalPin::XdgConfigHome, self.path("config")),
            (CanonicalPin::XdgDataHome, self.path("data")),
            (CanonicalPin::XdgCacheHome, self.path("cache")),
            (CanonicalPin::XdgStateHome, self.path("state")),
        ]
    }

    /// A pinned command for a process that does not own the pty.
    fn command(&self, program: &Path) -> PinnedCommand {
        let mut command = self.environment.command(program);
        for (pin, path) in self.xdg() {
            command.override_pin(pin, path);
        }
        for name in REMOVED_VARIABLES {
            command.env_remove(name);
        }
        command
            .env("COLORTERM", "truecolor")
            .current_dir(self.path("home"));
        command
    }

    /// The pty process: the same isolation, applied by the harness's pty spawn.
    fn launch(&self, program: &Path) -> TuiLaunch {
        let mut launch = TuiLaunch::new(program.as_os_str())
            .with_uncertified_startup()
            .env("HOME", self.path("home"))
            .env("TMPDIR", self.path("tmp"))
            .env("COLORTERM", "truecolor")
            .current_dir(self.path("home"));
        for (pin, path) in self.xdg() {
            for name in pin.names() {
                launch = launch.env(*name, &path);
            }
        }
        for name in REMOVED_VARIABLES {
            launch = launch.env_remove(name);
        }
        launch
    }
}

/// Why a dep-info file is not the rule cargo writes beside a binary.
#[derive(Debug, Eq, PartialEq)]
enum DepInfoProblem {
    /// The file exists but could not be read as text.
    Unreadable(std::io::ErrorKind),
    /// No rule names the binary as its target.
    NoRuleFor(PathBuf),
    /// A relative source path (`build.dep-info-basedir`): its base directory
    /// is cargo configuration this test cannot know.
    RelativeSource(PathBuf),
}

impl fmt::Display for DepInfoProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(kind) => write!(formatter, "unreadable: {kind}"),
            Self::NoRuleFor(binary) => {
                write!(formatter, "no rule has {} as its target", binary.display())
            }
            Self::RelativeSource(source) => {
                write!(formatter, "the source {} is relative", source.display())
            }
        }
    }
}

/// Why the oracle will not drive, or not trust, a runtime.
#[derive(Debug, Eq, PartialEq)]
enum RuntimeRefusal {
    /// `CARGO_TARGET_DIR` is relative: Cargo resolved it against the
    /// directory it ran in, which a test process cannot know.
    RelativeTargetDirectory(PathBuf),
    /// The dev profile directory lacks a binary the oracle drives.
    MissingBinary(PathBuf),
    /// The binary has no dep-info file beside it, so nothing names the
    /// sources it was built from and its freshness cannot be shown.
    MissingDepInfo { binary: PathBuf, dep_info: PathBuf },
    /// The dep-info file beside the binary is not the rule cargo writes.
    UnreadableDepInfo {
        dep_info: PathBuf,
        problem: DepInfoProblem,
    },
    /// The binary was linked after its dep-info file was written: a build
    /// that only needed it (a test build) linked it there, not `cargo build`.
    LinkedAfterItsDepInfo { binary: PathBuf, dep_info: PathBuf },
    /// A source the binary was built from changed after it was linked:
    /// `cargo build` would rebuild it.
    BinaryOlderThanSource { binary: PathBuf, source: PathBuf },
    /// A source the binary was built from cannot be read any more (deleted
    /// or renamed): `cargo build` would rebuild it (`crates/neomacs/build.rs`
    /// notes that cargo treats a watched path that names nothing as changed).
    SourceUnavailable { binary: PathBuf, source: PathBuf },
    /// `pdumper-stats` is nil: the editor bootstrapped from Lisp sources.
    NoImageLoaded,
    /// The editor that loaded the image was built after it was dumped.
    ImageOlderThanEditor { image: PathBuf, editor: PathBuf },
    /// A Lisp file the editor has loaded changed after the image was dumped;
    /// an image keeps the definitions of every file it preloaded.
    ImageOlderThanLisp { image: PathBuf, lisp: PathBuf },
    /// The copy of the editor the image recipe dumps with is not beside the
    /// editor, so nothing shows which build dumped the image.
    NoImageDumper { image: PathBuf, dumper: PathBuf },
    /// The copy that dumped the image holds other bytes than the editor.
    ImageDumpedByAnotherEditor {
        image: PathBuf,
        dumper: PathBuf,
        editor: PathBuf,
    },
    /// The copy the image is dumped with changed after the image was dumped.
    ImageOlderThanDumper { image: PathBuf, dumper: PathBuf },
}

impl fmt::Display for RuntimeRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativeTargetDirectory(directory) => write!(
                formatter,
                "{TARGET_DIRECTORY_VARIABLE}={} is relative; Cargo resolved it against the \
                 directory it ran in, which this test cannot know: make it absolute",
                directory.display()
            ),
            Self::MissingBinary(program) => {
                write!(formatter, "{} is missing: {REBUILD}", program.display())
            }
            Self::MissingDepInfo { binary, dep_info } => write!(
                formatter,
                "{} has no dep-info file {}; cargo writes one beside every binary it builds, \
                 naming the sources it was built from, so this binary's freshness cannot be \
                 shown: {REBUILD}",
                binary.display(),
                dep_info.display()
            ),
            Self::UnreadableDepInfo { dep_info, problem } => write!(
                formatter,
                "{} is not the dep-info rule cargo writes ({problem}): {REBUILD}",
                dep_info.display()
            ),
            Self::LinkedAfterItsDepInfo { binary, dep_info } => write!(
                formatter,
                "{} was linked after cargo wrote {}, so `cargo build` did not link it last: a \
                 build that only needed it did, such as `cargo nextest run -p neomacs`, whose \
                 test build has neomacs's dev-dependency features and the test profile, so this \
                 run would observe another product: {REBUILD}",
                binary.display(),
                dep_info.display()
            ),
            Self::BinaryOlderThanSource { binary, source } => write!(
                formatter,
                "{} is older than {}, which its dep-info names as a source; `cargo build` \
                 would rebuild it, so this run would observe the previous build: {REBUILD}",
                binary.display(),
                source.display()
            ),
            Self::SourceUnavailable { binary, source } => write!(
                formatter,
                "{}, a source the dep-info of {} names, cannot be read; `cargo build` would \
                 rebuild the binary: {REBUILD}",
                source.display(),
                binary.display()
            ),
            Self::NoImageLoaded => write!(
                formatter,
                "the editor reports no runtime image (`pdumper-stats` is nil), so it \
                 bootstrapped from Lisp sources: {REBUILD}"
            ),
            Self::ImageOlderThanEditor { image, editor } => write!(
                formatter,
                "the runtime image {} is older than the editor {} that loaded it; a dev \
                 build loads an image dumped by any build, so this run would observe stale \
                 code: {REBUILD}",
                image.display(),
                editor.display()
            ),
            Self::ImageOlderThanLisp { image, lisp } => write!(
                formatter,
                "the runtime image {} is older than {}, which the editor has loaded; an image \
                 keeps the definitions of every file it preloaded, so this run may observe \
                 stale Lisp: {REBUILD}",
                image.display(),
                lisp.display()
            ),
            Self::NoImageDumper { image, dumper } => write!(
                formatter,
                "{} is missing: the image recipe dumps {} with that copy of the editor and \
                 leaves it, and nothing else shows which build dumped the image: {REBUILD}",
                dumper.display(),
                image.display()
            ),
            Self::ImageDumpedByAnotherEditor {
                image,
                dumper,
                editor,
            } => write!(
                formatter,
                "the runtime image {} was dumped by {}, whose bytes are not those of the editor \
                 {}: another build dumped it (`daemon_lifecycle`'s fixture dumps with the test \
                 build `cargo nextest run -p neomacs` links), and a dev build loads it without \
                 complaint. The recipe reuses an image and a copy that are not older than the \
                 editor, so remove both, then {REBUILD}",
                image.display(),
                dumper.display(),
                editor.display()
            ),
            Self::ImageOlderThanDumper { image, dumper } => write!(
                formatter,
                "the runtime image {} is older than {}, the copy of the editor the recipe dumps \
                 it with, so that copy did not dump it: {REBUILD}",
                image.display(),
                dumper.display()
            ),
        }
    }
}

/// The dev profile directory: `$CARGO_TARGET_DIR/debug` when the variable is
/// set and non-empty, as `neomacs_infra::melpa_cache_root` resolves the target
/// directory, else `<workspace>/target/debug`.
fn dev_profile_directory(
    target_directory: Option<OsString>,
    workspace: &Path,
) -> Result<PathBuf, RuntimeRefusal> {
    let target = match target_directory {
        Some(value) if !value.is_empty() => {
            let directory = PathBuf::from(value);
            if !directory.is_absolute() {
                return Err(RuntimeRefusal::RelativeTargetDirectory(directory));
            }
            directory
        }
        _ => workspace.join("target"),
    };
    Ok(target.join("debug"))
}

/// A binary the oracle drives, shown to be the one `cargo build` linked last,
/// from sources none of which is newer than it.
struct BuiltBinary {
    path: PathBuf,
    /// How many sources its dep-info names; none is newer than the binary.
    sources: usize,
}

struct Binaries {
    editor: BuiltBinary,
    client: BuiltBinary,
}

impl Binaries {
    /// `neomacs` and `neomacsclient` as `cargo build -p neomacs --bins
    /// --features dbus-vendored` leaves them in the dev profile directory,
    /// each refused unless that build linked it from the current sources.
    fn dev() -> Self {
        let directory = dev_profile_directory(
            std::env::var_os(TARGET_DIRECTORY_VARIABLE),
            &neomacs_infra::workspace_root(),
        )
        .unwrap_or_else(|refusal| panic!("{refusal}"));
        let built = |name: &str| {
            let path = directory.join(name);
            if !path.is_file() {
                panic!("{}", RuntimeRefusal::MissingBinary(path));
            }
            let sources =
                built_from_current_sources(&path).unwrap_or_else(|refusal| panic!("{refusal}"));
            BuiltBinary { path, sources }
        };
        Self {
            editor: built("neomacs"),
            client: built("neomacsclient"),
        }
    }
}

/// Split one dep-info line into paths. Paths are separated by spaces; as in
/// any Makefile, a space inside a path is written `\ `.
fn dep_info_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' if characters.peek() == Some(&' ') => {
                word.push(' ');
                characters.next();
            }
            ' ' => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            other => word.push(other),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The sources of `binary` as its dep-info rule names them. Cargo writes this
/// rule, `<binary>: <source> <source> ...`, beside every binary it builds: "all
/// of the file dependencies required to rebuild the artifact", absolute unless
/// `build.dep-info-basedir` is set (the Cargo Book, "Build cache", dep-info
/// files). `target/debug/neomacs.d` names every workspace Rust source the
/// editor is compiled from and every path its build scripts watch; registry
/// crates, which cannot change in place, are not in it.
fn dep_info_sources(text: &str, binary: &Path) -> Result<Vec<PathBuf>, DepInfoProblem> {
    for line in text.lines() {
        let mut words = dep_info_words(line).into_iter();
        let Some(target) = words.next() else {
            continue;
        };
        let Some(target) = target.strip_suffix(':') else {
            continue;
        };
        if Path::new(target) != binary {
            continue;
        }
        return words
            .map(|word| {
                let source = PathBuf::from(word);
                if source.is_absolute() {
                    Ok(source)
                } else {
                    Err(DepInfoProblem::RelativeSource(source))
                }
            })
            .collect();
    }
    Err(DepInfoProblem::NoRuleFor(binary.to_path_buf()))
}

/// The newest modification time at or below `path`, and where it was found.
/// A directory counts with everything beneath it, symbolic links followed and
/// each directory visited once, because cargo scans the whole of a directory a
/// build script watches (the Cargo Book, "Build scripts", `rerun-if-changed`).
/// `crates/neomacs/build.rs` watches every tracked top-level entry of the
/// worktree that way, `crates/` and `lisp/` among them. An entry that cannot
/// be read while its directory is scanned is skipped.
fn newest_modification(path: &Path) -> std::io::Result<(SystemTime, PathBuf)> {
    let metadata = std::fs::metadata(path)?;
    let mut newest = (metadata.modified()?, path.to_path_buf());
    if !metadata.is_dir() {
        return Ok(newest);
    }
    let mut visited = HashSet::from([(metadata.dev(), metadata.ino())]);
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            // A symbolic link counts with its own time and its target's.
            let own = entry.metadata();
            let target = std::fs::metadata(&entry_path);
            for modified in [&own, &target]
                .into_iter()
                .flatten()
                .filter_map(|metadata| metadata.modified().ok())
            {
                if modified > newest.0 {
                    newest = (modified, entry_path.clone());
                }
            }
            if let Ok(target) = &target
                && target.is_dir()
                && visited.insert((target.dev(), target.ino()))
            {
                pending.push(entry_path);
            }
        }
    }
    Ok(newest)
}

/// Show that `cargo build` linked `binary` last, and that it is not older
/// than any source its dep-info file (`<binary>.d`) names.
///
/// Cargo writes that file after linking the binary, on every build that names
/// the binary (`cargo build -p neomacs --bins`), even one that compiles
/// nothing. A build that only needs the binary (`cargo nextest run -p
/// neomacs`, whose integration tests run it) links its own product to the
/// same name and leaves the file as it was; that product's modification time
/// is often older than the file's (observed with cargo 1.96.1, M0 fix round
/// 3). Linking sets the inode's change time, which no tool can set back, so a
/// binary whose inode changed after the file was written was linked by such
/// a build.
///
/// Then cargo's rebuild rule: a source changed after the binary was linked,
/// or one that can no longer be read, means `cargo build` would rebuild the
/// binary. Equal times count as fresh, as `image_is_older_than` counts them.
/// Cargo measures from the start of the build that produced the binary, this
/// from the binary's own time, so an edit made while that build ran goes
/// unseen. Returns how many sources were checked.
fn built_from_current_sources(binary: &Path) -> Result<usize, RuntimeRefusal> {
    let dep_info = binary.with_extension("d");
    let text = std::fs::read_to_string(&dep_info).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => RuntimeRefusal::MissingDepInfo {
            binary: binary.to_path_buf(),
            dep_info: dep_info.clone(),
        },
        kind => RuntimeRefusal::UnreadableDepInfo {
            dep_info: dep_info.clone(),
            problem: DepInfoProblem::Unreadable(kind),
        },
    })?;
    let sources =
        dep_info_sources(&text, binary).map_err(|problem| RuntimeRefusal::UnreadableDepInfo {
            dep_info: dep_info.clone(),
            problem,
        })?;
    // The caller has seen the binary; a binary whose time cannot be read is
    // as good as missing.
    let linked = std::fs::metadata(binary)
        .map_err(|_| RuntimeRefusal::MissingBinary(binary.to_path_buf()))?;
    let written =
        std::fs::metadata(&dep_info).map_err(|error| RuntimeRefusal::UnreadableDepInfo {
            dep_info: dep_info.clone(),
            problem: DepInfoProblem::Unreadable(error.kind()),
        })?;
    if (linked.ctime(), linked.ctime_nsec()) > (written.mtime(), written.mtime_nsec()) {
        return Err(RuntimeRefusal::LinkedAfterItsDepInfo {
            binary: binary.to_path_buf(),
            dep_info,
        });
    }
    let built = linked
        .modified()
        .map_err(|_| RuntimeRefusal::MissingBinary(binary.to_path_buf()))?;
    for source in &sources {
        let (modified, newest) =
            newest_modification(source).map_err(|_| RuntimeRefusal::SourceUnavailable {
                binary: binary.to_path_buf(),
                source: source.clone(),
            })?;
        if modified > built {
            return Err(RuntimeRefusal::BinaryOlderThanSource {
                binary: binary.to_path_buf(),
                source: newest,
            });
        }
    }
    Ok(sources.len())
}

/// Why `neomacsclient -e` output is not the printed string the oracle asked for.
#[derive(Debug, Eq, PartialEq)]
enum PrintedStringError {
    NotQuoted,
    UnquotedDoubleQuote,
    /// An escape the oracle does not decode; refused rather than guessed.
    UnknownEscape(Option<char>),
}

/// Decode a string value as `neomacsclient -e` prints it: in double quotes,
/// with `\\`, `\"` and `\n` escaped.
fn read_printed_string(printed: &str) -> Result<String, PrintedStringError> {
    let inner = printed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or(PrintedStringError::NotQuoted)?;
    let mut text = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => match characters.next() {
                Some('\\') => text.push('\\'),
                Some('"') => text.push('"'),
                Some('n') => text.push('\n'),
                other => return Err(PrintedStringError::UnknownEscape(other)),
            },
            '"' => return Err(PrintedStringError::UnquotedDoubleQuote),
            other => text.push(other),
        }
    }
    Ok(text)
}

/// The runtime as the running editor reports it ([`RUNTIME_REPORT_FORM`]),
/// not as file names beside the binary suggest.
#[derive(Debug, Eq, PartialEq)]
struct ReportedRuntime {
    image: Option<PathBuf>,
    loaded_lisp: Vec<PathBuf>,
}

impl ReportedRuntime {
    fn parse(printed: &str) -> Result<Self, PrintedStringError> {
        let report = read_printed_string(printed)?;
        let mut lines = report.split('\n');
        let image = lines
            .next()
            .filter(|line| !line.is_empty())
            .map(PathBuf::from);
        let loaded_lisp = lines
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect();
        Ok(Self { image, loaded_lisp })
    }

    /// The loaded image, once it is shown not to predate any of its inputs:
    /// the editor that loaded it and every Lisp file that editor has loaded.
    /// One rule for all of them, `image_is_older_than`, the rule the image
    /// fixture provisions by. A Lisp file the image did not preload also
    /// counts: the editor cannot say which ones it did, and a refusal costs a
    /// re-dump while a stale pass costs a wrong baseline.
    fn fresh_image(&self, editor: &Path) -> Result<&Path, RuntimeRefusal> {
        use neomacs_infra::runtime_image::image_is_older_than;
        let image = self.image.as_deref().ok_or(RuntimeRefusal::NoImageLoaded)?;
        if image_is_older_than(image, editor) {
            return Err(RuntimeRefusal::ImageOlderThanEditor {
                image: image.to_path_buf(),
                editor: editor.to_path_buf(),
            });
        }
        match self
            .loaded_lisp
            .iter()
            .find(|lisp| image_is_older_than(image, lisp))
        {
            Some(lisp) => Err(RuntimeRefusal::ImageOlderThanLisp {
                image: image.to_path_buf(),
                lisp: lisp.clone(),
            }),
            None => Ok(image),
        }
    }
}

/// The copy of the editor that dumped `image`, once it is shown to hold the
/// editor's bytes. The image recipe copies the editor beside itself as
/// [`DUMPER_NAME`], runs that copy to dump the image, and leaves it there.
/// Nothing else ties an image to the build that dumped it: a dev build loads
/// an image any build dumped, and the time rule of
/// [`ReportedRuntime::fresh_image`] passes an image that `daemon_lifecycle`'s
/// fixture dumped with the test build once `cargo build` has linked the dev
/// editor back with its older time. So the copy must hold the editor's
/// bytes, and the image must not be older than the copy.
fn image_dumper(image: &Path, editor: &Path) -> Result<PathBuf, RuntimeRefusal> {
    use neomacs_infra::runtime_image::image_is_older_than;
    let dumper = editor.with_file_name(DUMPER_NAME);
    if !dumper.is_file() {
        return Err(RuntimeRefusal::NoImageDumper {
            image: image.to_path_buf(),
            dumper,
        });
    }
    if !same_contents(&dumper, editor) {
        return Err(RuntimeRefusal::ImageDumpedByAnotherEditor {
            image: image.to_path_buf(),
            dumper,
            editor: editor.to_path_buf(),
        });
    }
    if image_is_older_than(image, &dumper) {
        return Err(RuntimeRefusal::ImageOlderThanDumper {
            image: image.to_path_buf(),
            dumper,
        });
    }
    Ok(dumper)
}

/// Whether two files hold the same bytes.
fn same_contents(left: &Path, right: &Path) -> bool {
    let length = |path: &Path| {
        std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    };
    length(left) == length(right) && hash(left) == hash(right)
}

/// One executable the oracle ran: where, which bytes, what it says it is.
struct ExecutableRecord {
    path: PathBuf,
    sha256: String,
    version: String,
    sources: usize,
}

impl ExecutableRecord {
    fn of(isolation: &Isolation, binary: &BuiltBinary) -> Self {
        let program = binary.path.as_path();
        let mut command = isolation.command(program);
        command.arg("--version");
        let output = run_bounded(&mut command, EVAL_BUDGET, "--version");
        assert!(
            output.status.success(),
            "{} --version failed ({}):\n{}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let version = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        Self {
            path: program.to_path_buf(),
            sha256: hash(program),
            version,
            sources: binary.sources,
        }
    }
}

fn hash(path: &Path) -> String {
    sha256_file(path).unwrap_or_else(|error| panic!("hash {}: {error}", path.display()))
}

/// What one run drove: written as `provenance` beside its files (never
/// compared) and printed in the test output.
struct RunProvenance {
    editor: ExecutableRecord,
    client: ExecutableRecord,
    image: PathBuf,
    image_sha256: String,
    image_dumper: PathBuf,
    loaded_lisp: usize,
}

impl fmt::Display for RunProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (role, record) in [("editor", &self.editor), ("client", &self.client)] {
            writeln!(formatter, "{role}: {}", record.path.display())?;
            writeln!(formatter, "{role}-sha256: {}", record.sha256)?;
            writeln!(formatter, "{role}-version: {}", record.version)?;
            writeln!(
                formatter,
                "{role}-freshness: linked before its dep-info was written, not older than the \
                 {} sources it names",
                record.sources
            )?;
        }
        writeln!(
            formatter,
            "image: {} (the editor's pdumper-stats)",
            self.image.display()
        )?;
        writeln!(formatter, "image-sha256: {}", self.image_sha256)?;
        writeln!(
            formatter,
            "image-dumper: {}, byte-identical to the editor",
            self.image_dumper.display()
        )?;
        writeln!(
            formatter,
            "image-freshness: not older than the editor, its dumper or the {} Lisp files it \
             has loaded",
            self.loaded_lisp
        )
    }
}

/// The daemon of the client arrangement; killed if a step panics.
struct DaemonProcess(Child);

impl DaemonProcess {
    fn wait_for_socket(&mut self, isolation: &Isolation, server_name: &str, log: &Path) {
        let socket = isolation.socket(server_name);
        let deadline = Instant::now() + STARTUP_BUDGET;
        while !socket.exists() {
            if let Some(status) = self.0.try_wait().expect("wait for the daemon") {
                panic!(
                    "the daemon exited before readiness ({status}):\n{}",
                    std::fs::read_to_string(isolation.path("daemon.out")).unwrap_or_default()
                );
            }
            assert!(
                Instant::now() < deadline,
                "no server socket at {} within {STARTUP_BUDGET:?}; {}",
                socket.display(),
                startup_hint(log)
            );
            std::thread::sleep(POLL);
        }
    }

    /// The daemon's exit status, once it exits within `budget`.
    fn wait_for_exit(&mut self, budget: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + budget;
        loop {
            if let Some(status) = self.0.try_wait().expect("wait for the daemon") {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(POLL);
        }
    }
}

/// Where to look when an editor never became ready.
fn startup_hint(log: &Path) -> String {
    format!(
        "an editor without a runtime image bootstraps from Lisp sources for minutes; \
         its log is {}",
        log.display()
    )
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// Where the editor that serves `-e` requests runs.
enum Server {
    Daemon(DaemonProcess),
    /// The pty process itself (`-nw`).
    Primary,
}

fn read_to_end(mut pipe: impl std::io::Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = pipe.read_to_end(&mut bytes);
    bytes
}

/// Run a non-interactive command to completion within `budget`.
fn run_bounded(command: &mut PinnedCommand, budget: Duration, what: &str) -> Output {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("{what}: spawn failed: {error}"));
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout = std::thread::spawn(move || read_to_end(stdout));
    let stderr = std::thread::spawn(move || read_to_end(stderr));
    let deadline = Instant::now() + budget;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for a client") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what}: no answer within {budget:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: stdout.join().expect("stdout reader"),
        stderr: stderr.join().expect("stderr reader"),
    }
}

fn lisp_string(path: &Path) -> String {
    let text = path.to_str().expect("the scenario root is UTF-8");
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        if matches!(character, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('"');
    quoted
}

/// What one recorded step produced.
struct StepRecord {
    stem: String,
    screen: CanonicalScreenDump,
    layout: Option<String>,
    pty: Vec<u8>,
}

/// The two things the oracle observes of a running arrangement.
trait Observer {
    /// The visible screen once it has stayed unchanged for [`QUIESCENCE`].
    fn settled_screen(&mut self, context: &str) -> CanonicalScreenDump;
    /// The selected frame's `text-faces` snapshot. The snapshot form forces a
    /// redisplay before it lays the frame out, and that redisplay paints the
    /// terminal.
    fn layout_snapshot(&mut self, stem: &str) -> String;
}

/// The recorded observations of one step.
#[derive(Debug, Eq, PartialEq)]
struct Observed {
    screen: CanonicalScreenDump,
    layout: Option<String>,
}

/// A step whose own redisplay left the terminal stale: the screen changed
/// under the redisplay the layout snapshot forced.
#[derive(Debug, Eq, PartialEq)]
struct UnpaintedStep {
    painted: CanonicalScreenDump,
    repainted: CanonicalScreenDump,
}

impl fmt::Display for UnpaintedStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the screen changed under the layout snapshot's forced redisplay, so the step's \
             own redisplay did not paint what the editor lays out:\n{}",
            describe_difference(self.painted.as_str(), self.repainted.as_str())
        )
    }
}

/// Observe one step: the screen as the step itself painted it, then the
/// layout snapshot, then the screen again, which must not have changed.
fn observe(
    observer: &mut impl Observer,
    stem: &str,
    context: &str,
    observation: Observation,
) -> Result<Observed, UnpaintedStep> {
    let screen = observer.settled_screen(context);
    let layout = match observation {
        Observation::Screen => None,
        Observation::ScreenAndLayout => {
            let layout = observer.layout_snapshot(stem);
            let repainted = observer.settled_screen(context);
            if repainted != screen {
                return Err(UnpaintedStep {
                    painted: screen,
                    repainted,
                });
            }
            Some(layout)
        }
    };
    Ok(Observed { screen, layout })
}

/// A started arrangement: its pty, its server and its isolation.
struct Run {
    arrangement: Arrangement,
    session: TuiSession,
    // Dropped after the session, so a client never outlives its daemon by
    // more than the session's own cleanup.
    server: Server,
    binaries: Binaries,
    isolation: Isolation,
}

impl Run {
    fn start(arrangement: Arrangement, output: &Path) -> Self {
        let log_file = output.join(EDITOR_LOG_FILE);
        let log_file = log_file.as_path();
        let binaries = Binaries::dev();
        let isolation = Isolation::new(arrangement);
        let terminal = TuiTerminalConfig::new(TERMINAL_TYPE, ROWS, COLUMNS);
        let server_name = arrangement.server_name();
        let (session, server) = match arrangement {
            Arrangement::DaemonTtyClient => {
                let daemon_output = std::fs::File::create(isolation.path("daemon.out"))
                    .expect("create the daemon's output file");
                let mut command = isolation.command(&binaries.editor.path);
                command
                    .arg("-Q")
                    .arg(format!("--fg-daemon={server_name}"))
                    .env("NEOMACS_LOG_FILE", log_file)
                    .stdin(Stdio::null())
                    .stdout(
                        daemon_output
                            .try_clone()
                            .expect("share the daemon's output file"),
                    )
                    .stderr(daemon_output);
                let mut daemon = DaemonProcess(command.spawn().expect("spawn the daemon"));
                daemon.wait_for_socket(&isolation, server_name, log_file);
                let launch =
                    isolation
                        .launch(&binaries.client.path)
                        .args(["-s", server_name, "-t"]);
                let session =
                    TuiSession::spawn_launch_on_terminal(launch, "seam-tty-client", terminal);
                (session, Server::Daemon(daemon))
            }
            Arrangement::PrimaryTty => {
                let launch = isolation
                    .launch(&binaries.editor.path)
                    .args(["-Q", "-nw", "--eval"])
                    .arg(primary_server_form())
                    .env("NEOMACS_LOG_FILE", log_file);
                let mut session =
                    TuiSession::spawn_launch_on_terminal(launch, "seam-primary-tty", terminal);
                // Pumping the pty also fails fast if the editor exits first.
                let socket = isolation.socket(server_name);
                session.read_until(STARTUP_BUDGET, |_| socket.exists());
                assert!(
                    socket.exists(),
                    "no server socket at {} within {STARTUP_BUDGET:?}; {}",
                    socket.display(),
                    startup_hint(log_file)
                );
                (session, Server::Primary)
            }
        };
        let mut run = Self {
            arrangement,
            session,
            server,
            binaries,
            isolation,
        };
        assert_eq!(run.eval("(+ 20 22)"), "42", "server round trip");
        let provenance = run.provenance();
        std::fs::write(output.join(PROVENANCE_FILE), provenance.to_string())
            .unwrap_or_else(|error| panic!("write the run provenance: {error}"));
        eprintln!("{arrangement:?} run provenance:\n{provenance}");
        let frame_ready = ScreenState::Shows("*scratch*");
        run.session
            .read_until(STARTUP_BUDGET, |grid| frame_ready.holds(grid));
        assert!(
            frame_ready.holds(&run.session.text_grid()),
            "{:?}: the frame never showed *scratch*:\n{}",
            arrangement,
            run.session.text_grid().join("\n")
        );
        run
    }

    fn eval(&self, form: &str) -> String {
        let mut command = self.isolation.command(&self.binaries.client.path);
        command.args(["-s", self.arrangement.server_name(), "-w", "30", "-e", form]);
        let output = run_bounded(&mut command, EVAL_BUDGET, form);
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr.trim().is_empty(),
            "{:?}: `{form}` failed ({}):\nstdout: {stdout}\nstderr: {stderr}",
            self.arrangement,
            output.status
        );
        stdout
    }

    /// Identify what this run drives, refusing an image that predates the
    /// editor or the Lisp it has loaded ([`ReportedRuntime::fresh_image`]) or
    /// that another build dumped ([`image_dumper`]).
    fn provenance(&self) -> RunProvenance {
        let printed = self.eval(RUNTIME_REPORT_FORM);
        let runtime = ReportedRuntime::parse(&printed).unwrap_or_else(|error| {
            panic!("the editor's runtime report is unreadable ({error:?}): {printed}")
        });
        let image = runtime
            .fresh_image(&self.binaries.editor.path)
            .unwrap_or_else(|refusal| panic!("{:?}: {refusal}", self.arrangement));
        let dumper = image_dumper(image, &self.binaries.editor.path)
            .unwrap_or_else(|refusal| panic!("{:?}: {refusal}", self.arrangement));
        RunProvenance {
            editor: ExecutableRecord::of(&self.isolation, &self.binaries.editor),
            client: ExecutableRecord::of(&self.isolation, &self.binaries.client),
            image: image.to_path_buf(),
            image_sha256: hash(image),
            image_dumper: dumper,
            loaded_lisp: runtime.loaded_lisp.len(),
        }
    }

    fn record(&mut self, index: usize, step: &Step) -> StepRecord {
        let stem = format!("{index:02}-{}", step.name);
        let context = format!("{:?} step {stem}", self.arrangement);
        self.session.clear_recent_output();
        match step.action {
            Action::Eval(form) => {
                self.eval(form);
            }
            Action::Keys { keys, until } => {
                self.session.send_keys(keys);
                self.session
                    .read_until(STEP_BUDGET, |grid| until.holds(grid));
                assert!(
                    until.holds(&self.session.text_grid()),
                    "{context}: the screen never reached {until:?}:\n{}",
                    self.session.text_grid().join("\n")
                );
            }
        }
        let observed = observe(self, &stem, &context, step.observation)
            .unwrap_or_else(|unpainted| panic!("{context}: {unpainted}"));
        StepRecord {
            stem,
            screen: observed.screen,
            layout: observed.layout,
            pty: self.session.recent_output().to_vec(),
        }
    }

    /// The scenario's exit step ([`EXIT`]): the screen the pty process leaves
    /// behind. A daemon resets its client's terminal while it deletes the
    /// terminal, which `server-delete-client` does before it closes the
    /// client's connection, so the reset reaches the terminal before the
    /// client exits. A reset written only after the client has exited would
    /// reach a terminal the user's shell already has back; it is not part of
    /// this record.
    fn leave(&mut self) -> StepRecord {
        let stem = EXIT.stem();
        self.session.clear_recent_output();
        let screen = leave_terminal(&mut self.session, EXIT.keys, STEP_BUDGET)
            .unwrap_or_else(|running| panic!("{:?} step {stem}: {running}", self.arrangement));
        StepRecord {
            stem,
            screen,
            layout: None,
            pty: self.session.recent_output().to_vec(),
        }
    }

    /// Stop the daemon the client left running. Under `-nw` the pty process
    /// was the editor, and it has exited.
    fn stop_server(mut self) {
        let Server::Daemon(daemon) = &mut self.server else {
            return;
        };
        // The server may close the connection before it answers.
        let mut command = self.isolation.command(&self.binaries.client.path);
        command.args(["-s", self.arrangement.server_name(), "-e", "(kill-emacs)"]);
        let _ = run_bounded(&mut command, EVAL_BUDGET, "(kill-emacs)");
        match daemon.wait_for_exit(STEP_BUDGET) {
            Some(status) => eprintln!("{:?}: the daemon exited ({status})", self.arrangement),
            None => panic!(
                "{:?}: the daemon did not exit within {STEP_BUDGET:?} of (kill-emacs)",
                self.arrangement
            ),
        }
    }
}

/// A pty process that did not exit within the budget after the exit keys.
#[derive(Debug, Eq, PartialEq)]
struct StillRunning {
    budget: Duration,
    /// The terminal as the process left it when it was terminated.
    screen: CanonicalScreenDump,
}

impl fmt::Display for StillRunning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the pty process did not exit within {:?} of the exit keys; it was terminated \
             with the terminal showing:\n{}",
            self.budget,
            self.screen.as_str()
        )
    }
}

/// Type `keys` on the session's terminal, wait up to `budget` for its process
/// to exit, and return the screen it leaves behind: everything written to the
/// terminal before the process exited, read and parsed.
fn leave_terminal(
    session: &mut TuiSession,
    keys: &str,
    budget: Duration,
) -> Result<CanonicalScreenDump, StillRunning> {
    session.send_keys(keys);
    let outcome = session.run_to_completion(budget);
    let screen = CanonicalScreenDump::capture(session.screen());
    match outcome {
        TuiProcessOutcome::Exited => Ok(screen),
        TuiProcessOutcome::TimedOut => Err(StillRunning { budget, screen }),
    }
}

impl Observer for Run {
    fn settled_screen(&mut self, context: &str) -> CanonicalScreenDump {
        let deadline = Instant::now() + STEP_BUDGET;
        let mut current = CanonicalScreenDump::capture(self.session.screen());
        let mut unchanged_since = Instant::now();
        loop {
            self.session.read(POLL);
            let observed = CanonicalScreenDump::capture(self.session.screen());
            if observed != current {
                current = observed;
                unchanged_since = Instant::now();
            } else if unchanged_since.elapsed() >= QUIESCENCE {
                return current;
            }
            assert!(
                Instant::now() < deadline,
                "{context}: the screen never stayed unchanged for {QUIESCENCE:?}:\n{}",
                current.as_str()
            );
        }
    }

    fn layout_snapshot(&mut self, stem: &str) -> String {
        let file = self
            .isolation
            .path("tmp")
            .join(format!("{stem}.text-faces"));
        let form = format!(
            "(neomacs--write-frame-snapshot {} nil 'text-faces)",
            lisp_string(&file)
        );
        assert_eq!(self.eval(&form), "t", "{stem}: snapshot form");
        std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("{stem}: read {}: {error}", file.display()))
    }
}

fn write_step(directory: &Path, record: &StepRecord) {
    let write = |extension: &str, bytes: &[u8]| {
        let path = directory.join(format!("{}.{extension}", record.stem));
        std::fs::write(&path, bytes)
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    };
    write("screen", record.screen.as_str().as_bytes());
    if let Some(layout) = &record.layout {
        write("text-faces", layout.as_bytes());
    }
    write("pty", &record.pty);
}

/// The gated files of a recorded run: screens and layout snapshots, by name.
fn gated_files(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| {
            entry
                .expect("baseline directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".screen") || name.ends_with(".text-faces"))
        .collect();
    names.sort();
    names
}

/// The first differing lines of two recorded files, for the failure report.
fn describe_difference(expected: &str, actual: &str) -> String {
    const SHOWN: usize = 12;
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();
    let mut report = String::new();
    let mut shown = 0;
    for line in 0..expected.len().max(actual.len()) {
        let before = expected.get(line).copied();
        let after = actual.get(line).copied();
        if before == after {
            continue;
        }
        let _ = writeln!(report, "    line {}:", line + 1);
        let _ = writeln!(report, "      - {}", before.unwrap_or("<absent>"));
        let _ = writeln!(report, "      + {}", after.unwrap_or("<absent>"));
        shown += 1;
        if shown == SHOWN {
            let _ = writeln!(report, "    ...");
            break;
        }
    }
    report
}

fn compare_runs(expected: &Path, actual: &Path) -> Vec<String> {
    let expected_files = gated_files(expected);
    let actual_files = gated_files(actual);
    let mut differences = Vec::new();
    for name in &expected_files {
        if !actual_files.contains(name) {
            differences.push(format!("{name}: recorded at baseline, absent now"));
        }
    }
    for name in &actual_files {
        if !expected_files.contains(name) {
            differences.push(format!("{name}: absent at baseline"));
            continue;
        }
        let read = |directory: &Path| {
            std::fs::read_to_string(directory.join(name))
                .unwrap_or_else(|error| panic!("read {name}: {error}"))
        };
        let (before, after) = (read(expected), read(actual));
        if before != after {
            differences.push(format!(
                "{name} differs:\n{}",
                describe_difference(&before, &after)
            ));
        }
    }
    differences
}

fn write_scenario_file(baseline: &Path, arrangement: Arrangement) {
    let staged = baseline.join(format!(
        "{SCENARIO_FILE}.{}.partial",
        arrangement.directory_name()
    ));
    std::fs::write(&staged, scenario_text()).expect("stage scenario.el");
    std::fs::rename(&staged, baseline.join(SCENARIO_FILE)).expect("install scenario.el");
}

fn recreate(directory: &Path) {
    if directory.exists() {
        std::fs::remove_dir_all(directory)
            .unwrap_or_else(|error| panic!("remove {}: {error}", directory.display()));
    }
    std::fs::create_dir_all(directory)
        .unwrap_or_else(|error| panic!("create {}: {error}", directory.display()));
}

fn run_scenario(arrangement: Arrangement, output: &Path) {
    let mut run = Run::start(arrangement, output);
    for (index, step) in SCENARIO.iter().enumerate() {
        if arrangement.runs(step.coverage) {
            let record = run.record(index, step);
            write_step(output, &record);
        }
    }
    let exit = run.leave();
    write_step(output, &exit);
    run.stop_server();
}

fn seam_baseline(arrangement: Arrangement) {
    let mode = BaselineMode::from_environment();
    let baseline = baseline_directory();
    let expected = baseline.join(arrangement.directory_name());
    match mode {
        BaselineMode::Capture => {
            // Replace the baseline only after a whole run: a refused runtime
            // or a failed step must not leave a partial one in its place.
            let staged = baseline.join(format!("{}.partial", arrangement.directory_name()));
            recreate(&staged);
            run_scenario(arrangement, &staged);
            if expected.exists() {
                std::fs::remove_dir_all(&expected)
                    .unwrap_or_else(|error| panic!("remove {}: {error}", expected.display()));
            }
            std::fs::rename(&staged, &expected)
                .unwrap_or_else(|error| panic!("install {}: {error}", expected.display()));
            write_scenario_file(&baseline, arrangement);
        }
        BaselineMode::Compare => {
            assert!(
                expected.is_dir(),
                "no baseline at {}; run with {MODE_VARIABLE}=capture first",
                expected.display()
            );
            let recorded = std::fs::read_to_string(baseline.join(SCENARIO_FILE))
                .expect("read the recorded scenario.el");
            assert!(
                recorded == scenario_text(),
                "the scenario changed since {} was captured; recapture deliberately",
                baseline.display()
            );
            let actual = baseline.join(format!("{}.actual", arrangement.directory_name()));
            recreate(&actual);
            run_scenario(arrangement, &actual);
            let differences = compare_runs(&expected, &actual);
            let provenance = |directory: &Path| {
                std::fs::read_to_string(directory.join(PROVENANCE_FILE))
                    .unwrap_or_else(|_| String::from("none recorded\n"))
            };
            assert!(
                differences.is_empty(),
                "{arrangement:?}: {} gated file(s) differ from the baseline \
                 (expected {}, actual {}):\n{}\nbaseline provenance:\n{}this run's provenance:\n{}",
                differences.len(),
                expected.display(),
                actual.display(),
                differences.join("\n"),
                provenance(&expected),
                provenance(&actual)
            );
        }
    }
}

#[test]
#[ignore = "drives target/debug editors; NEOMACS_SEAM_BASELINE=capture|compare (MILESTONES M0)"]
fn seam_baseline_daemon_tty_client() {
    seam_baseline(Arrangement::DaemonTtyClient);
}

#[test]
#[ignore = "drives target/debug editors; NEOMACS_SEAM_BASELINE=capture|compare (MILESTONES M0)"]
fn seam_baseline_primary_tty() {
    seam_baseline(Arrangement::PrimaryTty);
}

// ── The oracle's own rules (run by default; no editor needed) ──────────────

/// A small terminal screen showing `text`.
fn screen_showing(text: &str) -> CanonicalScreenDump {
    let mut parser = vt100::Parser::new(2, 24, 0);
    parser.process(text.as_bytes());
    CanonicalScreenDump::capture(parser.screen())
}

/// An editor whose terminal shows `shown`; `pending` reaches the terminal
/// only when a layout snapshot forces a redisplay.
struct ScriptedEditor {
    shown: CanonicalScreenDump,
    pending: Option<CanonicalScreenDump>,
    snapshots: usize,
}

impl Observer for ScriptedEditor {
    fn settled_screen(&mut self, _context: &str) -> CanonicalScreenDump {
        self.shown.clone()
    }

    fn layout_snapshot(&mut self, stem: &str) -> String {
        self.snapshots += 1;
        if let Some(repainted) = self.pending.take() {
            self.shown = repainted;
        }
        format!("layout of {stem}")
    }
}

#[test]
fn observe_records_the_screen_the_step_painted() {
    let split = screen_showing("two windows");
    let mut editor = ScriptedEditor {
        shown: split.clone(),
        pending: None,
        snapshots: 0,
    };
    let observed = observe(
        &mut editor,
        "02-split",
        "test",
        Observation::ScreenAndLayout,
    );
    assert_eq!(
        observed,
        Ok(Observed {
            screen: split,
            layout: Some(String::from("layout of 02-split")),
        })
    );
    assert_eq!(editor.snapshots, 1);
}

#[test]
fn observe_refuses_a_step_only_the_forced_redisplay_paints() {
    // After an `-e` request the editor leaves the terminal stale, and only
    // the redisplay the snapshot form forces paints the split.
    let stale = screen_showing("one window");
    let split = screen_showing("two windows");
    let mut editor = ScriptedEditor {
        shown: stale.clone(),
        pending: Some(split.clone()),
        snapshots: 0,
    };
    let observed = observe(
        &mut editor,
        "02-split",
        "test",
        Observation::ScreenAndLayout,
    );
    assert_eq!(
        observed,
        Err(UnpaintedStep {
            painted: stale,
            repainted: split,
        })
    );
}

#[test]
fn observe_takes_no_snapshot_while_a_menu_is_up() {
    let menu = screen_showing("Visit New File");
    let mut editor = ScriptedEditor {
        shown: menu.clone(),
        pending: Some(screen_showing("no menu")),
        snapshots: 0,
    };
    let observed = observe(&mut editor, "07-menu-open", "test", Observation::Screen);
    assert_eq!(
        observed,
        Ok(Observed {
            screen: menu,
            layout: None,
        })
    );
    assert_eq!(editor.snapshots, 0);
}

#[test]
fn dev_binaries_follow_cargo_target_dir() {
    assert_eq!(
        dev_profile_directory(
            Some(OsString::from("/elsewhere/target")),
            Path::new("/workspace")
        ),
        Ok(PathBuf::from("/elsewhere/target/debug"))
    );
}

#[test]
fn dev_binaries_default_to_the_workspace_target() {
    for unset in [None, Some(OsString::new())] {
        assert_eq!(
            dev_profile_directory(unset, Path::new("/workspace")),
            Ok(PathBuf::from("/workspace/target/debug"))
        );
    }
}

#[test]
fn dev_binaries_refuse_a_relative_cargo_target_dir() {
    assert_eq!(
        dev_profile_directory(Some(OsString::from("target")), Path::new("/workspace")),
        Err(RuntimeRefusal::RelativeTargetDirectory(PathBuf::from(
            "target"
        )))
    );
}

/// Non-empty files whose modification times are seconds after one instant.
struct Timeline {
    root: tempfile::TempDir,
    epoch: SystemTime,
}

impl Timeline {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("create a timeline directory"),
            epoch: SystemTime::now() - Duration::from_secs(3600),
        }
    }

    /// A file at `name` (directories created as needed) modified at `seconds`.
    fn file(&self, name: &str, seconds: u64) -> PathBuf {
        self.file_holding(name, name, seconds)
    }

    /// A file at `name` holding `contents`, modified at `seconds`.
    fn file_holding(&self, name: &str, contents: &str, seconds: u64) -> PathBuf {
        let path = self.root.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create a timeline directory");
        }
        std::fs::write(&path, contents).expect("write a timeline file");
        self.stamp(&path, seconds);
        path
    }

    /// Set the modification time of a file or a directory under the root.
    fn stamp(&self, path: &Path, seconds: u64) {
        std::fs::File::open(path)
            .and_then(|file| file.set_modified(self.epoch + Duration::from_secs(seconds)))
            .expect("set a timeline modification time");
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
}

/// The dep-info rule cargo writes beside `binary`, naming `sources`.
fn write_dep_info(binary: &Path, sources: &[&Path]) {
    let mut rule = format!("{}:", binary.display());
    for source in sources {
        rule.push(' ');
        rule.push_str(&source.display().to_string().replace(' ', "\\ "));
    }
    rule.push('\n');
    std::fs::write(binary.with_extension("d"), rule).expect("write a dep-info file");
}

#[test]
fn binary_accepts_sources_no_newer_than_itself() {
    let timeline = Timeline::new();
    let source = timeline.file("src/main.rs", 5);
    timeline.file("lisp/subr.el", 5);
    let watched = timeline.path("lisp");
    timeline.stamp(&watched, 5);
    // Cargo counts a source modified in the same instant as fresh.
    let same_instant = timeline.file("Cargo.lock", 10);
    let binary = timeline.file("neomacs", 10);
    write_dep_info(&binary, &[&source, &watched, &same_instant]);
    assert_eq!(built_from_current_sources(&binary), Ok(3));
}

#[test]
fn binary_refuses_a_source_changed_after_it_was_linked() {
    // M3 edits `neovm-display` and runs the seam oracle without
    // `cargo build -p neomacs --bins`.
    let timeline = Timeline::new();
    let unchanged = timeline.file("src/main.rs", 5);
    let binary = timeline.file("neomacs", 10);
    let edited = timeline.file("src with space/display.rs", 20);
    write_dep_info(&binary, &[&unchanged, &edited]);
    assert_eq!(
        built_from_current_sources(&binary),
        Err(RuntimeRefusal::BinaryOlderThanSource {
            binary,
            source: edited,
        })
    );
}

#[test]
fn binary_refuses_a_file_changed_inside_a_watched_directory() {
    // A build script watches `crates/` whole; editing a file in it leaves the
    // directory's own modification time as it was.
    let timeline = Timeline::new();
    let binary = timeline.file("neomacs", 10);
    let edited = timeline.file("crates/neovm-display/src/lib.rs", 20);
    for directory in ["crates/neovm-display/src", "crates/neovm-display", "crates"] {
        timeline.stamp(&timeline.path(directory), 5);
    }
    let watched = timeline.path("crates");
    write_dep_info(&binary, &[&watched]);
    assert_eq!(
        built_from_current_sources(&binary),
        Err(RuntimeRefusal::BinaryOlderThanSource {
            binary,
            source: edited,
        })
    );
}

#[test]
fn binary_refuses_a_source_that_no_longer_exists() {
    let timeline = Timeline::new();
    let binary = timeline.file("neomacs", 10);
    let removed = timeline.path("src/removed.rs");
    write_dep_info(&binary, &[&removed]);
    assert_eq!(
        built_from_current_sources(&binary),
        Err(RuntimeRefusal::SourceUnavailable {
            binary,
            source: removed,
        })
    );
}

#[test]
fn binary_refuses_a_missing_dep_info_file() {
    let timeline = Timeline::new();
    let binary = timeline.file("neomacs", 10);
    assert_eq!(
        built_from_current_sources(&binary),
        Err(RuntimeRefusal::MissingDepInfo {
            dep_info: binary.with_extension("d"),
            binary,
        })
    );
}

#[test]
fn dep_info_reads_the_rule_cargo_writes() {
    // Shaped like `target/debug/neomacs.d`: one rule, absolute paths, the
    // watched directories and files of build scripts among the sources.
    let binary = Path::new("/t/debug/neomacs");
    let rule = "/t/debug/neomacs: /w/.git/index /w/crates /w/src/main.rs /w/a\\ b.rs\n";
    assert_eq!(
        dep_info_sources(rule, binary),
        Ok(vec![
            PathBuf::from("/w/.git/index"),
            PathBuf::from("/w/crates"),
            PathBuf::from("/w/src/main.rs"),
            PathBuf::from("/w/a b.rs"),
        ])
    );
    assert_eq!(
        dep_info_sources(rule, Path::new("/t/debug/neomacsclient")),
        Err(DepInfoProblem::NoRuleFor(PathBuf::from(
            "/t/debug/neomacsclient"
        )))
    );
    assert_eq!(
        dep_info_sources("/t/debug/neomacs: src/main.rs\n", binary),
        Err(DepInfoProblem::RelativeSource(PathBuf::from("src/main.rs")))
    );
}

/// Link `artifact` to `name` as cargo links a build's product to the name in
/// the profile directory: the name is removed, then hard-linked again.
fn link_over(artifact: &Path, name: &Path) {
    std::fs::remove_file(name).expect("remove the linked name");
    std::fs::hard_link(artifact, name).expect("link the artifact to the name");
}

#[test]
fn binary_refuses_one_a_test_build_linked() {
    // `cargo build` linked the dev build and wrote the dep-info; `cargo
    // nextest run -p neomacs` then linked its test build, compiled before
    // that, to the same name and left the dep-info alone (M0 fix round 3).
    // The test build is newer than its sources and older than the dep-info.
    let timeline = Timeline::new();
    let source = timeline.file("src/main.rs", 5);
    let test_build = timeline.file_holding("deps/neomacs-test", "test build", 8);
    let binary = timeline.file_holding("neomacs", "dev build", 10);
    write_dep_info(&binary, &[&source]);
    let dep_info = binary.with_extension("d");
    timeline.stamp(&dep_info, 12);
    link_over(&test_build, &binary);
    assert_eq!(
        built_from_current_sources(&binary),
        Err(RuntimeRefusal::LinkedAfterItsDepInfo { binary, dep_info })
    );
}

#[test]
fn binary_accepts_one_cargo_build_linked_back() {
    // `cargo build` after a test build links the dev build back, with its
    // older modification time, and then writes the dep-info.
    let timeline = Timeline::new();
    let source = timeline.file("src/main.rs", 5);
    let dev_build = timeline.file_holding("deps/neomacs-dev", "dev build", 8);
    let binary = timeline.file_holding("neomacs", "test build", 10);
    link_over(&dev_build, &binary);
    write_dep_info(&binary, &[&source]);
    assert_eq!(built_from_current_sources(&binary), Ok(1));
}

/// How long a scripted terminal program gets to start and to exit.
const PROGRAM_BUDGET: Duration = Duration::from_secs(10);

/// The modes line of a terminal given back as the editors found it.
const RESTORED_MODES: &str = "modes alternate-screen=off application-keypad=off \
     application-cursor=off bracketed-paste=off mouse=None/Default";

/// How a scripted terminal program ends once a line arrives.
#[derive(Clone, Copy, Debug)]
enum Teardown {
    /// It restores the modes it set, as the editors do when they exit.
    Restores,
    /// It exits with its modes still set: a teardown that skipped the reset.
    SkipsTheReset,
    /// It never exits on its own.
    KeepsRunning,
}

/// A program that takes the terminal over as the editors do (alternate
/// screen, application keypad and cursor keys, bracketed paste), shows
/// `ready`, and ends as `teardown` says.
fn terminal_program(teardown: Teardown) -> TuiSession {
    const TAKE_OVER: &str = r"printf '\033[?1049h\033=\033[?1h\033[?2004h\033[2J\033[Hready'";
    const RESET: &str = r"printf '\033[?2004l\033[?1l\033>\033[?1049l'";
    let script = match teardown {
        Teardown::Restores => format!("{TAKE_OVER}; IFS= read -r line; {RESET}"),
        Teardown::SkipsTheReset => format!("{TAKE_OVER}; IFS= read -r line"),
        Teardown::KeepsRunning => format!("{TAKE_OVER}; exec sleep 60"),
    };
    let mut session = TuiSession::spawn_launch_on_terminal(
        TuiLaunch::new("sh").args(["-c", &script]),
        "seam-exit",
        TuiTerminalConfig::new(TERMINAL_TYPE, 24, 80),
    );
    session.read_until(PROGRAM_BUDGET, |grid| {
        grid.iter().any(|row| row.contains("ready"))
    });
    assert!(
        session.screen().alternate_screen(),
        "the program took the terminal over"
    );
    session
}

#[test]
fn leaving_records_the_terminal_a_program_restored() {
    let mut program = terminal_program(Teardown::Restores);
    let left = leave_terminal(&mut program, "RET", PROGRAM_BUDGET).expect("the program exits");
    assert!(
        left.as_str().lines().any(|line| line == RESTORED_MODES),
        "{}",
        left.as_str()
    );
}

#[test]
fn compare_reports_an_exit_that_skips_the_terminal_reset() {
    let restored = tempfile::tempdir().expect("a run directory");
    let skipped = tempfile::tempdir().expect("a run directory");
    for (teardown, run) in [
        (Teardown::Restores, &restored),
        (Teardown::SkipsTheReset, &skipped),
    ] {
        let mut program = terminal_program(teardown);
        let screen =
            leave_terminal(&mut program, "RET", PROGRAM_BUDGET).expect("the program exits");
        let record = StepRecord {
            stem: EXIT.stem(),
            screen,
            layout: None,
            pty: program.recent_output().to_vec(),
        };
        write_step(run.path(), &record);
    }
    let differences = compare_runs(restored.path(), skipped.path());
    assert_eq!(differences.len(), 1, "{differences:#?}");
    assert!(
        differences[0].starts_with(&format!("{}.screen differs", EXIT.stem()))
            && differences[0].contains(
                "+ modes alternate-screen=on application-keypad=on application-cursor=on \
                 bracketed-paste=on"
            ),
        "{}",
        differences[0]
    );
}

#[test]
fn leaving_refuses_a_program_that_does_not_exit() {
    let mut program = terminal_program(Teardown::KeepsRunning);
    let budget = Duration::from_millis(500);
    match leave_terminal(&mut program, "RET", budget) {
        Err(running) => assert_eq!(running.budget, budget),
        Ok(left) => panic!(
            "a program that never exits was recorded as leaving:\n{}",
            left.as_str()
        ),
    }
}

#[test]
fn runtime_accepts_an_image_dumped_after_all_its_inputs() {
    let timeline = Timeline::new();
    let lisp = timeline.file("subr.el", 5);
    let editor = timeline.file("neomacs", 10);
    let image = timeline.file("bootstrap-neomacs.pdump", 20);
    let runtime = ReportedRuntime {
        image: Some(image.clone()),
        loaded_lisp: vec![lisp],
    };
    assert_eq!(runtime.fresh_image(&editor), Ok(image.as_path()));
}

#[test]
fn runtime_refuses_an_image_older_than_the_editor() {
    let timeline = Timeline::new();
    let image = timeline.file("bootstrap-neomacs.pdump", 10);
    let editor = timeline.file("neomacs", 20);
    let runtime = ReportedRuntime {
        image: Some(image.clone()),
        loaded_lisp: Vec::new(),
    };
    assert_eq!(
        runtime.fresh_image(&editor),
        Err(RuntimeRefusal::ImageOlderThanEditor { image, editor })
    );
}

#[test]
fn runtime_refuses_an_image_older_than_a_loaded_lisp_file() {
    let timeline = Timeline::new();
    let unchanged = timeline.file("subr.el", 5);
    let editor = timeline.file("neomacs", 10);
    let image = timeline.file("bootstrap-neomacs.pdump", 20);
    let edited = timeline.file("frame.el", 30);
    let runtime = ReportedRuntime {
        image: Some(image.clone()),
        loaded_lisp: vec![unchanged, edited.clone()],
    };
    assert_eq!(
        runtime.fresh_image(&editor),
        Err(RuntimeRefusal::ImageOlderThanLisp {
            image,
            lisp: edited,
        })
    );
}

#[test]
fn runtime_refuses_an_editor_that_loaded_no_image() {
    let timeline = Timeline::new();
    let editor = timeline.file("neomacs", 10);
    let runtime = ReportedRuntime {
        image: None,
        loaded_lisp: Vec::new(),
    };
    assert_eq!(
        runtime.fresh_image(&editor),
        Err(RuntimeRefusal::NoImageLoaded)
    );
}

#[test]
fn runtime_accepts_an_image_its_editor_dumped() {
    let timeline = Timeline::new();
    let editor = timeline.file_holding("neomacs", "build A", 10);
    let dumper = timeline.file_holding(DUMPER_NAME, "build A", 12);
    let image = timeline.file("bootstrap-neomacs.pdump", 20);
    assert_eq!(image_dumper(&image, &editor), Ok(dumper));
}

#[test]
fn runtime_refuses_an_image_another_build_dumped() {
    // `daemon_lifecycle`'s fixture dumped it with the test build; `cargo
    // build` then linked the dev editor back with its older time, so the
    // image is newer than the editor it did not come from (M0 fix round 3).
    let timeline = Timeline::new();
    let editor = timeline.file_holding("neomacs", "build A", 10);
    let dumper = timeline.file_holding(DUMPER_NAME, "build B", 12);
    let image = timeline.file("bootstrap-neomacs.pdump", 20);
    let runtime = ReportedRuntime {
        image: Some(image.clone()),
        loaded_lisp: Vec::new(),
    };
    assert_eq!(runtime.fresh_image(&editor), Ok(image.as_path()));
    assert_eq!(
        image_dumper(&image, &editor),
        Err(RuntimeRefusal::ImageDumpedByAnotherEditor {
            image,
            dumper,
            editor,
        })
    );
}

#[test]
fn runtime_refuses_an_image_older_than_its_dumper() {
    let timeline = Timeline::new();
    let editor = timeline.file_holding("neomacs", "build A", 10);
    let image = timeline.file("bootstrap-neomacs.pdump", 20);
    let dumper = timeline.file_holding(DUMPER_NAME, "build A", 30);
    assert_eq!(
        image_dumper(&image, &editor),
        Err(RuntimeRefusal::ImageOlderThanDumper { image, dumper })
    );
}

#[test]
fn runtime_refuses_an_image_without_its_dumper() {
    let timeline = Timeline::new();
    let editor = timeline.file_holding("neomacs", "build A", 10);
    let image = timeline.file("bootstrap-neomacs.pdump", 20);
    assert_eq!(
        image_dumper(&image, &editor),
        Err(RuntimeRefusal::NoImageDumper {
            image,
            dumper: timeline.path(DUMPER_NAME),
        })
    );
}

#[test]
fn runtime_report_reads_what_the_client_prints() {
    // `neomacsclient -e` prints a string with a quote, a backslash and a
    // newline as "a\"b\\c\nd" (observed on the dev daemon in M0 fix round 1).
    assert_eq!(
        read_printed_string(r#""a\"b\\c\nd""#),
        Ok(String::from("a\"b\\c\nd"))
    );
    assert_eq!(
        ReportedRuntime::parse(
            r#""/t/debug/bootstrap-neomacs-F.pdump\n/l/subr.el\n/l/byte-run.el""#
        ),
        Ok(ReportedRuntime {
            image: Some(PathBuf::from("/t/debug/bootstrap-neomacs-F.pdump")),
            loaded_lisp: vec![PathBuf::from("/l/subr.el"), PathBuf::from("/l/byte-run.el")],
        })
    );
    // `pdumper-stats` was nil: the report starts with an empty line.
    assert_eq!(
        ReportedRuntime::parse(r#""\n/l/subr.el""#),
        Ok(ReportedRuntime {
            image: None,
            loaded_lisp: vec![PathBuf::from("/l/subr.el")],
        })
    );
}

#[test]
fn runtime_report_refuses_output_it_cannot_decode() {
    assert_eq!(
        read_printed_string("nil"),
        Err(PrintedStringError::NotQuoted)
    );
    assert_eq!(
        read_printed_string(r#""a\tb""#),
        Err(PrintedStringError::UnknownEscape(Some('t')))
    );
    assert_eq!(
        read_printed_string(r#""a"b""#),
        Err(PrintedStringError::UnquotedDoubleQuote)
    );
}
