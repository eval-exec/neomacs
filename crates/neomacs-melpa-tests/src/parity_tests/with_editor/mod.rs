use std::time::Duration;

use crate::{CachedMelpaOracle, WITH_EDITOR_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod environment;
mod lifecycle;
mod protocol;

const WITH_EDITOR_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn with_editor_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WITH_EDITOR_MELPA_PIN, "with-editor.el")
        .expect("prepare pinned With-Editor source and dependencies below ./tmp")
        .with_timeout(WITH_EDITOR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed With-Editor parity test")
        .into()
}

/// Multi-probe batch for `assert_with_editor_parity` cases (2a).
pub(crate) fn assert_with_editor_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(with_editor_oracle(), &name, "with_editor_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn with_editor_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        environment::environment_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        protocol::protocol_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_with_editor_batch(&cases);
}

// END generated package batch tests
