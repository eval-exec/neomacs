use std::time::Duration;

use crate::{ARTBOLLOCKS_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARTBOLLOCKS_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn artbollocks_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARTBOLLOCKS_MODE_MELPA_PIN, "artbollocks-mode.el")
        .expect("prepare pinned Art Bollocks Mode source below ./tmp")
        .with_timeout(ARTBOLLOCKS_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Art Bollocks Mode parity test")
        .into()
}

/// Multi-probe batch for `assert_artbollocks_mode_parity` cases (2a).
pub(crate) fn assert_artbollocks_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        artbollocks_mode_oracle(),
        &name,
        "artbollocks_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn artbollocks_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_artbollocks_mode_batch(&cases);
}

// END generated package batch tests
