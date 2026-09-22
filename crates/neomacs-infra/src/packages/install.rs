//! Editor-agnostic install driver, deterministic process environment, and
//! the shared diagnostics moved here with the acquisition core.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

    /// A fresh, fully configured editor command (program and editor
    /// environment; the caller adds the specific install invocation).
    fn command(&self) -> Command;

    /// Install-run deadline policy.
    fn timeout(&self) -> Duration;

    /// Run `command` to completion, killing it at [`Self::timeout`].
    fn run(&self, command: &mut Command) -> Result<Output, InstallCommandError>;
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
                        "no GNU Emacs for package installs: set NEOMACS_PACKAGES_GNU or put                          `emacs` on PATH"
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

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        // Package installation only ever runs `--batch` probes; the
        // deterministic sandbox environment is applied per invocation by
        // `configure_process_environment`.
        command
            .env("TERM", "dumb")
            .env("LC_ALL", "C.UTF-8")
            .env("LANG", "C.UTF-8");
        command
    }

    fn run(&self, command: &mut Command) -> Result<Output, InstallCommandError> {
        output_with_timeout(command, self.timeout)
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
pub fn deterministic_process_environment(
    root: &Path,
    home: &Path,
    tmp: &Path,
    runtime: &Path,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    vec![
        (
            std::ffi::OsString::from("HOME"),
            os_string(home.as_os_str()),
        ),
        (
            std::ffi::OsString::from("TMPDIR"),
            os_string(tmp.as_os_str()),
        ),
        (std::ffi::OsString::from("TMP"), os_string(tmp.as_os_str())),
        (std::ffi::OsString::from("TEMP"), os_string(tmp.as_os_str())),
        (
            std::ffi::OsString::from("XDG_CONFIG_HOME"),
            os_string(root.join("xdg/config").as_os_str()),
        ),
        (
            std::ffi::OsString::from("XDG_CACHE_HOME"),
            os_string(root.join("xdg/cache").as_os_str()),
        ),
        (
            std::ffi::OsString::from("XDG_DATA_HOME"),
            os_string(root.join("xdg/data").as_os_str()),
        ),
        (
            std::ffi::OsString::from("XDG_STATE_HOME"),
            os_string(root.join("xdg/state").as_os_str()),
        ),
        (
            std::ffi::OsString::from("XDG_RUNTIME_DIR"),
            os_string(runtime.as_os_str()),
        ),
        (
            std::ffi::OsString::from("LANG"),
            std::ffi::OsString::from("C.UTF-8"),
        ),
        (
            std::ffi::OsString::from("LC_ALL"),
            std::ffi::OsString::from("C.UTF-8"),
        ),
        (
            std::ffi::OsString::from("TZ"),
            std::ffi::OsString::from("UTC"),
        ),
        (
            std::ffi::OsString::from("USER"),
            std::ffi::OsString::from("melpa-test"),
        ),
        (
            std::ffi::OsString::from("LOGNAME"),
            std::ffi::OsString::from("melpa-test"),
        ),
        (
            std::ffi::OsString::from("HOSTNAME"),
            std::ffi::OsString::from("melpa-host"),
        ),
        (
            std::ffi::OsString::from("EMAIL"),
            std::ffi::OsString::from("melpa-test@melpa-host"),
        ),
        (
            std::ffi::OsString::from("TERM"),
            std::ffi::OsString::from("dumb"),
        ),
        (
            std::ffi::OsString::from("NEOMACS_TEST_SANDBOX_ROOT"),
            os_string(root.as_os_str()),
        ),
        (
            std::ffi::OsString::from("NEOMACS_TEST_WORKSPACE_ROOT"),
            os_string(crate::workspace_root().as_os_str()),
        ),
        (
            std::ffi::OsString::from("GIT_CEILING_DIRECTORIES"),
            os_string(crate::workspace_root().as_os_str()),
        ),
    ]
}

/// Apply [`deterministic_process_environment`] to a command.
pub fn configure_process_environment(command: &mut Command, root: &Path, home: &Path, tmp: &Path) {
    configure_process_environment_with_runtime(command, root, home, tmp, &root.join("xdg/runtime"));
}

/// [`configure_process_environment`] with an explicit `XDG_RUNTIME_DIR`
/// (GNU binds AF_UNIX sockets under it, so sandboxed runs keep that namespace
/// on a short path).
pub fn configure_process_environment_with_runtime(
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

/// Run a plain helper command (git, package-build tooling) to completion,
/// killing it at `timeout`.  Single-process ownership: helper commands never
/// spawn process trees, so unlike the harness's grouped capture there is
/// nothing to reap beyond the child itself.
pub(crate) fn output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<Output, InstallCommandError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(InstallCommandError::Launch)?;
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
            let _ = child.kill();
            let status = child.wait().map_err(InstallCommandError::Launch)?;
            let _ = Instant::now();
            status
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
