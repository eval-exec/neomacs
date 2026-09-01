use std::time::Duration;

use crate::{APPLES_MODE_MELPA_PIN, CachedMelpaOracle, YASNIPPET_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APPLES_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn apples_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(APPLES_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned apples-mode source below ./tmp")
        .with_melpa_dependency(YASNIPPET_MELPA_PIN)
        .expect("prepare pinned Yasnippet dependency below ./tmp")
        .with_timeout(APPLES_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed apples-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_apples_mode_parity` cases (2a).
pub(crate) fn assert_apples_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        apples_mode_oracle("apples-mode.el"),
        &name,
        "apples_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn apples_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apples_mode_batch(&cases);
}

// END generated package batch tests
