//! Installing an exact package release from an ELPA-style archive into a
//! validated, cross-process cache.
//!
//! This lives beside `source_lock`'s MELPA machinery rather than in
//! `neomacs-melpa-tests` because BENCHMARKS need it too: `neomacs-perf`
//! depends on this crate, and the tests crate depends on this crate, so a
//! fetcher that only the tests crate could reach was unavailable to the one
//! consumer that has to pin a third-party suite (GNU ELPA's
//! `elisp-benchmarks`).  `neomacs-melpa-tests` re-exports it, so its existing
//! callers are unchanged.

use std::fs;
use std::path::PathBuf;

use crate::{
    CommandError, EmacsRuntime, configure_process_environment, elisp_string, output_with_timeout,
    package_preparation_run_id, publish_package_preparation_failure, workspace_root,
};

#[derive(Clone, Copy)]
pub struct PackageArchiveSpec {
    cache_directory: &'static str,
    /// Human-readable archive name, used in diagnostics.
    pub label: &'static str,
    name: &'static str,
    url: &'static str,
}

/// The GNU ELPA archive this crate installs pinned releases from.
pub const GNU_ELPA_ARCHIVE: PackageArchiveSpec = PackageArchiveSpec {
    cache_directory: "package-cache-gnu-elpa",
    label: "GNU ELPA",
    name: "gnu",
    url: "https://elpa.gnu.org/packages/",
};

/// Install one exact GNU ELPA package into a validated, cross-process cache.
///
/// Like the MELPA cache, this remains a workspace-local runtime artifact.
pub fn prepare_cached_gnu_elpa_package(
    gnu_emacs: &EmacsRuntime,
    package: (&str, &str),
) -> Result<PathBuf, String> {
    prepare_cached_package(gnu_emacs, package, GNU_ELPA_ARCHIVE)
}

fn prepare_cached_package(
    gnu_emacs: &EmacsRuntime,
    package: (&str, &str),
    archive: PackageArchiveSpec,
) -> Result<PathBuf, String> {
    let (name, version) = package;
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
            "cached {} package must have a safe hard-coded name and version, got `{name}` `{version}`",
            archive.label
        ));
    }

    let root = workspace_root()
        .join("tmp/melpa")
        .join(archive.cache_directory)
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
    let expected_marker = format!("{name}\t{version}\n");
    let cache_is_ready = descriptor.is_file()
        && fs::read_to_string(&ready_marker).is_ok_and(|contents| contents == expected_marker);
    if cache_is_ready {
        return Ok(package_dir);
    }
    let failure_prefix = format!(
        "run-id\t{}\nidentity\t{expected_marker}error\n",
        package_preparation_run_id()
    );
    if let Ok(contents) = fs::read_to_string(&failed_marker)
        && let Some(error) = contents.strip_prefix(&failure_prefix)
    {
        return Err(error.to_string());
    }

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

    let name_string = elisp_string(name);
    let version_string = elisp_string(version);
    let archive_name_string = elisp_string(archive.name);
    let archive_url_string = elisp_string(archive.url);
    let package_archives = format!(
        r##"(list
                      (cons {archive_name_string}
                            {archive_url_string}))"##
    );
    let form = format!(
        r##"(progn
               (require 'package)
               (setq package-user-dir
                     (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                     package-check-signature nil
                     package-archives {package_archives})
               (package-refresh-contents)
               (let* ((package-name {name_string})
                      (expected-version {version_string})
                      (package-symbol (intern package-name))
                      (description
                       (cadr
                        (assq package-symbol package-archive-contents)))
                      (archive-version
                       (and description
                            (package-version-join
                             (package-desc-version description)))))
                 (unless description
                   (error "Package is absent from selected archive: %s"
                          package-name))
                 (unless (equal archive-version expected-version)
                   (error
                    "Package version changed: %s expected %s, current %s"
                    package-name expected-version archive-version))
                 (package-install description)
                 (package-initialize)
                 (let* ((installed
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
    );
    let mut command = gnu_emacs.command();
    configure_process_environment(&mut command, &root, &home, &tmp);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output = match output_with_timeout(&mut command, gnu_emacs.timeout) {
        Ok(output) => output,
        Err(error) => {
            let error = match error {
                CommandError::Launch(error) => format!(
                    "failed to launch {} for cached package `{name}` in {}: {error}",
                    gnu_emacs.name,
                    root.display()
                ),
                CommandError::TimedOut(_) => format!(
                    "{} cached package `{name}` timed out after {:?} in {}",
                    gnu_emacs.name,
                    gnu_emacs.timeout,
                    root.display()
                ),
                CommandError::Capture(error) => format!(
                    "failed to capture {} cached package `{name}` output: {error}",
                    gnu_emacs.name
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
            "failed to prepare cached {} package {name} {version} below {}\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            archive.label,
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
    fs::write(&marker_tmp, &expected_marker).map_err(|error| {
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
