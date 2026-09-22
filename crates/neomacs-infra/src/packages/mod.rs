//! Deterministic MELPA package provisioning for every test suite.
//!
//! This module owns the acquisition core the package-parity harness built:
//! a locked manifest of pinned package sources, a content-addressed install
//! cache materialized **once by GNU Emacs**, and the activation vocabulary
//! tests use to mount a prepared package into an editor session.  It follows
//! the same three-step contract as [`crate::config_env`]: materialize once,
//! seal, mount per session.
//!
//! The seam that keeps this crate free of any concrete editor handle is
//! [`PackageInstallDriver`]: the runtime that drives `package-install`
//! (today `neomacs-melpa-test-support`'s attested [`EmacsRuntime`]) implements
//! the trait in its own crate, and this module only requires "configure a
//! command, run it under a timeout policy".
//!
//! [`EmacsRuntime`]: https://docs.rs/neomacs-melpa-test-support/struct.EmacsRuntime.html

pub mod activation;
pub mod install;
pub mod seal;
pub mod source_lock;
#[cfg(test)]
pub(crate) mod test_support;

pub use activation::{LoadSuffixes, PackageActivation, package_activation_elisp};
pub use install::{
    InstallCommandError, PackageInstallDriver, PathGnuDriver, configure_process_environment,
    deterministic_process_environment, elisp_string, package_preparation_run_id,
    publish_package_preparation_failure,
};
pub use seal::{ProvisionedSealReport, verify_provisioned};
pub use source_lock::{
    LockedPackageSource, SourceBuild, locked_melpa_install_plan, locked_melpa_source,
    locked_melpa_sources, preflight_locked_melpa_packages, prepare_cached_locked_melpa_package,
    prepare_cached_locked_package_plan,
};

use std::path::{Path, PathBuf};

/// A package a test wants, pinned by name and locked version.
///
/// The version must have a row in the embedded lock manifest
/// (`melpa-package-lock.tsv`); the manifest carries the full provenance
/// (upstream repository + revision, MELPA recipe revision, package-build
/// revision, dependency rows), so a provision is reproducible from data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinnedPackage {
    pub name: String,
    pub version: String,
}

impl PinnedPackage {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    pub fn as_pair(&self) -> (&str, &str) {
        (&self.name, &self.version)
    }
}

/// Shorthand for [`PinnedPackage::new`].
pub fn pin(name: impl Into<String>, version: impl Into<String>) -> PinnedPackage {
    PinnedPackage::new(name, version)
}

/// A prepared, cache-backed package ready to mount into any editor session.
#[derive(Clone, Debug)]
pub struct ProvisionedPackage {
    pin: PinnedPackage,
    /// The package's checked-out source tree below the shared cache.
    package_dir: PathBuf,
}

impl ProvisionedPackage {
    pub(crate) fn new(pin: PinnedPackage, package_dir: PathBuf) -> Self {
        Self { pin, package_dir }
    }

    pub fn name(&self) -> &str {
        &self.pin.name
    }

    pub fn version(&self) -> &str {
        &self.pin.version
    }

    /// The prepared package directory (`…/home/.emacs.d/elpa/<name>-<version>`).
    pub fn package_dir(&self) -> &Path {
        &self.package_dir
    }

    /// The elisp directory to add to `load-path` (`package_directory_list`).
    pub fn elpa_dir(&self) -> PathBuf {
        self.package_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// `NEOMACS_PACKAGE_USER_DIR` — the `package-user-dir` value for sessions
    /// that mount this package through the process environment.
    pub fn package_user_dir(&self) -> PathBuf {
        self.elpa_dir()
    }

    /// The entry source file to `load` (`NEOMACS_PACKAGE_SOURCE`).
    pub fn source_file(&self) -> PathBuf {
        self.package_dir.join(format!("{}.el", self.pin.name))
    }

    /// `(name version)` as the locked `package-load-list` entry.
    pub fn load_list_entry(&self) -> String {
        format!(
            "(list (intern \"{}\") \"{}\")",
            self.pin.name, self.pin.version
        )
    }

    /// Elisp that activates exactly this package in a session: register the
    /// package directories, then load the pinned source file.
    pub fn activation_elisp(&self) -> String {
        let source = self.source_file();
        let source = crate::packages::elisp_string(&source.to_string_lossy());
        let user_dir = crate::packages::elisp_string(&self.package_user_dir().to_string_lossy());
        let elpa_dir = crate::packages::elisp_string(&self.elpa_dir().to_string_lossy());
        format!(
            "(progn (require 'package)\
 (setq package-user-dir {user_dir}\
 package-directory-list '({elpa_dir})\
 package-load-list '({})\
 load-suffixes '(\".el\"))\
 (package-initialize)\
 (load {source} nil t t))",
            self.load_list_entry(),
        )
    }

    /// Write [`Self::activation_elisp`] to `directory/startup.el` and return
    /// its path — the `-l` mount for suites that boot editors with arguments.
    pub fn write_startup_file(&self, directory: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(directory).map_err(|error| {
            format!(
                "failed to create package startup directory {}: {error}",
                directory.display()
            )
        })?;
        let path = directory.join(format!("{}-startup.el", self.pin.name));
        std::fs::write(&path, self.activation_elisp()).map_err(|error| {
            format!(
                "failed to write package startup file {}: {error}",
                path.display()
            )
        })?;
        Ok(path)
    }
}

/// Provision a single elisp source file by content hash.
///
/// Not every test package has a lock row: GNU ELPA packages such as
/// `minibuffer-line' ship as one `.el` and several suites want the SAME
/// bytes without each embedding its own copy.  The file is stored once
/// under the shared cache keyed by its sha256, and every caller receives
/// the same path — deduplicated by construction, drift by definition
/// (different contents get different addresses).
pub fn source_file(name: &str, contents: &str) -> Result<ProvisionedSourceFile, String> {
    use sha2::{Digest, Sha256};
    let digest = hex_string(&Sha256::digest(contents.as_bytes()));
    let directory = install_cache_root()
        .join("source-files")
        .join(name)
        .join(&digest);
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "failed to create source-file cache {}: {error}",
            directory.display()
        )
    })?;
    let path = directory.join(format!("{name}.el"));
    if !path.is_file() {
        // Atomic publish: concurrent suites resolve the SAME content-
        // addressed path on a cold cache (identical bytes → identical
        // sha256), so a reader (`emacs -l <path>` at boot) must never
        // observe a truncated write.  Stage to a sibling temp file and
        // rename — rename is atomic within a directory.
        let staged = tempfile::Builder::new()
            .prefix(format!("{name}.partial-").as_str())
            .tempfile_in(&directory)
            .map_err(|error| {
                format!(
                    "failed to stage cached source file below {}: {error}",
                    directory.display()
                )
            })?;
        std::fs::write(staged.path(), contents).map_err(|error| {
            format!(
                "failed to write staged source file for {}: {error}",
                path.display()
            )
        })?;
        staged.persist(&path).map_err(|error| {
            format!(
                "failed to publish cached source file {}: {error}",
                path.display()
            )
        })?;
    }
    Ok(ProvisionedSourceFile {
        name: name.to_string(),
        path,
        sha256: digest,
    })
}

/// A content-addressed single elisp source file in the shared cache.
#[derive(Clone, Debug)]
pub struct ProvisionedSourceFile {
    name: String,
    path: PathBuf,
    sha256: String,
}

impl ProvisionedSourceFile {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The cached file's path — `load` this, or pass it as `-l`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The content hash the cache address was derived from.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn install_cache_root() -> PathBuf {
    crate::workspace_root().join("tmp/melpa/source-install-cache")
}

/// Provision a pinned package: cache hit returns the prepared directory;
/// a miss drives the install through `driver` exactly once (cross-process
/// lock), then seals and verifies the prepared tree.
///
/// This is the function suites call; [`prepare_cached_locked_melpa_package`]
/// is the lower-level form the package-parity harness uses directly.
pub fn provision(
    pinned: &PinnedPackage,
    driver: &dyn PackageInstallDriver,
) -> Result<ProvisionedPackage, String> {
    let package_dir = prepare_cached_locked_melpa_package(driver, pinned.as_pair())?;
    let provisioned = ProvisionedPackage::new(pinned.clone(), package_dir);
    seal::seal_provisioned(&provisioned)?;
    Ok(provisioned)
}
