use std::time::Duration;

use crate::{ANSI_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANSI_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn ansi_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANSI_MELPA_PIN, "ansi.el")
        .expect("prepare pinned ansi source below ./tmp")
        .with_timeout(ANSI_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ansi parity test").into()
}

/// Multi-probe batch for `assert_ansi_parity` cases (2a).
pub(crate) fn assert_ansi_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ansi_oracle(), &name, "ansi_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ansi_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ansi_batch(&cases);
}

// END generated package batch tests
