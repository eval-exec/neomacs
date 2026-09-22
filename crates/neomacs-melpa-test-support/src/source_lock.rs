//! Facade over the package acquisition core.
//!
//! The implementation moved to `neomacs-infra`'s `packages` module (the
//! content-addressed install cache, the locked package manifest, and the
//! activation vocabulary) so *every* suite — GUI, TUI, oracle, perf — can
//! provision the same pinned packages through one shared cache.  This module
//! keeps the historical `neomacs_melpa_test_support::source_lock::*` paths
//! alive for the package-parity harness's callers, and implements the
//! install driver for this crate's attested [`EmacsRuntime`] (the trait seam
//! lives in `neomacs-infra`, which never depends on a concrete runtime).

pub use neomacs_infra::packages::source_lock::{
    LockedPackageSource, SHALLOW_GIT_FETCH_ARGS, SourceBuild, locked_melpa_install_plan,
    locked_melpa_source, locked_melpa_sources, preflight_locked_melpa_packages,
    prepare_cached_locked_melpa_package, prepare_cached_locked_package_plan,
};

use crate::{CommandError, EmacsRuntime, output_with_timeout};

impl neomacs_infra::packages::PackageInstallDriver for EmacsRuntime {
    fn name(&self) -> &str {
        &self.name
    }

    fn timeout(&self) -> std::time::Duration {
        self.timeout
    }

    fn command(&self) -> std::process::Command {
        EmacsRuntime::command(self)
    }

    fn run(
        &self,
        command: &mut std::process::Command,
    ) -> Result<std::process::Output, neomacs_infra::packages::InstallCommandError> {
        output_with_timeout(command, self.timeout).map_err(|error| match error {
            CommandError::Launch(launch) => {
                neomacs_infra::packages::InstallCommandError::Launch(launch)
            }
            CommandError::TimedOut(output) => {
                neomacs_infra::packages::InstallCommandError::TimedOut(output)
            }
            CommandError::Capture(capture) => {
                neomacs_infra::packages::InstallCommandError::Capture(capture)
            }
        })
    }
}
