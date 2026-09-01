//! Shared preparation support for Neomacs MELPA compatibility tests.
//!
//! This crate owns revision-pinned package acquisition, deterministic package
//! process setup, and filesystem isolation. Test adapters remain in their
//! respective crates: batch/value comparison in `neomacs-melpa-tests` and PTY
//! grid comparison in `neomacs-tui-tests`.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use command_group::{CommandGroup, GroupChild};
use wait_timeout::ChildExt;

mod prepared_package_set;
mod source_lock;
mod tree_sitter_grammar;

pub use prepared_package_set::{PackageActivation, PreparedPackageSet, package_activation_elisp};
pub use source_lock::{
    LockedPackageSource, SHALLOW_GIT_FETCH_ARGS, SourceBuild, locked_melpa_install_plan,
    locked_melpa_source, locked_melpa_sources, preflight_locked_melpa_packages,
    prepare_cached_locked_melpa_package, prepare_cached_locked_package_plan,
};
pub use tree_sitter_grammar::{
    prepare_cached_tree_sitter_grammar, prepare_cached_tree_sitter_grammar_from_subdirectory,
};

pub const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(300);

#[cfg(test)]
mod process_test;
#[cfg(test)]
mod tree_sitter_grammar_test;

/// Resolve the checkout used by a normal Cargo run or an extracted Nextest
/// archive.
pub fn workspace_root() -> PathBuf {
    if let Some(root) = std::env::var_os("NEXTEST_WORKSPACE_ROOT") {
        return PathBuf::from(root);
    }
    Path::new(env!("CARGO_WORKSPACE_DIR")).to_path_buf()
}

/// Per-scenario filesystem and subprocess isolation.
pub struct MelpaSandbox {
    case_root: tempfile::TempDir,
    home: PathBuf,
    tmp: PathBuf,
    runtime: RuntimeDirectory,
}

enum RuntimeDirectory {
    #[cfg(unix)]
    ShortSocketPath(tempfile::TempDir),
    #[cfg(not(unix))]
    InSandbox(PathBuf),
}

impl RuntimeDirectory {
    fn new(_case_root: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        {
            // GNU Emacs appends `emacs/<server-name>` to XDG_RUNTIME_DIR
            // before binding an AF_UNIX socket.  Keep that namespace outside
            // the arbitrarily deep checkout path while every persistent test
            // artifact remains below CASE_ROOT.
            let system_tmp = Path::new("/tmp");
            let base = if system_tmp.is_dir() {
                system_tmp.to_path_buf()
            } else {
                std::env::temp_dir()
            };
            let directory = tempfile::Builder::new()
                .prefix("nmr-")
                .tempdir_in(&base)
                .map_err(|error| {
                    format!(
                        "failed to create short MELPA runtime directory in {}: {error}",
                        base.display()
                    )
                })?;
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).map_err(
                |error| format!("failed to secure isolated XDG runtime directory: {error}"),
            )?;
            Ok(Self::ShortSocketPath(directory))
        }
        #[cfg(not(unix))]
        {
            let directory = _case_root.join("xdg/runtime");
            fs::create_dir_all(&directory).map_err(|error| {
                format!(
                    "failed to create MELPA runtime directory {}: {error}",
                    directory.display()
                )
            })?;
            Ok(Self::InSandbox(directory))
        }
    }

    fn path(&self) -> &Path {
        match self {
            #[cfg(unix)]
            Self::ShortSocketPath(directory) => directory.path(),
            #[cfg(not(unix))]
            Self::InSandbox(directory) => directory,
        }
    }
}

impl MelpaSandbox {
    /// Create a sandbox below `<workspace>/tmp/melpa`.
    pub fn new(label: &str) -> Result<Self, String> {
        let base = workspace_root().join("tmp/melpa");
        fs::create_dir_all(&base).map_err(|error| {
            format!(
                "failed to create MELPA scratch directory {}: {error}",
                base.display()
            )
        })?;
        let prefix = format!("{}-", sanitize_label(label));
        let case_root = tempfile::Builder::new()
            .prefix(&prefix)
            .tempdir_in(&base)
            .map_err(|error| {
                format!(
                    "failed to create MELPA scenario directory in {}: {error}",
                    base.display()
                )
            })?;
        let home = case_root.path().join("home");
        let tmp = case_root.path().join("tmp");
        let xdg_config = case_root.path().join("xdg/config");
        let xdg_cache = case_root.path().join("xdg/cache");
        let xdg_data = case_root.path().join("xdg/data");
        let xdg_state = case_root.path().join("xdg/state");
        for directory in [&home, &tmp, &xdg_config, &xdg_cache, &xdg_data, &xdg_state] {
            fs::create_dir_all(directory).map_err(|error| {
                format!(
                    "failed to create MELPA sandbox directory {}: {error}",
                    directory.display()
                )
            })?;
        }
        let runtime = RuntimeDirectory::new(case_root.path())?;
        fs::create_dir_all(home.join(".emacs.d"))
            .map_err(|error| format!("failed to create isolated .emacs.d: {error}"))?;

        Ok(Self {
            case_root,
            home,
            tmp,
            runtime,
        })
    }

    pub fn root(&self) -> &Path {
        self.case_root.path()
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn tmp_dir(&self) -> &Path {
        &self.tmp
    }

    /// Deterministic environment entries for adapters that do not use
    /// [`std::process::Command`] directly, such as a PTY launcher.
    pub fn process_environment(&self) -> Vec<PackageEnvironmentEntry> {
        deterministic_process_environment(self.root(), &self.home, &self.tmp, self.runtime.path())
    }

    /// Apply the deterministic process environment shared by package test
    /// adapters.
    pub fn configure(&self, command: &mut Command) {
        configure_process_environment_with_runtime(
            command,
            self.root(),
            &self.home,
            &self.tmp,
            self.runtime.path(),
        );
    }
}

/// Apply the deterministic environment used by package preparation and test
/// processes.
pub fn configure_process_environment(command: &mut Command, root: &Path, home: &Path, tmp: &Path) {
    configure_process_environment_with_runtime(command, root, home, tmp, &root.join("xdg/runtime"));
}

fn configure_process_environment_with_runtime(
    command: &mut Command,
    root: &Path,
    home: &Path,
    tmp: &Path,
    runtime: &Path,
) {
    command
        .current_dir(root)
        .envs(deterministic_process_environment(root, home, tmp, runtime))
        .env_remove("EMACSLOADPATH");
}

fn deterministic_process_environment(
    root: &Path,
    home: &Path,
    tmp: &Path,
    runtime: &Path,
) -> Vec<PackageEnvironmentEntry> {
    vec![
        (OsString::from("HOME"), os_string(home.as_os_str())),
        (OsString::from("TMPDIR"), os_string(tmp.as_os_str())),
        (OsString::from("TMP"), os_string(tmp.as_os_str())),
        (OsString::from("TEMP"), os_string(tmp.as_os_str())),
        (
            OsString::from("XDG_CONFIG_HOME"),
            os_string(root.join("xdg/config").as_os_str()),
        ),
        (
            OsString::from("XDG_CACHE_HOME"),
            os_string(root.join("xdg/cache").as_os_str()),
        ),
        (
            OsString::from("XDG_DATA_HOME"),
            os_string(root.join("xdg/data").as_os_str()),
        ),
        (
            OsString::from("XDG_STATE_HOME"),
            os_string(root.join("xdg/state").as_os_str()),
        ),
        (
            OsString::from("XDG_RUNTIME_DIR"),
            os_string(runtime.as_os_str()),
        ),
        (OsString::from("LANG"), OsString::from("C.UTF-8")),
        (OsString::from("LC_ALL"), OsString::from("C.UTF-8")),
        (OsString::from("TZ"), OsString::from("UTC")),
        (OsString::from("USER"), OsString::from("melpa-test")),
        (OsString::from("LOGNAME"), OsString::from("melpa-test")),
        (OsString::from("HOSTNAME"), OsString::from("melpa-host")),
        (
            OsString::from("EMAIL"),
            OsString::from("melpa-test@melpa-host"),
        ),
        (OsString::from("TERM"), OsString::from("dumb")),
        (
            OsString::from("NEOMACS_TEST_SANDBOX_ROOT"),
            os_string(root.as_os_str()),
        ),
        (
            OsString::from("NEOMACS_TEST_WORKSPACE_ROOT"),
            os_string(workspace_root().as_os_str()),
        ),
        (
            OsString::from("GIT_CEILING_DIRECTORIES"),
            os_string(workspace_root().as_os_str()),
        ),
    ]
}

pub fn sanitize_label(label: &str) -> String {
    let sanitized = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "scenario".to_string()
    } else {
        sanitized
    }
}

/// An editor executable used to prepare or exercise a package scenario.
#[derive(Clone, Debug)]
pub struct EmacsRuntime {
    pub name: String,
    pub executable: PathBuf,
    extra_env: Vec<(OsString, OsString)>,
    pub timeout: Duration,
    /// Set only by [`EmacsRuntime::gnu_emacs`], and only when a reference was
    /// actually attested.  `None` on this port's own runtime, where no pin
    /// applies, and on a GNU that is absent.
    reference: Option<neomacs_parity_reference::ReferenceUse>,
}

impl EmacsRuntime {
    pub fn new(name: impl Into<String>, executable: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            executable: executable.into(),
            extra_env: Vec::new(),
            timeout: DEFAULT_PROCESS_TIMEOUT,
            reference: None,
        }
    }

    pub fn neomacs() -> Self {
        Self::new("neomacs", neomacs_binary())
    }

    /// GNU Emacs oracle selected explicitly by environment, then from the
    /// developer's adjacent source checkout, and finally from `PATH`.
    ///
    /// # The reference is ATTESTED here (ledger 214)
    ///
    /// This is the single chokepoint through which every melpa and TUI parity
    /// test reaches GNU, and until ledger 214 it checked nothing at all: the
    /// three environment variables, the hard-coded checkout and `PATH` are FOUR
    /// resolution rules, and the other harnesses have their own, so two suites
    /// in one session could have scored against two different GNUs without a
    /// word.  Attesting here means a mismatch cannot reach a comparison.
    ///
    /// A mismatch panics rather than returning an error on purpose.  Every
    /// caller is a parity test whose only possible response is to stop, and a
    /// `Result` here would be a door for someone to score anyway.  A GNU that
    /// is simply ABSENT is left to the caller as before --- that is a skip, not
    /// a wrong answer.
    pub fn gnu_emacs() -> Self {
        let mut runtime = Self::new("gnu-emacs", Self::gnu_emacs_executable());
        match neomacs_parity_reference::attest(
            &runtime.executable,
            neomacs_parity_reference::AttestationDepth::Fingerprint,
        ) {
            Ok(reference) => {
                runtime.executable = reference.executable().to_path_buf();
                runtime.reference = Some(reference);
            }
            Err(
                error @ neomacs_parity_reference::AttestationError::ExecutableUnresolved { .. },
            ) => {
                // No GNU here at all; the caller's own missing-editor handling
                // reports it.  Ledger 211 section 10.1's distinction: an editor
                // that could not be RUN is not an editor that answered wrongly.
                let _ = error;
            }
            Err(error) => panic!(
                "the GNU oracle is present but is NOT the pinned reference, so a parity \
                 comparison against it is not comparable with any published number.\n{error}"
            ),
        }
        runtime
    }

    fn gnu_emacs_executable() -> PathBuf {
        for variable in [
            "NEOMACS_MELPA_ORACLE_EMACS",
            "NEOVM_ORACLE_EMACS",
            "ORACLE_EMACS",
        ] {
            if let Some(path) = std::env::var_os(variable) {
                return PathBuf::from(path);
            }
        }
        let source_checkout =
            PathBuf::from("/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs");
        if source_checkout.is_file() {
            return source_checkout;
        }
        PathBuf::from("emacs")
    }

    /// What this runtime was attested to be, when it is GNU and was checked.
    ///
    /// This is what a published melpa or TUI number carries, the way ledger 210
    /// made every motion count carry its geometry.
    pub fn reference(&self) -> Option<&neomacs_parity_reference::ReferenceUse> {
        self.reference.as_ref()
    }

    pub fn with_env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.extra_env.push((name.into(), value.into()));
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        for (name, value) in &self.extra_env {
            command.env(name, value);
        }
        command
    }
}

#[derive(Debug)]
pub enum CommandError {
    Launch(std::io::Error),
    TimedOut(Output),
    Capture(String),
}

pub fn output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<Output, CommandError> {
    output_with_timeout_in_scope(command, timeout, ProcessScope::Single)
}

/// Capture a command with a deadline that terminates its complete process tree.
///
/// Use this for adapters that intentionally launch profilers, PTYs, or
/// compositors. Ordinary single-process package probes retain
/// [`output_with_timeout`]'s narrower process ownership.
pub fn group_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<Output, CommandError> {
    output_with_timeout_in_scope(command, timeout, ProcessScope::Group)
}

#[derive(Clone, Copy)]
enum ProcessScope {
    Single,
    Group,
}

enum ManagedChild {
    Single(std::process::Child),
    Group(GroupChild),
}

impl ManagedChild {
    fn process(&mut self) -> &mut std::process::Child {
        match self {
            Self::Single(child) => child,
            Self::Group(child) => child.inner(),
        }
    }

    fn kill(&mut self) -> std::io::Result<()> {
        match self {
            Self::Single(child) => child.kill(),
            Self::Group(child) => child.kill(),
        }
    }

    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        match self {
            Self::Single(child) => child.wait(),
            Self::Group(child) => child.wait(),
        }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        match self {
            Self::Single(child) => child.try_wait(),
            Self::Group(child) => child.try_wait(),
        }
    }

    fn wait_for_exit_and_output(
        &mut self,
        scope: ProcessScope,
        timeout: Duration,
        stdout_reader: &thread::JoinHandle<std::io::Result<Vec<u8>>>,
        stderr_reader: &thread::JoinHandle<std::io::Result<Vec<u8>>>,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        match scope {
            ProcessScope::Single => self.process().wait_timeout(timeout),
            ProcessScope::Group => {
                let started = Instant::now();
                let mut leader_status = None;
                loop {
                    if leader_status.is_none() {
                        leader_status = self.try_wait()?;
                    }
                    if leader_status.is_some()
                        && stdout_reader.is_finished()
                        && stderr_reader.is_finished()
                    {
                        return Ok(leader_status);
                    }
                    let elapsed = started.elapsed();
                    let Some(remaining) = timeout.checked_sub(elapsed) else {
                        return Ok(None);
                    };
                    thread::sleep(remaining.min(Duration::from_millis(10)));
                }
            }
        }
    }
}

fn output_with_timeout_in_scope(
    command: &mut Command,
    timeout: Duration,
    scope: ProcessScope,
) -> Result<Output, CommandError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match scope {
        ProcessScope::Single => {
            ManagedChild::Single(command.spawn().map_err(CommandError::Launch)?)
        }
        ProcessScope::Group => {
            ManagedChild::Group(command.group_spawn().map_err(CommandError::Launch)?)
        }
    };
    let stdout = child
        .process()
        .stdout
        .take()
        .ok_or_else(|| CommandError::Capture("stdout pipe was not created".to_string()))?;
    let stderr = child
        .process()
        .stderr
        .take()
        .ok_or_else(|| CommandError::Capture("stderr pipe was not created".to_string()))?;
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));

    let status = match child
        .wait_for_exit_and_output(scope, timeout, &stdout_reader, &stderr_reader)
        .map_err(CommandError::Launch)?
    {
        Some(status) => status,
        None => {
            // For grouped children this terminates the complete process group
            // (or Windows job), so descendants cannot retain the output pipes.
            let _ = child.kill();
            let status = child.wait().map_err(CommandError::Launch)?;
            let stdout = stdout_reader
                .join()
                .map_err(|_| CommandError::Capture("stdout reader panicked".to_string()))?
                .map_err(|error| {
                    CommandError::Capture(format!("failed to read stdout: {error}"))
                })?;
            let stderr = stderr_reader
                .join()
                .map_err(|_| CommandError::Capture("stderr reader panicked".to_string()))?
                .map_err(|error| {
                    CommandError::Capture(format!("failed to read stderr: {error}"))
                })?;
            return Err(CommandError::TimedOut(Output {
                status,
                stdout,
                stderr,
            }));
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| CommandError::Capture("stdout reader panicked".to_string()))?
        .map_err(|error| CommandError::Capture(format!("failed to read stdout: {error}")))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| CommandError::Capture("stderr reader panicked".to_string()))?
        .map_err(|error| CommandError::Capture(format!("failed to read stderr: {error}")))?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_pipe(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn package_preparation_run_id() -> String {
    std::env::var("NEXTEST_RUN_ID").unwrap_or_else(|_| format!("process-{}", std::process::id()))
}

pub fn publish_package_preparation_failure(
    failed_marker: &Path,
    failure_prefix: &str,
    error: String,
) -> String {
    let marker_tmp = failed_marker.with_extension(format!("{}.tmp", std::process::id()));
    let contents = format!("{failure_prefix}{error}");
    if let Err(cache_error) =
        fs::write(&marker_tmp, contents).and_then(|()| fs::rename(&marker_tmp, failed_marker))
    {
        return format!(
            "{error}\nfailed to publish shared package preparation failure {}: {cache_error}",
            failed_marker.display()
        );
    }
    error
}

pub fn elisp_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The path to the `neomacs` binary (override with `NEOMACS_BIN`).
pub fn neomacs_binary() -> PathBuf {
    std::env::var_os("NEOMACS_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target/release/neomacs"))
}

/// Environment entry exported by a prepared package set.
pub type PackageEnvironmentEntry = (OsString, OsString);

pub(crate) fn os_string(value: &OsStr) -> OsString {
    value.to_os_string()
}
