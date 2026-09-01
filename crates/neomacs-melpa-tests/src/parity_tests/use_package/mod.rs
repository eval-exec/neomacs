use std::time::Duration;

use crate::{CachedPackageOracle, USE_PACKAGE_GNU_ELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod activation;
mod core;
mod integrations;

const USE_PACKAGE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn use_package_oracle() -> CachedPackageOracle {
    CachedPackageOracle::new_from_gnu_elpa(USE_PACKAGE_GNU_ELPA_PIN, "use-package.el")
        .expect("prepare pinned Use-Package source and dependencies below ./tmp")
        .with_timeout(USE_PACKAGE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Use-Package parity test")
        .into()
}

/// Multi-probe batch for `assert_use_package_parity` cases (2a).
pub(crate) fn assert_use_package_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(use_package_oracle(), &name, "use_package_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn use_package_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        activation::activation_public_surface_batch_cases(),
        core::core_public_surface_batch_cases(),
        integrations::integrations_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_use_package_batch(&cases);
}

// END generated package batch tests
