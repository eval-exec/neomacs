use std::time::Duration;

use crate::{AIDER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod core;
mod editing;
mod files;
mod registry;
mod workflows;

const AIDER_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn aider_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AIDER_MELPA_PIN, source_file)
        .expect("prepare pinned aider source below ./tmp")
        .with_timeout(AIDER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aider parity test").into()
}

/// Multi-probe batch for `assert_aider_autoload_parity` cases (2a).
pub(crate) fn assert_aider_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        aider_oracle("aider-autoloads.el"),
        &name,
        "aider_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_aider_parity` cases (2a).
pub(crate) fn assert_aider_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aider_oracle("aider.el"), &name, "aider_parity", cases);
}

/// Multi-probe batch for `assert_aider_helm_parity` cases (2a).
pub(crate) fn assert_aider_helm_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        aider_oracle("aider-helm.el"),
        &name,
        "aider_helm_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn aider_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_aider_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aider_autoload_batch(&cases);
}

#[test]
fn aider_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        core::core_public_surface_batch_cases(),
        editing::editing_public_surface_batch_cases(),
        files::files_public_surface_batch_cases(),
        registry::registry_aider_batch_cases(),
        workflows::workflows_aider_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_aider_batch(&cases);
}

#[test]
fn aider_helm_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_aider_helm_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aider_helm_batch(&cases);
}

// END generated package batch tests
