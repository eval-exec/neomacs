use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    EmacsRuntime, PackageEnvironmentEntry, elisp_string, os_string,
    prepare_cached_locked_melpa_package,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageActivation {
    SourceFile,
    InstalledAutoloads,
}

pub fn package_activation_elisp(activation: PackageActivation) -> &'static str {
    match activation {
        PackageActivation::SourceFile => r#"(load (getenv "NEOMACS_PACKAGE_SOURCE") nil t t)"#,
        PackageActivation::InstalledAutoloads => "nil",
    }
}

/// An immutable, prepared package graph that can be observed through more
/// than one editor adapter.
#[derive(Clone, Debug)]
pub struct PreparedPackageSet {
    package_name: String,
    package_user_dir: PathBuf,
    package_directory_list: Vec<PathBuf>,
    package_load_list: Vec<(String, String)>,
    source_file: PathBuf,
    activation: PackageActivation,
    prelude: String,
}

impl PreparedPackageSet {
    /// Prepare a revision-pinned MELPA package and select its source file.
    pub fn from_locked_melpa(
        gnu_emacs: &EmacsRuntime,
        package: (&str, &str),
        source_file_name: &str,
    ) -> Result<Self, String> {
        let package_dir = prepare_cached_locked_melpa_package(gnu_emacs, package)?;
        Self::from_package_dir(package, source_file_name, package_dir)
    }

    /// Describe an already prepared package directory.
    pub fn from_package_dir(
        package: (&str, &str),
        source_file_name: &str,
        package_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        validate_source_file_name(source_file_name)?;
        let package_dir = package_dir.into();
        let source_file = package_dir.join(source_file_name);
        if !source_file.is_file() {
            return Err(format!(
                "prepared {} source `{source_file_name}` is missing below {}",
                package.0,
                package_dir.display()
            ));
        }
        let package_user_dir = package_dir
            .parent()
            .ok_or_else(|| {
                format!(
                    "prepared package directory {} has no package root",
                    package_dir.display()
                )
            })?
            .to_path_buf();
        Ok(Self {
            package_name: package.0.to_string(),
            package_user_dir,
            package_directory_list: Vec::new(),
            package_load_list: vec![(package.0.to_string(), package.1.to_string())],
            source_file,
            activation: PackageActivation::SourceFile,
            prelude: String::new(),
        })
    }

    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    pub fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.prelude = prelude.into();
        self
    }

    pub fn with_installed_autoloads(mut self) -> Self {
        self.activation = PackageActivation::InstalledAutoloads;
        self
    }

    /// Add an exact dependency that has already been prepared below an ELPA
    /// package directory.
    pub fn with_prepared_dependency(
        mut self,
        package: (&str, &str),
        package_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let package_dir = package_dir.into();
        let package_directory = package_dir
            .parent()
            .ok_or_else(|| {
                format!(
                    "prepared dependency directory {} has no package root",
                    package_dir.display()
                )
            })?
            .to_path_buf();
        if !self.package_directory_list.contains(&package_directory) {
            self.package_directory_list.push(package_directory);
        }
        if let Some((_, pinned_version)) = self
            .package_load_list
            .iter()
            .find(|(pinned_name, _)| pinned_name == package.0)
        {
            if pinned_version != package.1 {
                return Err(format!(
                    "package `{}` is already pinned to version `{pinned_version}`, cannot also pin `{}`",
                    package.0, package.1
                ));
            }
        } else {
            self.package_load_list
                .push((package.0.to_string(), package.1.to_string()));
        }
        Ok(self)
    }

    /// Environment required by either a batch or PTY editor adapter.
    pub fn process_environment(&self) -> Vec<PackageEnvironmentEntry> {
        vec![
            (
                OsString::from("NEOMACS_PACKAGE_USER_DIR"),
                os_string(self.package_user_dir.as_os_str()),
            ),
            (
                OsString::from("NEOMACS_PACKAGE_SOURCE"),
                os_string(self.source_file.as_os_str()),
            ),
        ]
    }

    /// Elisp that initializes exactly this package graph.
    pub fn startup_elisp(&self) -> String {
        let package_directory_list = self
            .package_directory_list
            .iter()
            .map(|directory| elisp_string(&directory.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");
        let package_load_list = self
            .package_load_list
            .iter()
            .map(|(name, version)| {
                format!(
                    "(list (intern {}) {})",
                    elisp_string(name),
                    elisp_string(version)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let activation = package_activation_elisp(self.activation);
        format!(
            r##";;; -*- lexical-binding: t; -*-
(progn
                   (require 'package)
                   (setq package-user-dir
                         (getenv "NEOMACS_PACKAGE_USER_DIR")
                         package-directory-list
                         (list {package_directory_list})
                         package-load-list
                         (list 'all {package_load_list})
                         load-suffixes '(".el"))
                   (package-initialize)
                   {}
                   {})"##,
            self.prelude, activation
        )
    }

    /// Write the startup form to a stable file suitable for `--load` in a PTY
    /// launch, avoiding shell or command-line quoting.
    pub fn write_startup_file(&self, directory: &Path) -> Result<PathBuf, String> {
        fs::create_dir_all(directory).map_err(|error| {
            format!(
                "failed to create package startup directory {}: {error}",
                directory.display()
            )
        })?;
        let path = directory.join(format!("{}-startup.el", self.package_name));
        fs::write(&path, self.startup_elisp()).map_err(|error| {
            format!(
                "failed to write package startup file {}: {error}",
                path.display()
            )
        })?;
        Ok(path)
    }
}

fn validate_source_file_name(source_file_name: &str) -> Result<(), String> {
    let mut components = Path::new(source_file_name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(format!(
            "prepared package source must be one file name, got `{source_file_name}`"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().expect("create package fixture");
        let package_dir = root.path().join("elpa/example-1.2.3");
        fs::create_dir_all(&package_dir).expect("create prepared package directory");
        fs::write(package_dir.join("example.el"), "(provide 'example)\n")
            .expect("write prepared package source");
        (root, package_dir)
    }

    #[test]
    fn prepared_package_set_exports_one_reusable_startup_contract() {
        let (root, package_dir) = fixture();
        let dependency_dir = root.path().join("dependencies/compat-30.1.0.0");
        fs::create_dir_all(&dependency_dir).expect("create dependency directory");
        let prepared =
            PreparedPackageSet::from_package_dir(("example", "1.2.3"), "example.el", package_dir)
                .expect("describe prepared package")
                .with_prepared_dependency(("compat", "30.1.0.0"), dependency_dir)
                .expect("add exact dependency")
                .with_prelude("(setq example-test-ready t)");

        let environment = prepared.process_environment();
        assert_eq!(environment.len(), 2);
        assert_eq!(environment[0].0, "NEOMACS_PACKAGE_USER_DIR");
        assert_eq!(environment[1].0, "NEOMACS_PACKAGE_SOURCE");

        let startup = prepared.startup_elisp();
        assert!(startup.starts_with(";;; -*- lexical-binding: t; -*-"));
        assert!(startup.contains("(package-initialize)"));
        assert!(startup.contains("example-test-ready"));
        assert!(startup.contains("compat"));
        assert!(startup.contains("30.1.0.0"));
        assert!(startup.contains("NEOMACS_PACKAGE_SOURCE"));
    }

    #[test]
    fn prepared_package_set_writes_the_same_startup_contract_for_pty_launches() {
        let (root, package_dir) = fixture();
        let prepared =
            PreparedPackageSet::from_package_dir(("example", "1.2.3"), "example.el", package_dir)
                .expect("describe prepared package");

        let startup_file = prepared
            .write_startup_file(&root.path().join("launch"))
            .expect("write startup file");
        assert_eq!(
            fs::read_to_string(startup_file).expect("read startup file"),
            prepared.startup_elisp()
        );
    }
}
