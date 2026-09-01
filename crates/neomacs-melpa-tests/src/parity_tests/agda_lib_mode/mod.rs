use std::time::Duration;

use crate::{AGDA_LIB_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AGDA_LIB_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn agda_lib_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGDA_LIB_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned agda-lib-mode source below ./tmp")
        .with_timeout(AGDA_LIB_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed agda-lib-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_agda_lib_mode_autoload_parity` cases (2a).
pub(crate) fn assert_agda_lib_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        agda_lib_mode_oracle("agda-lib-mode-autoloads.el"),
        &name,
        "agda_lib_mode_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_agda_lib_mode_parity` cases (2a).
pub(crate) fn assert_agda_lib_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        agda_lib_mode_oracle("agda-lib-mode.el"),
        &name,
        "agda_lib_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn agda_lib_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_agda_lib_mode_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_agda_lib_mode_autoload_batch(&cases);
}

#[test]
fn agda_lib_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_agda_lib_mode_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_agda_lib_mode_batch(&cases);
}

// END generated package batch tests
