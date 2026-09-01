use std::time::Duration;

use crate::{A_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const A_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn a_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(A_MELPA_PIN, "a.el")
        .expect("prepare pinned a source below ./tmp")
        .with_timeout(A_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed a parity test").into()
}

/// Multi-probe batch for `assert_a_parity` cases (2a).
pub(crate) fn assert_a_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(a_oracle(), &name, "a_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn a_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::a_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_a_batch(&cases);
}

// END generated package batch tests
