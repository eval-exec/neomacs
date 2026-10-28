//! Editor-agnostic install driver, deterministic process environment, and
//! the shared diagnostics moved here with the acquisition core.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::Duration;

use neomacs_test_env::{CanonicalEnvironment, PackagePaths, PinnedCommand};
use wait_timeout::ChildExt;

/// Mirror of the package-parity harness's command error for install runs.
///
/// The variants match the harness's `CommandError` one-for-one so the moved
/// install diagnostics keep their exact wording.
#[derive(Debug)]
pub enum InstallCommandError {
    Launch(std::io::Error),
    TimedOut(Output),
    Capture(String),
}

impl std::fmt::Display for InstallCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch(error) => write!(formatter, "launch failed: {error}"),
            Self::TimedOut(_) => write!(formatter, "timed out"),
            Self::Capture(error) => write!(formatter, "capture failed: {error}"),
        }
    }
}

/// The editor that drives package installation into the shared cache.
///
/// Implemented today by `neomacs-melpa-test-support`'s attested
/// `EmacsRuntime`; the CLI uses a PATH-emacs implementation.  This trait is
/// the reason this crate never depends on any concrete runtime type.
pub trait PackageInstallDriver: Sync {
    /// Display name for diagnostics (e.g. `gnu-emacs-31.1`).
    fn name(&self) -> &str;

    /// A fresh editor command carrying every pin ENVIRONMENT defines: the
    /// recorded identity (locale, TZ, login/full name, host, mail, HOME,
    /// TMPDIR, umask) and the caller's per-case sandbox paths.  There is no
    /// second environment-application step: a raw re-application would hit
    /// the pin guards.
    fn command(&self, environment: &CanonicalEnvironment) -> PinnedCommand;

    /// Install-run deadline policy.
    fn timeout(&self) -> Duration;

    /// Run `command` to completion, killing it at [`Self::timeout`].
    fn run(&self, command: &mut PinnedCommand) -> Result<Output, InstallCommandError>;
}

/// A [`PackageInstallDriver`] over any GNU Emacs binary reachable from a
/// path — `NEOMACS_PACKAGES_GNU`, else `emacs` on `PATH`.  Suites that do
/// not carry the parity harness (GUI, TUI, oracle, the CLI) provision
/// through this driver; the package-parity harness keeps its attested
/// runtime's implementation.
pub struct PathGnuDriver {
    program: PathBuf,
    timeout: Duration,
}

impl PathGnuDriver {
    /// Resolve the install editor: `NEOMACS_PACKAGES_GNU` first, then
    /// `emacs` on `PATH`.  `Err` carries the probe diagnostics.
    pub fn resolve() -> Result<Self, String> {
        let from_env = std::env::var_os("NEOMACS_PACKAGES_GNU")
            .map(PathBuf::from)
            .filter(|path| path.is_file());
        let program = match from_env {
            Some(path) => path,
            None => {
                let which = std::process::Command::new("which")
                    .arg("emacs")
                    .output()
                    .map_err(|error| format!("failed to probe PATH for emacs: {error}"))?;
                let candidate =
                    PathBuf::from(String::from_utf8_lossy(&which.stdout).trim().to_string());
                if !which.status.success() || !candidate.is_file() {
                    return Err(
                        "no GNU Emacs for package installs: set NEOMACS_PACKAGES_GNU or put `emacs` on PATH"
                            .to_string(),
                    );
                }
                candidate
            }
        };
        Ok(Self {
            program,
            timeout: Duration::from_secs(600),
        })
    }
}

impl PackageInstallDriver for PathGnuDriver {
    fn name(&self) -> &str {
        "path-gnu-emacs"
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }

    fn command(&self, environment: &CanonicalEnvironment) -> PinnedCommand {
        // The environment carries both the identity pins and this case's
        // sandbox paths; there is no unpinned way to name the editor.
        environment.command(&self.program)
    }

    fn run(&self, command: &mut PinnedCommand) -> Result<Output, InstallCommandError> {
        output_with_timeout_pinned(command, self.timeout)
    }
}

/// `NEXTEST_RUN_ID`, or a per-process fallback outside nextest.
pub fn package_preparation_run_id() -> String {
    std::env::var("NEXTEST_RUN_ID").unwrap_or_else(|_| format!("process-{}", std::process::id()))
}

/// Publish a shared preparation failure so concurrent cache callers fail
/// fast with the same error instead of repeating the doomed install.
pub fn publish_package_preparation_failure(
    failed_marker: &Path,
    failure_prefix: &str,
    error: String,
) -> String {
    let marker_tmp = failed_marker.with_extension(format!("{}.tmp", std::process::id()));
    let contents = format!("{failure_prefix}{error}");
    if let Err(cache_error) = std::fs::write(&marker_tmp, contents)
        .and_then(|()| std::fs::rename(&marker_tmp, failed_marker))
    {
        return format!(
            "{error}\nfailed to publish shared package preparation failure {}: {cache_error}",
            failed_marker.display()
        );
    }
    error
}

/// Quote a string as an elisp string literal.
pub fn elisp_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        if matches!(character, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('"');
    quoted
}

pub(crate) fn os_string(value: &std::ffi::OsStr) -> std::ffi::OsString {
    value.to_os_string()
}

/// The deterministic process environment shared by package preparation and
/// test sessions (moved with the acquisition core so the install cache and
/// its consumers cannot drift apart).
/// The package sandbox's canonical environment for one case.
///
/// The identity pins and every suite variable are defined once, in
/// `neomacs_test_oracle::canonical_env` (`CanonicalProfile::Package`); this
/// function only supplies the case's roots.
pub fn package_environment_with_runtime(
    root: &Path,
    home: &Path,
    tmp: &Path,
    runtime: &Path,
) -> Result<CanonicalEnvironment, String> {
    CanonicalEnvironment::package(
        home,
        tmp,
        PackagePaths {
            scratch_root: root.to_path_buf(),
            workspace_root: crate::workspace_root(),
            runtime_dir: runtime.to_path_buf(),
            melpa_cache_root: crate::melpa_cache_root(),
        },
    )
    .map_err(|error| format!("package sandbox environment: {error}"))
}

/// Deterministic environment entries for adapters that launch a package
/// process without [`std::process::Command`] (a PTY launcher, a test driver).
pub fn deterministic_process_environment(
    root: &Path,
    home: &Path,
    tmp: &Path,
    runtime: &Path,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    package_environment_with_runtime(root, home, tmp, runtime)
        .expect("package sandbox roots must be absolute")
        .env_pairs()
        .into_iter()
        .map(|(name, value)| (std::ffi::OsString::from(name), value))
        .collect()
}

/// Apply the package sandbox's deterministic environment to a NON-editor
/// helper command (git, package-build tooling).
///
/// The values are the same as the editor environment's, so helper output
/// (git messages, timestamps, locale-sensitive text) is as deterministic as
/// the editor's; the helper is not an editor process, so it has no identity
/// pins to enforce with a `PinnedCommand`.
pub fn configure_helper_process_environment(
    command: &mut Command,
    root: &Path,
    home: &Path,
    tmp: &Path,
) {
    let environment = package_environment_with_runtime(root, home, tmp, &root.join("xdg/runtime"))
        .expect("package sandbox roots must be absolute");
    command.current_dir(root);
    for (name, value) in environment.env_pairs() {
        command.env(name, value);
    }
    for name in environment.removed_vars() {
        command.env_remove(name);
    }
}

/// Run a plain helper command (git, package-build tooling) to completion,
/// killing it at `timeout`.  Single-process ownership: helper commands never
/// spawn process trees, so unlike the harness's grouped capture there is
/// nothing to reap beyond the child itself.
/// The two command types this crate can run: a plain helper `Command` and a
/// canonically pinned editor command.
trait EditorLikeCommand {
    fn pipe_output(&mut self);
    fn launch(&mut self) -> std::io::Result<Child>;
}

impl EditorLikeCommand for Command {
    fn pipe_output(&mut self) {
        self.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    fn launch(&mut self) -> std::io::Result<Child> {
        self.spawn()
    }
}

impl EditorLikeCommand for PinnedCommand {
    fn pipe_output(&mut self) {
        self.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    fn launch(&mut self) -> std::io::Result<Child> {
        self.spawn()
    }
}

fn run_with_timeout<C: EditorLikeCommand>(
    command: &mut C,
    timeout: Duration,
) -> Result<Output, InstallCommandError> {
    command.pipe_output();
    let mut child = command.launch().map_err(InstallCommandError::Launch)?;
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| InstallCommandError::Capture("stdout pipe was not created".to_string()))?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| InstallCommandError::Capture("stderr pipe was not created".to_string()))?;
    let stdout_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buffer).map_err(|error| {
            InstallCommandError::Capture(format!("failed to read stdout: {error}"))
        })?;
        Ok::<_, InstallCommandError>(buffer)
    });
    let stderr_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buffer).map_err(|error| {
            InstallCommandError::Capture(format!("failed to read stderr: {error}"))
        })?;
        Ok::<_, InstallCommandError>(buffer)
    });

    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            // Deadline hit: kill the child, drain whatever output it managed
            // to produce, and report TimedOut — the `source_lock` install
            // diagnostics match on this variant for the "timed out after
            // {timeout:?}" wording, so a deadline must never masquerade as
            // an ordinary (failed) exit.
            let _ = child.kill();
            let status = child.wait().map_err(InstallCommandError::Launch)?;
            let stdout = stdout_reader.join().map_err(|_| {
                InstallCommandError::Capture("stdout reader panicked".to_string())
            })??;
            let stderr = stderr_reader.join().map_err(|_| {
                InstallCommandError::Capture("stderr reader panicked".to_string())
            })??;
            return Err(InstallCommandError::TimedOut(Output {
                status,
                stdout,
                stderr,
            }));
        }
        Err(error) => return Err(InstallCommandError::Launch(error)),
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| InstallCommandError::Capture("stdout reader panicked".to_string()))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| InstallCommandError::Capture("stderr reader panicked".to_string()))??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<Output, InstallCommandError> {
    run_with_timeout(command, timeout)
}

pub(crate) fn output_with_timeout_pinned(
    command: &mut PinnedCommand,
    timeout: Duration,
) -> Result<Output, InstallCommandError> {
    run_with_timeout(command, timeout)
}

/// Per-scratch directories the install cache builds inside.
pub struct InstallScratch {
    pub root: PathBuf,
    pub home: PathBuf,
    pub editor_tmp: PathBuf,
}

impl InstallScratch {
    pub fn new(root: PathBuf) -> Self {
        Self {
            home: root.join("home"),
            editor_tmp: root.join("tmp"),
            root,
        }
    }
}
