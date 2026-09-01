//! Per-case filesystem and subprocess configuration for oracle evaluations.
//!
//! `OracleSandbox` is the single seam through which both GNU Emacs and
//! Neomacs receive their form, load roots, scratch directory, and explicit
//! environment overrides. Keeping that setup here prevents the two engine
//! runners from drifting apart.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::{NamedTempFile, TempDir};

/// Locale used when recording the checked-in GNU Emacs expectations.
///
/// Collation is locale-sensitive, so inheriting the developer's locale would
/// make snapshot mode non-deterministic. Per-case overrides are applied after
/// this default and can still select another locale explicitly.
const SNAPSHOT_LOCALE: &str = "en_US.UTF-8";

/// Time zone used when recording the checked-in GNU Emacs expectations.
///
/// Date conversion, daylight-saving, and Org timestamp behavior all consult
/// the process time zone. GitHub's Ubuntu runners default to UTC, while the
/// snapshots were recorded in the America/New_York zone.
const SNAPSHOT_TIME_ZONE: &str = "America/New_York";

/// Stable process identity used by snapshot probes that inspect the
/// environment. The home directory itself remains a fresh per-case directory.
const SNAPSHOT_USER: &str = "exec";

/// Stable host identity for forms and libraries that derive output from the
/// machine name (for example TRAMP defaults, Org macros, and email addresses).
const SNAPSHOT_HOST: &str = "oracle-host";

pub(crate) struct OracleSandbox {
    case_root: TempDir,
    home_root: PathBuf,
    form_path: PathBuf,
    load_root: PathBuf,
    load_files: Vec<PathBuf>,
    project_root: PathBuf,
    case_filesystem: CaseFilesystemPolicy,
    result_normalization: ResultNormalization,
    extra_env: Vec<(OsString, OsString)>,
}

/// Text-property policy applied to the value returned by an oracle form.
///
/// Exact is the safe default: tests which intentionally exercise font-lock
/// must observe every property.  Org workflow probes may explicitly discard
/// `fontified', which is jit-lock scheduling state rather than Org data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ResultNormalization {
    #[default]
    Exact,
    IgnoreVolatileFontification,
}

impl ResultNormalization {
    fn as_env_value(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::IgnoreVolatileFontification => "ignore-volatile-fontification",
        }
    }
}

/// How a child process may use the per-case directory.
///
/// The working-directory variant deliberately includes `TMPDIR`: relative OS
/// paths and Lisp file operations must resolve inside the same case root, or a
/// GNU run can leave state that poisons the following Neomacs run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CaseFilesystemPolicy {
    #[default]
    Private,
    ExposedAsTestTempDirectory,
    WorkingDirectoryAndTempDirectory,
}

impl OracleSandbox {
    pub(crate) fn new(form: &str, load_files: &[&str], load_root: &Path) -> Result<Self, String> {
        let project_root = project_root();
        let case_root = create_case_tempdir_in(&project_root)?;
        let home_root = case_root.path().join("home");
        fs::create_dir(&home_root).map_err(|error| {
            format!(
                "failed to create oracle home directory {}: {error}",
                home_root.display()
            )
        })?;
        let form_path = write_form_file(case_root.path(), form)?;
        let load_files = load_files
            .iter()
            .map(|file| load_root.join(file))
            .collect::<Vec<_>>();

        Ok(Self {
            case_root,
            home_root,
            form_path,
            load_root: load_root.to_path_buf(),
            load_files,
            project_root,
            case_filesystem: CaseFilesystemPolicy::Private,
            result_normalization: ResultNormalization::Exact,
            extra_env: Vec::new(),
        })
    }

    pub(crate) fn expose_case_root_as_test_tmpdir(mut self) -> Self {
        if self.case_filesystem == CaseFilesystemPolicy::Private {
            self.case_filesystem = CaseFilesystemPolicy::ExposedAsTestTempDirectory;
        }
        self
    }

    pub(crate) fn with_case_working_directory_and_tmpdir(mut self) -> Self {
        self.case_filesystem = CaseFilesystemPolicy::WorkingDirectoryAndTempDirectory;
        self
    }

    pub(crate) fn with_extra_env(mut self, extra_env: &[(&str, &str)]) -> Self {
        self.extra_env.extend(
            extra_env
                .iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value))),
        );
        self
    }

    pub(crate) fn with_result_normalization(
        mut self,
        result_normalization: ResultNormalization,
    ) -> Self {
        self.result_normalization = result_normalization;
        self
    }

    pub(crate) fn configure(&self, command: &mut Command) {
        command
            .env("LANG", SNAPSHOT_LOCALE)
            .env("LC_ALL", SNAPSHOT_LOCALE)
            .env("TZ", SNAPSHOT_TIME_ZONE)
            .env("HOME", &self.home_root)
            .env("USER", SNAPSHOT_USER)
            .env("LOGNAME", SNAPSHOT_USER)
            .env("HOSTNAME", SNAPSHOT_HOST)
            .env("EMAIL", format!("{SNAPSHOT_USER}@{SNAPSHOT_HOST}"))
            .env("NEOVM_ORACLE_SYSTEM_NAME", SNAPSHOT_HOST)
            .env("TERM", "dumb")
            // Workspace-local scratch directories sit beneath the checkout.
            // Prevent repository helpers from discovering the outer repo.
            .env("GIT_CEILING_DIRECTORIES", &self.project_root);

        for (name, value) in &self.extra_env {
            command.env(name, value);
        }
        // This is harness-owned typed state, not an arbitrary per-test
        // override.  Apply it after `extra_env' so callers cannot create a
        // policy value which Rust did not select.
        command.env(
            "NEOVM_ORACLE_RESULT_NORMALIZATION",
            self.result_normalization.as_env_value(),
        );

        let scratch_root = self.case_root.path();
        let explicit_tmpdir = self
            .extra_env
            .iter()
            .rev()
            .find(|(name, _)| name.as_os_str() == OsStr::new("TMPDIR"))
            .map(|(_, value)| value.clone());
        let session_tmpdir = match self.case_filesystem {
            CaseFilesystemPolicy::WorkingDirectoryAndTempDirectory => {
                command.env("TMPDIR", scratch_root);
                Some(scratch_root.as_os_str().to_os_string())
            }
            CaseFilesystemPolicy::Private | CaseFilesystemPolicy::ExposedAsTestTempDirectory => {
                explicit_tmpdir.or_else(|| std::env::var_os("TMPDIR"))
            }
        };
        let load_files = self
            .load_files
            .iter()
            .map(|file| file.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        command
            .env("NEOVM_ORACLE_FORM_FILE", &self.form_path)
            .env("NEOVM_ORACLE_LOAD_ROOT", &self.load_root)
            .env("NEOVM_ORACLE_PROJECT_ROOT", &self.project_root)
            .env("NEOVM_ORACLE_SCRATCH_ROOT", scratch_root)
            .env("NEOVM_ORACLE_HOME", &self.home_root)
            .env("NEOVM_ORACLE_LOAD_FILES", load_files);

        if let Some(session_tmpdir) = session_tmpdir {
            command.env("NEOVM_ORACLE_SESSION_TMPDIR", session_tmpdir);
        } else {
            command.env_remove("NEOVM_ORACLE_SESSION_TMPDIR");
        }

        match self.case_filesystem {
            CaseFilesystemPolicy::Private => {
                command.env_remove("NEOVM_ORACLE_TEST_TMPDIR");
            }
            CaseFilesystemPolicy::ExposedAsTestTempDirectory => {
                command.env("NEOVM_ORACLE_TEST_TMPDIR", scratch_root);
            }
            CaseFilesystemPolicy::WorkingDirectoryAndTempDirectory => {
                command.current_dir(scratch_root);
                command.env("NEOVM_ORACLE_TEST_TMPDIR", scratch_root);
            }
        }
    }

    pub(crate) fn create_fixture_tempdir() -> Result<TempDir, String> {
        create_case_tempdir_in(&project_root())
    }
}

pub(crate) fn project_root() -> PathBuf {
    if let Some(root) = std::env::var_os("NEXTEST_WORKSPACE_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

fn scratch_base(project_root: &Path) -> Result<PathBuf, String> {
    let root = project_root.join("tmp/oracle");
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create oracle scratch directory {}: {error}",
            root.display()
        )
    })?;
    ensure_dir_locals_barrier(&root)?;
    Ok(root)
}

fn ensure_dir_locals_barrier(scratch_root: &Path) -> Result<(), String> {
    let barrier = scratch_root.join(".dir-locals.el");
    if barrier.is_file() {
        return Ok(());
    }

    let mut staged = NamedTempFile::new_in(scratch_root)
        .map_err(|error| format!("failed to stage oracle dir-locals barrier: {error}"))?;
    staged
        .write_all(b"((nil . nil))\n")
        .and_then(|()| staged.flush())
        .map_err(|error| format!("failed to write oracle dir-locals barrier: {error}"))?;
    match staged.persist_noclobber(&barrier) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(format!(
            "failed to install oracle dir-locals barrier {}: {}",
            barrier.display(),
            error.error
        )),
    }
}

fn create_case_tempdir_in(project_root: &Path) -> Result<TempDir, String> {
    let scratch_base = scratch_base(project_root)?;
    tempfile::Builder::new()
        // Keep this deliberately short: some cases create relative Unix-domain
        // socket names inside this directory.
        .prefix("case-")
        .tempdir_in(&scratch_base)
        .map_err(|error| {
            format!(
                "failed to create oracle case directory in {}: {error}",
                scratch_base.display()
            )
        })
}

fn write_form_file(case_root: &Path, form: &str) -> Result<PathBuf, String> {
    let form_path = case_root.join("form.el");
    let mut file = fs::File::create(&form_path)
        .map_err(|error| format!("failed to create oracle form file: {error}"))?;
    file.write_all(form.as_bytes())
        .map_err(|error| format!("failed to write oracle form file: {error}"))?;
    file.flush()
        .map_err(|error| format!("failed to flush oracle form file: {error}"))?;
    Ok(form_path)
}
