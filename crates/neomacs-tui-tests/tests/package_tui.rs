//! Interactive package-TUI parity: real MELPA packages rendered in real
//! PTY pairs.
//!
//! This is a distinct Cargo test target so package-screen compatibility can
//! be selected without compiling test-name conventions into CI shell code.
//!
//! The suite moved here from `neomacs-melpa-tests`: package acquisition is
//! shared (`neomacs-melpa-test-support`, `neomacs-infra::packages`), so
//! "MELPA package exercised through a terminal" is a TUI-suite concern, and
//! the MELPA CI jobs carry batch parity only.
//!
//! Conventions:
//! * pins are name/version rows in the lock manifests — provenance data,
//!   never vendored source;
//! * [`PackageOracle`] is the provisioning front-door (the old
//!   `CachedMelpaOracle` constructor chain over `PreparedPackageSet`);
//! * [`scenario`] re-exports the PTY-pair adapter
//!   (`PackageTuiScenario`, readiness/timeout checkpoints).

#![cfg(unix)]

use neomacs_melpa_test_support::EmacsRuntime;
pub use neomacs_melpa_test_support::PreparedPackageSet;

pub mod scenario {
    pub use neomacs_tui_tests::package_scenario::{
        DisplayCheckpoint, PackageTuiPair, PackageTuiScenario, PairTimeout, ReadinessCheckpoint,
        TerminalProfile,
    };
}

// ── Pins (rows of the lock manifests) ──────────────────────────────────

pub const ACE_WINDOW_MELPA_PIN: (&str, &str) = ("ace-window", "20220911.358");
pub const BEACON_MELPA_PIN: (&str, &str) = ("beacon", "20220730.100");
pub const COMPAT_GNU_ELPA_PIN: (&str, &str) = ("compat", "31.0.0.2");
pub const CORFU_MELPA_PIN: (&str, &str) = ("corfu", "20260802.2028");
pub const GRUVBOX_THEME_MELPA_PIN: (&str, &str) = ("gruvbox-theme", "20250117.222");
pub const HELM_CORE_MELPA_PIN: (&str, &str) = ("helm-core", "20260720.1307");
pub const HELM_CSS_SCSS_MELPA_PIN: (&str, &str) = ("helm-css-scss", "20230522.1113");
pub const HELM_GITIGNORE_MELPA_PIN: (&str, &str) = ("helm-gitignore", "20230310.1829");
pub const HELM_PYDOC_MELPA_PIN: (&str, &str) = ("helm-pydoc", "20220721.433");
pub const LEUVEN_THEME_MELPA_PIN: (&str, &str) = ("leuven-theme", "20260213.1052");
pub const MAGIT_MELPA_PIN: (&str, &str) = ("magit", "20260724.2338");
pub const MWIM_MELPA_PIN: (&str, &str) = ("mwim", "20260227.705");
pub const ORDERLESS_MELPA_PIN: (&str, &str) = ("orderless", "20260519.1029");
pub const VERTICO_MELPA_PIN: (&str, &str) = ("vertico", "20260805.1129");

// ── Provisioning front-door ────────────────────────────────────────────

/// Differential oracle for one exact package cached below `./tmp`.
///
/// The constructor chain mirrors the old `CachedMelpaOracle` from the
/// package-parity crate, over the same `PreparedPackageSet`.
#[derive(Clone)]
pub struct PackageOracle {
    packages: PreparedPackageSet,
}

/// MELPA-focused name retained for package-specific parity modules.
pub type CachedMelpaOracle = PackageOracle;

impl PackageOracle {
    /// Build an exact revision-pinned package from source and select its file.
    pub fn new(package: (&str, &str), source_file_name: &str) -> Result<Self, String> {
        Ok(Self {
            packages: PreparedPackageSet::from_locked_melpa(
                &EmacsRuntime::gnu_emacs(),
                package,
                source_file_name,
            )?,
        })
    }

    /// Prepare one pinned GNU ELPA package and select its Elisp source file.
    pub fn new_from_gnu_elpa(
        package: (&str, &str),
        source_file_name: &str,
    ) -> Result<Self, String> {
        let package_dir = neomacs_melpa_test_support::prepare_cached_gnu_elpa_package(
            &EmacsRuntime::gnu_emacs(),
            package,
        )?;
        Ok(Self {
            packages: PreparedPackageSet::from_package_dir(package, source_file_name, package_dir)?,
        })
    }

    /// Evaluate an additional setup form before loading the package source.
    pub fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.packages = self.packages.with_prelude(prelude);
        self
    }

    /// Mount one exact pinned package as an additional dependency.
    pub fn with_gnu_elpa_dependency(self, package: (&str, &str)) -> Result<Self, String> {
        let package_dir = neomacs_melpa_test_support::prepare_cached_gnu_elpa_package(
            &EmacsRuntime::gnu_emacs(),
            package,
        )?;
        self.with_prepared_dependency(package, package_dir)
    }

    fn with_prepared_dependency(
        self,
        package: (&str, &str),
        package_dir: std::path::PathBuf,
    ) -> Result<Self, String> {
        self.packages
            .with_prepared_dependency(package, package_dir)
            .map(|packages| Self { packages })
    }

    /// Exercise the package state established by `package-initialize` without
    /// loading the selected source file afterward.
    pub fn with_installed_autoloads(mut self) -> Self {
        self.packages = self.packages.with_installed_autoloads();
        self
    }

    /// Mount one exact pinned MELPA package as an additional dependency.
    pub fn with_melpa_dependency(self, package: (&str, &str)) -> Result<Self, String> {
        let package_dir = neomacs_melpa_test_support::prepare_cached_locked_melpa_package(
            &EmacsRuntime::gnu_emacs(),
            package,
        )?;
        self.with_prepared_dependency(package, package_dir)
    }

    /// The immutable package setup shared by batch and interactive adapters.
    pub fn prepared_packages(&self) -> &PreparedPackageSet {
        &self.packages
    }
}

// ── The moved package-screen suites ────────────────────────────────────

#[path = "package_tui/ace_window_test.rs"]
mod ace_window_test;

#[path = "package_tui/beacon_test.rs"]
mod beacon_test;

#[path = "package_tui/corfu_test.rs"]
mod corfu_test;

#[path = "package_tui/gruvbox_theme_test.rs"]
mod gruvbox_theme_test;

#[path = "package_tui/helm_css_scss_test.rs"]
mod helm_css_scss_test;

#[path = "package_tui/helm_gitignore_test.rs"]
mod helm_gitignore_test;

#[path = "package_tui/helm_pydoc_test.rs"]
mod helm_pydoc_test;

#[path = "package_tui/leuven_theme_test.rs"]
mod leuven_theme_test;

#[path = "package_tui/magit_test.rs"]
mod magit_test;

#[path = "package_tui/mwim_test.rs"]
mod mwim_test;

#[path = "package_tui/vertico_test.rs"]
mod vertico_test;
