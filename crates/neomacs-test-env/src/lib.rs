//! The one canonical process environment every harness gives an editor.
//!
//! Checked-in expectations are captured under a fixed ambient environment.
//! Anything the editor can read from the host that is *not* part of the form
//! -- the account's passwd GECOS through `NAME`, the login name through
//! `USER`/`LOGNAME`, the locale through `LC_ALL`, the time zone through `TZ`,
//! the home directory, the temp root and the process umask -- turns a
//! comparison into a property of the host that ran it.  Two hosts already
//! proved it: treeroot's passwd GECOS named the account, so Org export's
//! default author line changed, and a long workspace path puts a unix socket
//! beyond `sun_path`.
//!
//! [`CanonicalEnvironment`] is the only place those values are named.  It has
//! private fields and one constructor, and [`PinnedCommand`] has private
//! fields and no public constructor: a harness gets one only from
//! [`CanonicalEnvironment::command`], so an editor spawn cannot be built
//! "mostly pinned".  The profiles differ because the recorded expectation
//! sets differ, not because a harness may choose what to pin:
//!
//! * [`CanonicalProfile::Oracle`] -- `neovm-oracle-tests` snapshots, recorded
//!   under `en_US.UTF-8`, `America/New_York`, user `exec`, empty full name,
//!   host `oracle-host`.
//! * [`CanonicalProfile::Package`] -- MELPA package scenarios, recorded under
//!   `C.UTF-8`, `UTC`, user/full name `melpa-test`, host `melpa-host`.
//! * [`CanonicalProfile::Tui`] -- paired TUI grid comparisons, recorded under
//!   `C.UTF-8` for the mode line's mule-info segment.
//!
//! A value that a suite's expectations pin per case (a test that deliberately
//! selects another locale, time zone or TMPDIR) is applied *after* the
//! canonical set through [`PinnedCommand::env`]; that override is part of the
//! case, while the default is not negotiable.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};

/// Which recorded identity set an expectation suite was made under.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalProfile {
    /// `neovm-oracle-tests` snapshot/live comparisons.
    Oracle,
    /// MELPA package preparation and parity scenarios.
    Package,
    /// Paired TUI grid comparisons.
    Tui,
}

impl CanonicalProfile {
    /// The locale the suite's expectations were recorded under.
    pub const fn locale(self) -> &'static str {
        match self {
            Self::Oracle => "en_US.UTF-8",
            Self::Package | Self::Tui => "C.UTF-8",
        }
    }

    /// The time zone the suite's expectations were recorded under.
    pub const fn time_zone(self) -> &'static str {
        match self {
            Self::Oracle => "America/New_York",
            Self::Package | Self::Tui => "UTC",
        }
    }

    /// The login name the suite's expectations were recorded under.
    pub const fn login_name(self) -> &'static str {
        match self {
            Self::Oracle | Self::Tui => "exec",
            Self::Package => "melpa-test",
        }
    }

    /// The full name (`$NAME`, which GNU prefers over passwd GECOS,
    /// `src/editfns.c:128-134`) the suite's expectations were recorded under.
    pub const fn full_name(self) -> &'static str {
        match self {
            Self::Oracle | Self::Tui => "",
            Self::Package => "melpa-test",
        }
    }

    /// The host name the expectations were recorded under.
    pub const fn host_name(self) -> &'static str {
        match self {
            Self::Oracle | Self::Tui => "oracle-host",
            Self::Package => "melpa-host",
        }
    }

    /// Whether the suite creates unix-domain sockets in its temp root, and
    /// therefore needs a `sun_path`-sized root.
    ///
    /// The oracle suite is the one that does: TLS/Tramp fixtures bind
    /// `AF_UNIX` sockets and GNU's `server`/dbus adapters use temp paths.  A
    /// package scenario's temp root is whatever its prepared cache root is,
    /// and TUI sessions talk to a pty device rather than a socket path, so
    /// neither binds in TMPDIR.
    const fn bounds_temp_root(self) -> bool {
        match self {
            Self::Oracle => true,
            Self::Package | Self::Tui => false,
        }
    }
}

/// A rejected canonical environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalError {
    /// The home or temp root was not absolute.
    NotAbsolute { field: &'static str, path: PathBuf },
    /// The temp root is longer than the platform's `sun_path` leaves room for.
    TempRootTooLong {
        path: PathBuf,
        bytes: usize,
        max: usize,
    },
    /// A profile that derives path variables did not get a project root.
    MissingProjectRoot,
    /// The package profile needs its scratch/workspace/runtime roots; use
    /// [`CanonicalEnvironment::package`] to supply them.
    MissingPackagePaths,
}

impl std::fmt::Display for CanonicalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAbsolute { field, path } => {
                write!(
                    formatter,
                    "canonical {field} must be absolute: {}",
                    path.display()
                )
            }
            Self::TempRootTooLong { path, bytes, max } => write!(
                formatter,
                "canonical TMPDIR is {bytes} bytes (max {max}) and will overflow a unix socket path: {}",
                path.display()
            ),
            Self::MissingProjectRoot => {
                write!(
                    formatter,
                    "the package profile needs an absolute project root"
                )
            }
            Self::MissingPackagePaths => write!(
                formatter,
                "the package profile needs its scratch, workspace, runtime and MELPA-cache roots: use CanonicalEnvironment::package"
            ),
        }
    }
}

impl std::error::Error for CanonicalError {}

/// `sizeof(((struct sockaddr_un *) 0)->sun_path)`: 108 on Linux, 104 on
/// Darwin/BSD.  The smallest of the platforms the suite runs on is the one to
/// respect; leaving 40 bytes for a test's socket file name means the temp
/// root must stay at or below 64 bytes.
const MAX_TEMP_ROOT_BYTES: usize = 64;

/// The path roots a [`CanonicalProfile::Package`] environment derives its
/// suite variables from.
///
/// They are not identity pins -- they are this run's sandbox -- but they are
/// part of the recorded package environment (`XDG_*`, `NEOMACS_TEST_*`,
/// `GIT_CEILING_DIRECTORIES`), so they live beside the pins rather than being
/// re-derived differently by each adapter.
#[derive(Clone, Debug)]
pub struct PackagePaths {
    pub scratch_root: PathBuf,
    pub workspace_root: PathBuf,
    pub runtime_dir: PathBuf,
    pub melpa_cache_root: PathBuf,
}

/// The one canonical environment for a test process.
#[derive(Clone, Debug)]
pub struct CanonicalEnvironment {
    profile: CanonicalProfile,
    home: PathBuf,
    tmpdir: PathBuf,
    project_root: PathBuf,
    package: Option<PackagePaths>,
}

impl CanonicalEnvironment {
    /// The longest `TMPDIR` this builder accepts for a socket-bearing profile.
    pub const MAX_TEMP_ROOT_BYTES: usize = MAX_TEMP_ROOT_BYTES;

    /// Build the canonical environment for `profile`.
    ///
    /// `home` and `tmpdir` are per-case directories the caller created; the
    /// builder does not create or delete them.  A profile that binds unix
    /// sockets in its temp root rejects a root longer than
    /// [`Self::MAX_TEMP_ROOT_BYTES`], which is why callers use
    /// [`bounded_temp_dir`] instead of an arbitrary directory.
    pub fn new(
        profile: CanonicalProfile,
        home: impl Into<PathBuf>,
        tmpdir: impl Into<PathBuf>,
        project_root: impl Into<PathBuf>,
    ) -> Result<Self, CanonicalError> {
        let home = home.into();
        let tmpdir = tmpdir.into();
        let project_root = project_root.into();
        if !home.is_absolute() {
            return Err(CanonicalError::NotAbsolute {
                field: "HOME",
                path: home,
            });
        }
        if !tmpdir.is_absolute() {
            return Err(CanonicalError::NotAbsolute {
                field: "TMPDIR",
                path: tmpdir,
            });
        }
        if profile.bounds_temp_root() {
            let bytes = tmpdir.as_os_str().len();
            if bytes > MAX_TEMP_ROOT_BYTES {
                return Err(CanonicalError::TempRootTooLong {
                    path: tmpdir,
                    bytes,
                    max: MAX_TEMP_ROOT_BYTES,
                });
            }
        }
        if profile == CanonicalProfile::Package && !project_root.is_absolute() {
            return Err(CanonicalError::MissingProjectRoot);
        }
        if profile == CanonicalProfile::Package {
            return Err(CanonicalError::MissingPackagePaths);
        }
        Ok(Self {
            profile,
            home,
            tmpdir,
            project_root,
            package: None,
        })
    }

    /// The package profile's environment: identity pins plus the suite's
    /// sandbox roots.  All roots must be absolute; the temp root is the
    /// editor's `TMPDIR`.
    pub fn package(
        home: impl Into<PathBuf>,
        tmpdir: impl Into<PathBuf>,
        paths: PackagePaths,
    ) -> Result<Self, CanonicalError> {
        let home = home.into();
        let tmpdir = tmpdir.into();
        for (field, path) in [
            ("HOME", &home),
            ("TMPDIR", &tmpdir),
            ("scratch root", &paths.scratch_root),
            ("workspace root", &paths.workspace_root),
            ("runtime dir", &paths.runtime_dir),
            ("MELPA cache root", &paths.melpa_cache_root),
        ] {
            if !path.is_absolute() {
                return Err(CanonicalError::NotAbsolute {
                    field,
                    path: path.clone(),
                });
            }
        }
        Ok(Self {
            profile: CanonicalProfile::Package,
            home,
            tmpdir,
            project_root: paths.workspace_root.clone(),
            package: Some(paths),
        })
    }

    pub fn profile(&self) -> CanonicalProfile {
        self.profile
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn tmpdir(&self) -> &Path {
        &self.tmpdir
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// The identity variables GNU's `init_editfns` derives from these pins
    /// (`src/editfns.c:90-140`), as `(variable, value)` pairs.  An in-process
    /// evaluator applies them through [`Self::lisp_environment_form`]; a
    /// spawned editor derives them itself from the environment.
    pub fn identity_bindings(&self) -> [(&'static str, String); 5] {
        let profile = self.profile;
        let login = profile.login_name().to_string();
        let host = profile.host_name().to_string();
        let full_name = profile.full_name().to_string();
        let mail = format!("{login}@{host}");
        [
            ("user-full-name", full_name),
            ("user-login-name", login.clone()),
            ("user-real-login-name", login),
            ("system-name", host),
            ("user-mail-address", mail),
        ]
    }

    /// Every pinned variable, in a stable order.
    ///
    /// `TMP`/`TEMP` are pinned alongside `TMPDIR` because the C libraries and
    /// Rust's `temp_dir` on other hosts consult them; `GIT_CEILING_DIRECTORIES`
    /// keeps a repository helper from discovering the outer checkout;
    /// `NEOVM_ORACLE_SYSTEM_NAME` is the eval program's stable `system-name`
    /// override for the Neomacs binary (an in-process evaluator uses
    /// [`Self::lisp_environment_form`] instead).
    pub fn env_pairs(&self) -> Vec<(&'static str, OsString)> {
        let profile = self.profile;
        let mut pairs: Vec<(&'static str, OsString)> = vec![
            ("LANG", OsString::from(profile.locale())),
            ("LC_ALL", OsString::from(profile.locale())),
            ("TZ", OsString::from(profile.time_zone())),
            ("HOME", self.home.clone().into_os_string()),
            ("TMPDIR", self.tmpdir.clone().into_os_string()),
            ("TMP", self.tmpdir.clone().into_os_string()),
            ("TEMP", self.tmpdir.clone().into_os_string()),
            ("USER", OsString::from(profile.login_name())),
            ("LOGNAME", OsString::from(profile.login_name())),
            ("NAME", OsString::from(profile.full_name())),
            ("HOSTNAME", OsString::from(profile.host_name())),
            (
                "EMAIL",
                OsString::from(format!("{}@{}", profile.login_name(), profile.host_name())),
            ),
            ("TERM", OsString::from("dumb")),
        ];
        match profile {
            CanonicalProfile::Oracle => {
                pairs.push((
                    "NEOVM_ORACLE_SYSTEM_NAME",
                    OsString::from(profile.host_name()),
                ));
                pairs.push((
                    "GIT_CEILING_DIRECTORIES",
                    self.project_root.clone().into_os_string(),
                ));
            }
            CanonicalProfile::Package => {
                // A recorded process environment prints this verbatim, and CI
                // exports it for the whole job.
                pairs.push(("RUST_BACKTRACE", OsString::from("1")));
                // CPython 3.11+ adds fine-grained error-location lines
                // ("    ~~~~~~^^^^^") to tracebacks; earlier interpreters do
                // not, and which python a host pins is a host property.  A
                // traceback the editor surfaces is program output, not the
                // interpreter's decoration, so disable the ranges for every
                // editor-spawned python (ignored before 3.11; main's
                // dee39f1930).
                pairs.push(("PYTHONNODEBUGRANGES", OsString::from("1")));
                let paths = self
                    .package
                    .as_ref()
                    .expect("package profile always carries its path roots");
                pairs.push((
                    "XDG_CONFIG_HOME",
                    paths.scratch_root.join("xdg/config").into_os_string(),
                ));
                pairs.push((
                    "XDG_CACHE_HOME",
                    paths.scratch_root.join("xdg/cache").into_os_string(),
                ));
                pairs.push((
                    "XDG_DATA_HOME",
                    paths.scratch_root.join("xdg/data").into_os_string(),
                ));
                pairs.push((
                    "XDG_STATE_HOME",
                    paths.scratch_root.join("xdg/state").into_os_string(),
                ));
                pairs.push((
                    "XDG_RUNTIME_DIR",
                    paths.runtime_dir.clone().into_os_string(),
                ));
                pairs.push((
                    "NEOMACS_TEST_SANDBOX_ROOT",
                    paths.scratch_root.clone().into_os_string(),
                ));
                pairs.push((
                    "NEOMACS_TEST_WORKSPACE_ROOT",
                    paths.workspace_root.clone().into_os_string(),
                ));
                pairs.push((
                    "NEOMACS_TEST_SCRATCH_ROOT",
                    paths.melpa_cache_root.clone().into_os_string(),
                ));
                // `mailcap-parse-mimetypes' reads $MIMETYPES before
                // /etc/mime.types; pin it to the suite's own database so a
                // recorded MIME type is a property of the fixture, not of
                // the host (main's e71f0e9054).
                pairs.push((
                    "MIMETYPES",
                    paths
                        .workspace_root
                        .join("crates/neomacs-melpa-test-support/fixtures/mime.types")
                        .into_os_string(),
                ));
                pairs.push((
                    "GIT_CEILING_DIRECTORIES",
                    paths.workspace_root.clone().into_os_string(),
                ));
            }
            CanonicalProfile::Tui => {}
        }
        pairs
    }

    /// Variables that must not leak in from the ambient environment.
    ///
    /// `LANGUAGE` overrides `LC_ALL` for gettext message translations, so a
    /// developer's `LANGUAGE` would translate GNU's messages out from under a
    /// recorded transcript.  `EMACSLOADPATH` redirects GNU's `load-path`
    /// before any form runs.
    pub fn removed_vars(&self) -> &'static [&'static str] {
        match self.profile {
            // `LANGUAGE` overrides `LC_ALL` for gettext translations of GNU's
            // messages; `EMACSLOADPATH` redirects GNU's `load-path`.  The TUI
            // grids were recorded with no `LC_CTYPE` in the ambient
            // environment either, and `LC_ALL` decides the category anyway.
            CanonicalProfile::Oracle => &["LANGUAGE", "EMACSLOADPATH"],
            CanonicalProfile::Tui => &["LANGUAGE", "LC_CTYPE", "EMACSLOADPATH"],
            CanonicalProfile::Package => &["EMACSLOADPATH"],
        }
    }

    /// A command for `program` that already carries every canonical pin.
    ///
    /// This is the only constructor of [`PinnedCommand`].
    pub fn command(&self, program: impl AsRef<OsStr>) -> PinnedCommand {
        let mut command = Command::new(program);
        for (name, value) in self.env_pairs() {
            command.env(name, value);
        }
        for name in self.removed_vars() {
            command.env_remove(name);
        }
        pin_child_process(&mut command);
        PinnedCommand { command }
    }

    /// Apply the canonical pins to a command type this crate does not own
    /// (the TUI harness's pty command).  The pty site is a single, named
    /// exception enforced by the harness-spawn architecture test; the ordinary
    /// path is [`Self::command`].
    pub fn apply_env_pairs(&self) -> (Vec<(&'static str, OsString)>, &'static [&'static str]) {
        (self.env_pairs(), self.removed_vars())
    }

    /// A Lisp form that installs this environment into an in-process Neomacs
    /// evaluator.
    ///
    /// A spawned editor reads the pins from its process environment.  The
    /// in-process evaluator used by the `neovm-core` compatibility harness has
    /// no child process to pin, so it gets the same answers through the
    /// Lisp-visible seams: `process-environment` (which is what `getenv`
    /// searches, `src/emacs.c:2177`) and the identity variables GNU derives in
    /// `init_editfns` (`src/editfns.c:90-140`).  The ambient environment is not
    /// replaced: the spawned engine sees ambient-plus-pins, and `setenv`'s
    /// per-name overrides reproduce exactly that.
    pub fn lisp_environment_form(&self) -> String {
        let mut form = String::from("(progn");
        for (name, value) in self.env_pairs() {
            form.push_str(&format!(
                " (setenv {} {})",
                lisp_string_quote(name),
                lisp_string_quote(&value.to_string_lossy())
            ));
        }
        for name in self.removed_vars() {
            form.push_str(&format!(" (setenv {} nil)", lisp_string_quote(name)));
        }
        form.push_str(" (setq");
        for (name, value) in self.identity_bindings() {
            form.push_str(&format!(" {} {}", name, lisp_string_quote(&value)));
        }
        // The oracle eval program pins these two for both engines
        // (`setq system-time-locale "C"`, `set-language-environment "UTF-8"`),
        // so an in-process comparison must see the same answers.
        form.push_str(" system-time-locale \"C\")");
        form.push_str(" (set-language-environment \"UTF-8\"))");
        form
    }
}

/// A `Command` that cannot exist without the canonical environment.
///
/// The fields are private and there is no `Default`/`new`: the only way to get
/// one is [`CanonicalEnvironment::command`].  Per-case overrides use [`env`]
/// and are applied after the pins by construction.
///
/// [`env`]: Self::env
pub struct PinnedCommand {
    command: Command,
}

impl std::fmt::Debug for PinnedCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PinnedCommand")
            .field("program", &self.command.get_program())
            .finish_non_exhaustive()
    }
}

impl PinnedCommand {
    /// Add or override one variable.  Overriding a canonical pin is how a case
    /// that deliberately selects another locale, time zone or TMPDIR says so;
    /// the default remains the builder's.
    pub fn env(&mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.command.env(name, value);
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.envs(vars);
        self
    }

    pub fn env_remove(&mut self, name: impl AsRef<OsStr>) -> &mut Self {
        self.command.env_remove(name);
        self
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.command.current_dir(dir);
        self
    }

    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.command.stdin(cfg);
        self
    }

    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.command.stdout(cfg);
        self
    }

    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.command.stderr(cfg);
        self
    }

    /// Put the child in its own process group (`CommandExt::process_group`).
    /// Kept here so a pinned editor can be reaped as a tree by the package
    /// probes without exposing the inner `Command`.
    #[cfg(unix)]
    pub fn process_group(&mut self, pgroup: i32) -> &mut Self {
        use std::os::unix::process::CommandExt;
        self.command.process_group(pgroup);
        self
    }

    /// Windows creation flags (suspended start), for the same reason.
    #[cfg(windows)]
    pub fn creation_flags(&mut self, flags: u32) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.command.creation_flags(flags);
        self
    }

    /// The only escape hatch, for child-side setup such as `RLIMIT_AS`; it
    /// cannot remove the pins, which were applied when this value was built.
    #[cfg(unix)]
    pub fn pre_exec(
        &mut self,
        f: impl FnMut() -> std::io::Result<()> + Send + Sync + 'static,
    ) -> &mut Self {
        use std::os::unix::process::CommandExt;
        // Safety: the caller upholds `CommandExt::pre_exec`'s contract; the
        // pins are environment entries, not child-side state, so this cannot
        // undo them.
        unsafe {
            self.command.pre_exec(f);
        }
        self
    }

    /// The current directory this command will run in, if set.  Read-only:
    /// a command's environment cannot be inspected for a way to spawn it.
    pub fn get_current_dir(&self) -> Option<&Path> {
        self.command.get_current_dir()
    }

    /// The value this command will give `name` in the child, or `None` when
    /// the variable is not mentioned.  `Some(None)` means the variable is
    /// explicitly removed from the inherited environment.
    pub fn get_env(&self, name: impl AsRef<OsStr>) -> Option<Option<&OsStr>> {
        let name = name.as_ref();
        self.command
            .get_envs()
            .find_map(|(key, value)| (key == name).then_some(value))
    }

    pub fn spawn(&mut self) -> std::io::Result<Child> {
        self.command.spawn()
    }

    pub fn output(&mut self) -> std::io::Result<Output> {
        self.command.output()
    }

    pub fn status(&mut self) -> std::io::Result<ExitStatus> {
        self.command.status()
    }
}

/// Set the child's umask before exec.
///
/// The expectations were recorded under `022` (the conventional runner
/// default): file modes appear in dired/backup/write-region probes, so an
/// inherited umask of `002` or `077` would change them.  `umask` is
/// async-signal-safe and is applied in the child, never in the test process.
fn pin_child_process(_command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        _command.pre_exec(|| {
            libc::umask(0o022);
            Ok(())
        });
    }
}

/// The canonical umask the expectations were recorded under.
pub const CANONICAL_UMASK: u32 = 0o022;

/// A per-case temp directory whose absolute path fits in a unix socket path.
///
/// `tempfile`'s default root follows the ambient `TMPDIR`, which is the
/// workspace or nextest's extraction directory on CI and a deep worktree path
/// on a developer machine.  A case that binds `AF_UNIX` sockets then fails at
/// `bind` with `ENAMETOOLONG` once the root plus the socket name crosses
/// `sun_path` (104 bytes on Darwin).  The base is therefore chosen here: the
/// ambient temp dir when it is short enough, `/tmp` otherwise.
pub fn bounded_temp_dir(prefix: &str) -> std::io::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(bounded_temp_base())
}

/// The base [`bounded_temp_dir`] creates under: the ambient temp directory
/// when that leaves room for a per-case directory and a socket name, else
/// `/tmp` (short on every Unix the suite runs on).
pub fn bounded_temp_base() -> PathBuf {
    let ambient = std::env::temp_dir();
    // The bound exists for unix-domain sockets; Windows has no `/tmp` and
    // allows longer socket names, so its own temp directory is the right
    // root.
    if !cfg!(unix) {
        return ambient;
    }
    if ambient.is_absolute() && ambient.as_os_str().len() <= 32 {
        return ambient;
    }
    PathBuf::from("/tmp")
}

fn lisp_string_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
#[path = "canonical_env_test.rs"]
mod tests;
