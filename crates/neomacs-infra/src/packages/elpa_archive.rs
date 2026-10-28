//! Installing exact GNU ELPA package releases from vendored, checksummed
//! tarballs into a validated, cross-process cache.
//!
//! The pinned set lives in `elpa-package-lock.tsv`: package, version, the
//! upstream commit the release was published from, and the sha256 of the
//! release tarball committed under `fixtures/gnu-elpa/`.  GNU Emacs installs
//! the tarball with `package-install-file`; the archive URL is never contacted.
//!
//! The archive was contacted once, through `package-refresh-contents`, and
//! that made these tests depend on runner egress: GNU's
//! `package--download-and-read-archives` only *messages* "Failed to download
//! `gnu' archive" and returns, so the install then fails as "Package is
//! absent from selected archive".  A test that fetches from the live internet
//! is nondeterministic by construction; a checksummed fixture is not.
//!
//! This lives beside `source_lock`'s MELPA machinery rather than in
//! `neomacs-melpa-tests` because BENCHMARKS need it too: `neomacs-perf`
//! depends on this crate, and the tests crate depends on this crate, so a
//! fetcher that only the tests crate could reach was unavailable to the one
//! consumer that has to pin a third-party suite (GNU ELPA's
//! `elisp-benchmarks`).  `neomacs-melpa-tests` re-exports it, so its existing
//! callers are unchanged.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::elpa_lock::{LockedElpaSource, locked_elpa_source};
use super::install::{
    InstallCommandError, PackageInstallDriver, elisp_string, package_environment_with_runtime,
    package_preparation_run_id, publish_package_preparation_failure,
};

/// Human-readable archive name, used in diagnostics.
pub const GNU_ELPA_LABEL: &str = "GNU ELPA";

/// Cache directory below [`crate::melpa_cache_root`].
const CACHE_DIRECTORY: &str = "package-cache-gnu-elpa";

/// Fixture directory below [`crate::workspace_root`].
const FIXTURE_DIRECTORY: &str = "crates/neomacs-infra/fixtures/gnu-elpa";

/// A pin in the vendored GNU ELPA set.
///
/// The associated constants are the only constructors (`new` is private), so
/// a suite can name a pinned package but cannot invent one: requesting an
/// un-vendored GNU ELPA package is not expressible.  [`Self::from_pin`] is the
/// runtime bridge for the string pins that the lock manifest dispatches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GnuElpaPackage {
    name: &'static str,
    version: &'static str,
}

impl GnuElpaPackage {
    const fn new(name: &'static str, version: &'static str) -> Self {
        Self { name, version }
    }

    pub const ASYNC: Self = Self::new("async", "2.0");
    pub const AUCTEX: Self = Self::new("auctex", "14.2.0");
    pub const BIND_KEY: Self = Self::new("bind-key", "2.4.1");
    pub const COMPAT: Self = Self::new("compat", "31.1.0.0");
    pub const ELISP_BENCHMARKS: Self = Self::new("elisp-benchmarks", "1.16");
    pub const LET_ALIST: Self = Self::new("let-alist", "1.0.6");
    pub const MINIBUFFER_LINE: Self = Self::new("minibuffer-line", "0.1");
    pub const QUEUE: Self = Self::new("queue", "0.2");
    pub const USE_PACKAGE: Self = Self::new("use-package", "2.4.6");

    /// Every pinned package; the lock manifest must describe exactly this set.
    pub const ALL: [Self; 9] = [
        Self::ASYNC,
        Self::AUCTEX,
        Self::BIND_KEY,
        Self::COMPAT,
        Self::ELISP_BENCHMARKS,
        Self::LET_ALIST,
        Self::MINIBUFFER_LINE,
        Self::QUEUE,
        Self::USE_PACKAGE,
    ];

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn version(self) -> &'static str {
        self.version
    }

    /// GNU ELPA packages this release requires.
    ///
    /// `package-install-file` installs exactly one tarball and resolves no
    /// dependencies, while the archive path this replaces did.  The installer
    /// therefore materializes this closure itself, from fixtures; an entry
    /// here must itself be a pinned [`GnuElpaPackage`].
    pub fn dependencies(self) -> &'static [Self] {
        match self.name {
            // use-package.el requires use-package-bind-key, which requires
            // bind-key (`Package-Requires: ((emacs "24.3") (bind-key "2.4"))`).
            "use-package" => &[Self::BIND_KEY],
            _ => &[],
        }
    }

    /// The pinned package with exactly this name and version, if any.
    pub fn from_pin(name: &str, version: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|package| package.name == name && package.version == version)
    }

    /// The lock row (upstream commit and fixture sha256) for this pin.
    pub fn source(self) -> Result<LockedElpaSource, String> {
        locked_elpa_source(self.name, self.version)
    }

    /// The workspace-relative path of the committed release tarball, for
    /// diagnostics and measurement provenance.
    pub fn fixture_identity(self) -> String {
        format!("{FIXTURE_DIRECTORY}/{}-{}.tar", self.name, self.version)
    }

    /// The committed release tarball for this pin.
    fn fixture_path(self) -> PathBuf {
        crate::workspace_root().join(self.fixture_identity())
    }

    /// This package plus its GNU ELPA dependencies, dependencies first.
    fn install_order(self) -> Vec<Self> {
        let mut visiting = Vec::new();
        let mut ordered = Vec::new();
        self.collect_install_order(&mut visiting, &mut ordered);
        ordered
    }

    fn collect_install_order(self, visiting: &mut Vec<Self>, ordered: &mut Vec<Self>) {
        if visiting.contains(&self) {
            return;
        }
        visiting.push(self);
        for dependency in self.dependencies() {
            dependency.collect_install_order(visiting, ordered);
        }
        ordered.push(self);
    }

    /// Read and checksum this pin's fixture, naming the file on every failure.
    fn verified_tarball(self, source: &LockedElpaSource) -> Result<Vec<u8>, String> {
        self.verified_tarball_at(&self.fixture_path(), source)
    }

    fn verified_tarball_at(
        self,
        path: &Path,
        source: &LockedElpaSource,
    ) -> Result<Vec<u8>, String> {
        let bytes = std::fs::read(path).map_err(|error| {
            format!(
                "vendored {GNU_ELPA_LABEL} fixture for {} {} is missing or unreadable: {}\n\
                 expected sha256 {}\n\
                 pinned GNU ELPA packages install from checksummed tarballs committed under \
                 {FIXTURE_DIRECTORY}; restore the fixture (the archive URL is never fetched at \
                 test time): {error}",
                source.name,
                source.version,
                path.display(),
                source.sha256
            )
        })?;
        let actual = hex_digest(&Sha256::digest(&bytes));
        if actual != source.sha256 {
            return Err(format!(
                "vendored {GNU_ELPA_LABEL} fixture {} for {} {} does not match its pinned \
                 checksum: expected sha256 {}, got {}\n\
                 replace the tarball and its elpa-package-lock.tsv row together",
                path.display(),
                source.name,
                source.version,
                source.sha256,
                actual
            ));
        }
        Ok(bytes)
    }
}

/// One checksummed tarball resolved for an install, before materialization.
struct PinnedTarball {
    source: LockedElpaSource,
    bytes: Vec<u8>,
}

/// One materialized tarball handed to the install editor.
struct PreparedTarball {
    source: LockedElpaSource,
    path: PathBuf,
}

/// Install one pinned GNU ELPA package — and its GNU ELPA dependencies — from
/// vendored tarballs into a validated, cross-process cache.
///
/// Like the MELPA cache, this remains a workspace-local runtime artifact.  A
/// missing or corrupt fixture is an error naming the file and its pinned
/// checksum; it is never skipped and never falls back to the network.
pub fn prepare_cached_gnu_elpa_package(
    driver: &dyn PackageInstallDriver,
    package: GnuElpaPackage,
) -> Result<PathBuf, String> {
    let installs = package.install_order();
    for install in &installs {
        validate_identity(install.name(), install.version())?;
    }
    prepare_cached_package(driver, package, &installs)
}

fn prepare_cached_package(
    driver: &dyn PackageInstallDriver,
    package: GnuElpaPackage,
    installs: &[GnuElpaPackage],
) -> Result<PathBuf, String> {
    let (name, version) = (package.name(), package.version());
    let root = crate::melpa_cache_root()
        .join(CACHE_DIRECTORY)
        .join(name)
        .join(version);
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create package cache root {}: {error}",
            root.display()
        )
    })?;
    let lock_path = root.join("prepare.lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "failed to open package cache lock {}: {error}",
                lock_path.display()
            )
        })?;
    fs4::FileExt::lock(&lock)
        .map_err(|error| format!("failed to lock package cache {}: {error}", root.display()))?;

    let home = root.join("home");
    let tmp = root.join("tmp");
    let package_dir = home.join(".emacs.d/elpa").join(format!("{name}-{version}"));
    let descriptor = package_dir.join(format!("{name}-pkg.el"));
    let ready_marker = root.join("ready");
    let failed_marker = root.join("failed");
    // The cache identity carries every fixture checksum in the install
    // closure, so replacing a tarball invalidates an older prepared tree
    // instead of silently serving bytes from the previous fixture.
    let mut identity = format!("{name}\t{version}\n");
    for install in installs {
        let source = install.source()?;
        identity.push_str(&format!(
            "{}\t{}\t{}\n",
            source.name, source.version, source.sha256
        ));
    }
    let cache_is_ready = descriptor.is_file()
        && fs::read_to_string(&ready_marker).is_ok_and(|contents| contents == identity);
    if cache_is_ready {
        return Ok(package_dir);
    }
    let failure_prefix = format!(
        "run-id\t{}\nidentity\t{identity}error\n",
        package_preparation_run_id()
    );
    if let Ok(contents) = fs::read_to_string(&failed_marker)
        && let Some(error) = contents.strip_prefix(&failure_prefix)
    {
        return Err(error.to_string());
    }

    // Resolve and verify every fixture before disturbing a previous tree: a
    // missing or corrupt tarball must leave the cache exactly as it was.
    let pinned = installs
        .iter()
        .map(|install| {
            let source = install.source()?;
            let bytes = install.verified_tarball(&source)?;
            Ok(PinnedTarball { source, bytes })
        })
        .collect::<Result<Vec<_>, String>>()?;

    if home.exists() {
        fs::remove_dir_all(&home).map_err(|error| {
            format!(
                "failed to remove incomplete package cache {}: {error}",
                home.display()
            )
        })?;
    }
    if ready_marker.exists() {
        fs::remove_file(&ready_marker).map_err(|error| {
            format!(
                "failed to remove invalid package cache marker {}: {error}",
                ready_marker.display()
            )
        })?;
    }
    if failed_marker.exists() {
        fs::remove_file(&failed_marker).map_err(|error| {
            format!(
                "failed to remove stale package preparation failure {}: {error}",
                failed_marker.display()
            )
        })?;
    }
    for directory in [
        home.join(".emacs.d"),
        tmp.clone(),
        root.join("xdg/config"),
        root.join("xdg/cache"),
        root.join("xdg/data"),
        root.join("xdg/state"),
    ] {
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "failed to create package cache directory {}: {error}",
                directory.display()
            )
        })?;
    }

    let vendor_directory = root.join("vendor");
    fs::create_dir_all(&vendor_directory).map_err(|error| {
        format!(
            "failed to create vendored tarball directory {}: {error}",
            vendor_directory.display()
        )
    })?;
    let mut prepared = Vec::with_capacity(pinned.len());
    for tarball in pinned {
        let path = vendor_directory.join(format!(
            "{}-{}.tar",
            tarball.source.name, tarball.source.version
        ));
        publish_tarball(&vendor_directory, &path, &tarball.bytes)?;
        prepared.push(PreparedTarball {
            source: tarball.source,
            path,
        });
    }

    let form = install_form(&prepared);
    let environment =
        package_environment_with_runtime(&root, &home, &tmp, &root.join("xdg/runtime"))
            .expect("package sandbox roots must be absolute");
    let mut command = driver.command(&environment);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output = match driver.run(&mut command) {
        Ok(output) => output,
        Err(error) => {
            let error = match error {
                InstallCommandError::Launch(error) => format!(
                    "failed to launch {} for cached package `{name}` in {}: {error}",
                    driver.name(),
                    root.display()
                ),
                InstallCommandError::TimedOut(_) => format!(
                    "{} cached package `{name}` timed out after {:?} in {}",
                    driver.name(),
                    driver.timeout(),
                    root.display()
                ),
                InstallCommandError::Capture(error) => format!(
                    "failed to capture {} cached package `{name}` output: {error}",
                    driver.name()
                ),
            };
            return Err(publish_package_preparation_failure(
                &failed_marker,
                &failure_prefix,
                error,
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || !stdout.contains("NEOMACS-PACKAGE-CACHE:ready")
        || !descriptor.is_file()
    {
        let error = format!(
            "failed to prepare cached {GNU_ELPA_LABEL} package {name} {version} from the vendored \
             fixture below {}\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            root.display(),
            output.status.code()
        );
        return Err(publish_package_preparation_failure(
            &failed_marker,
            &failure_prefix,
            error,
        ));
    }

    let marker_tmp = root.join(format!("ready.{}.tmp", std::process::id()));
    fs::write(&marker_tmp, &identity).map_err(|error| {
        format!(
            "failed to write package cache marker {}: {error}",
            marker_tmp.display()
        )
    })?;
    fs::rename(&marker_tmp, &ready_marker).map_err(|error| {
        format!(
            "failed to publish package cache marker {}: {error}",
            ready_marker.display()
        )
    })?;
    Ok(package_dir)
}

fn validate_identity(name: &str, version: &str) -> Result<(), String> {
    if name.is_empty()
        || version.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '@'))
        || !version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
    {
        return Err(format!(
            "cached {GNU_ELPA_LABEL} package must have a safe hard-coded name and version, \
             got `{name}` `{version}`"
        ));
    }
    Ok(())
}

/// Publish a tarball into the cache atomically, so a crashed install never
/// leaves a partial file for a later run to hand to GNU Emacs.
fn publish_tarball(directory: &Path, path: &Path, bytes: &[u8]) -> Result<(), String> {
    if fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    let staged = tempfile::Builder::new()
        .prefix("tarball.partial-")
        .tempfile_in(directory)
        .map_err(|error| {
            format!(
                "failed to stage vendored tarball below {}: {error}",
                directory.display()
            )
        })?;
    fs::write(staged.path(), bytes).map_err(|error| {
        format!(
            "failed to write staged vendored tarball for {}: {error}",
            path.display()
        )
    })?;
    staged.persist(path).map_err(|error| {
        format!(
            "failed to publish vendored tarball {}: {error}",
            path.display()
        )
    })?;
    Ok(())
}

/// The GNU Emacs form that installs the materialized closure and verifies
/// every installed version and descriptor.
///
/// `package-install-file` reads the tarball and nothing else: no
/// `package-refresh-contents`, no `package-archives`, no URL.
fn install_form(installs: &[PreparedTarball]) -> String {
    let tarballs = installs
        .iter()
        .map(|install| elisp_string(&install.path.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    let pins = installs
        .iter()
        .map(|install| {
            format!(
                "(cons {} {})",
                elisp_string(install.source.name),
                elisp_string(install.source.version)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r##"(progn
               (require 'package)
               (setq package-user-dir
                     (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                     package-check-signature nil)
               (dolist (tarball (list {tarballs}))
                 (package-install-file tarball))
               (package-initialize)
               (dolist (pin (list {pins}))
                 (let* ((package-name (car pin))
                        (expected-version (cdr pin))
                        (package-symbol (intern package-name))
                        (installed
                         (cadr (assq package-symbol package-alist)))
                        (installed-version
                         (and installed
                              (package-version-join
                               (package-desc-version installed))))
                        (directory
                         (and installed (package-desc-dir installed)))
                        (descriptor
                         (and directory
                              (expand-file-name
                               (concat package-name "-pkg.el")
                               directory))))
                   (unless (equal installed-version expected-version)
                     (error
                      "Installed package version mismatch: %s expected %s, got %s"
                      package-name expected-version installed-version))
                   (unless (and descriptor (file-readable-p descriptor))
                     (error
                      "Installed package descriptor is unreadable: %s"
                      descriptor))))
               (princ "NEOMACS-PACKAGE-CACHE:ready"))"##
    )
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "elpa_archive/tests/vendor_test.rs"]
mod tests;
