use std::time::Duration;

use crate::{ARIADNE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARIADNE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn ariadne_oracle(prelude: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARIADNE_MELPA_PIN, "ariadne.el")
        .expect("prepare pinned Ariadne source below ./tmp")
        .with_prelude(prelude)
        .with_timeout(ARIADNE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Ariadne parity test")
        .into()
}

/// Multi-probe batch for `assert_ariadne_parity` cases (2a).
pub(crate) fn assert_ariadne_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ariadne_oracle(""), &name, "ariadne_parity", cases);
}

/// Multi-probe batch for `assert_ariadne_with_legacy_cl_parity` cases (2a).
pub(crate) fn assert_ariadne_with_legacy_cl_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ariadne_oracle("(require 'cl)"),
        &name,
        "ariadne_with_legacy_cl_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ariadne_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_ariadne_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ariadne_batch(&cases);
}

#[test]
fn ariadne_with_legacy_cl_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_ariadne_with_legacy_cl_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ariadne_with_legacy_cl_batch(&cases);
}

// END generated package batch tests
