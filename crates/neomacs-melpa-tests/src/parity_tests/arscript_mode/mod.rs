use std::time::Duration;

use crate::{ARSCRIPT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARSCRIPT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn arscript_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARSCRIPT_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned arscript-mode source below ./tmp")
        .with_timeout(ARSCRIPT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed arscript-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_arscript_mode_parity` cases (2a).
pub(crate) fn assert_arscript_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        arscript_mode_oracle("arscript-mode.el"),
        &name,
        "arscript_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn arscript_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arscript_mode_batch(&cases);
}

// END generated package batch tests
