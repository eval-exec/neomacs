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

use neomacs_test_env::{
    CanonicalEnvironment, CanonicalPin, CanonicalProfile, PinnedCommand, bounded_temp_dir,
};
use tempfile::{NamedTempFile, TempDir};

#[path = "form_source.rs"]
mod form_source;

use form_source::{FormSourceCheck, check_form_source};

// The snapshot pins themselves -- locale, time zone, login and full name,
// host, mail address, home and a bounded temp root -- have one definition in
// `neomacs_test_oracle::canonical_env` (`CanonicalProfile::Oracle`), shared
// with every other harness.  This adapter owns only what is per case: the
// form, the load root, the case directory and explicit overrides.

pub(crate) struct OracleSandbox {
    case_root: TempDir,
    /// Private temporary-file root for this case.
    ///
    /// The default child `TMPDIR` and `temporary-file-directory`.  It is
    /// created under `bounded_temp_base()` rather than the ambient temp dir:
    /// cases bind unix-domain sockets here, and a deep workspace/nextest path
    /// plus a socket name overflows `sun_path` (104 bytes on Darwin).  A
    /// distinct per-case directory also keeps the snapshot normalizer from
    /// replacing a bare `/tmp` a form prints.
    session_root: TempDir,
    /// The canonical pins for this case (locale, TZ, identity, HOME, TMPDIR,
    /// umask).  Built once; `configure` adds only per-case variables.
    environment: CanonicalEnvironment,
    home_root: PathBuf,
    form_path: PathBuf,
    load_root: PathBuf,
    load_root_owner: LoadRootOwner,
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

/// Who owns the sandbox's load root.
///
/// The project `lisp/` tree is Neomacs's own runtime and a FOREIGN tree to
/// GNU, which must keep its installed lisp; only the engine that owns the
/// tree may have it prepended to `load-path`.  A root a probe supplied
/// explicitly (a fixture or package checkout, via the `*_with_load_root`
/// helpers) is a request that BOTH engines put it first, so `(require ...)`
/// resolves the probe's files on GNU too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LoadRootOwner {
    #[default]
    Project,
    Probe,
}

impl LoadRootOwner {
    /// Environment variable carrying [`LoadRootOwner`] into the eval program.
    const ENV: &'static str = "NEOVM_ORACLE_LOAD_ROOT_OWNER";

    fn as_env_value(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Probe => "probe",
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
        validate_form_source(form)?;
        let project_root = project_root();
        let case_root = create_case_tempdir_in(&project_root)?;
        // The child's default TMPDIR: a bounded path, because cases bind unix
        // sockets in it (sun_path is 104 bytes on Darwin).  The case root
        // above stays in the workspace; it is a path the form sees, not the
        // socket namespace.
        let session_root = bounded_temp_dir("nv-oracle-")
            .map_err(|error| format!("failed to create bounded oracle session tempdir: {error}"))?;
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
        let environment = CanonicalEnvironment::new(
            CanonicalProfile::Oracle,
            &home_root,
            session_root.path(),
            &project_root,
        )
        .map_err(|error| format!("invalid canonical oracle environment: {error}"))?;

        Ok(Self {
            case_root,
            session_root,
            environment,
            home_root,
            form_path,
            load_root: load_root.to_path_buf(),
            load_root_owner: LoadRootOwner::Project,
            load_files,
            project_root,
            case_filesystem: CaseFilesystemPolicy::Private,
            result_normalization: ResultNormalization::Exact,
            extra_env: Vec::new(),
        })
    }

    /// Mark the load root as supplied by the probe itself (a fixture or
    /// package checkout) rather than the project's `lisp/` tree.
    pub(crate) fn with_probe_load_root(mut self) -> Self {
        self.load_root_owner = LoadRootOwner::Probe;
        self
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

    /// A command for one engine, already carrying the canonical pins.
    ///
    /// The pins themselves (locale, time zone, login/full name, host, mail
    /// address, HOME, TMPDIR, umask) come from
    /// `CanonicalEnvironment::command`, which is the only way a `PinnedCommand`
    /// can be built.  This method adds the case's own environment.
    pub(crate) fn pinned_command(&self, program: impl AsRef<OsStr>) -> PinnedCommand {
        self.environment.command(program)
    }

    /// Add the per-case variables to a command produced by
    /// [`Self::pinned_command`].
    ///
    /// Per-case overrides are applied after the pins on purpose: a probe that
    /// deliberately selects another locale/TMPDIR says so in its source, while
    /// the default is the recorded one.
    pub(crate) fn configure(&self, command: &mut PinnedCommand) {
        for (name, value) in &self.extra_env {
            match name.to_str().and_then(CanonicalPin::from_env_name) {
                Some(pin) => {
                    command.override_pin(pin, value);
                }
                None => {
                    command.env(name, value);
                }
            }
        }
        // This is harness-owned typed state, not an arbitrary per-test
        // override.  Apply it after `extra_env' so callers cannot create a
        // policy value which Rust did not select.
        command.env(
            "NEOVM_ORACLE_RESULT_NORMALIZATION",
            self.result_normalization.as_env_value(),
        );
        // Every editor, and every child it spawns, sees one hermetic git
        // environment: the host's system config and its default branch are
        // inputs, not oracle facts.  `org-attach-git' reports the branch of a
        // fresh repository, and `git init' names it from
        // `init.defaultBranch' (Apple Git sets "main" in /etc/gitconfig,
        // GNU/Linux distributions leave "master"), so an unpinned probe
        // records the host's git configuration.  This is the same pinning the
        // board's Magit fixture applies to its repository builder
        // (`crates/neomacs-perf/src/harness/scenarios/editor_workload.rs'):
        // ignore the system config and state the branch.  HOME is already the
        // sandbox's empty home, so no global config is read either.
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "init.defaultBranch")
            .env("GIT_CONFIG_VALUE_0", "master");

        let scratch_root = self.case_root.path();
        let explicit_tmpdir = self
            .extra_env
            .iter()
            .rev()
            .find(|(name, _)| name.as_os_str() == OsStr::new("TMPDIR"))
            .map(|(_, value)| value.clone());
        let session_tmpdir = match self.case_filesystem {
            CaseFilesystemPolicy::WorkingDirectoryAndTempDirectory => {
                command.override_pin(CanonicalPin::TmpDir, scratch_root);
                Some(scratch_root.as_os_str().to_os_string())
            }
            CaseFilesystemPolicy::Private | CaseFilesystemPolicy::ExposedAsTestTempDirectory => {
                let tmpdir = explicit_tmpdir
                    .unwrap_or_else(|| self.session_root.path().as_os_str().to_os_string());
                command.override_pin(CanonicalPin::TmpDir, &tmpdir);
                Some(tmpdir)
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
            .env(LoadRootOwner::ENV, self.load_root_owner.as_env_value())
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

/// Validate a form source before any engine receives it.
///
/// `OracleSandbox::new` calls this for every evaluation.  Assertion helpers
/// that can return before constructing a sandbox (the AOT battery when it is
/// disabled) call it directly so a skipped test cannot hide a malformed form.
pub(crate) fn validate_form_source(form: &str) -> Result<(), String> {
    match check_form_source(form) {
        FormSourceCheck::Complete => Ok(()),
        rejected => Err(rejected_form_source_error(form, rejected)),
    }
}

/// Error for a form source that GNU's reader would not consume completely.
///
/// The test name comes from the test harness thread (nextest/libtest name each
/// case thread after its test), and the source prefix makes the defect
/// actionable even when a runner executes a test on an unnamed thread.
fn rejected_form_source_error(form: &str, rejected: FormSourceCheck) -> String {
    let test = std::thread::current()
        .name()
        .map(str::to_owned)
        .unwrap_or_else(|| "<unknown test>".to_owned());
    format!(
        "oracle form source rejected for test `{test}`: {rejected}\n\
         form starts: {}\n\
         The oracle eval loop reads top-level forms until end-of-file; a source it \
         cannot read completely is never evaluated and would leave a stale expectation. \
         Balance the embedded Lisp or remove the trailing text.",
        form_source_excerpt(form)
    )
}

fn form_source_excerpt(form: &str) -> String {
    let mut chars = form.chars();
    let mut excerpt: String = chars.by_ref().take(96).collect();
    if chars.next().is_some() {
        excerpt.push('…');
    }
    excerpt
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub(crate) fn project_root() -> PathBuf {
    let root = std::env::var_os("NEXTEST_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_WORKSPACE_DIR")));
    // Cargo's relative workspace env ends in a separator; Nextest's does not.
    // Give both engines the same lexical path under either runner, without
    // resolving symlinks or weakening result normalization.
    root.components().collect()
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
