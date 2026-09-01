use std::time::Duration;

use crate::{AGE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AGE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn age_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGE_MELPA_PIN, "age.el")
        .expect("prepare pinned age source below ./tmp")
        .with_timeout(AGE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed age parity test").into()
}

/// Multi-probe batch for `assert_age_parity` cases (2a).
pub(crate) fn assert_age_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(age_oracle(), &name, "age_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn age_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_age_batch(&cases);
}

// END generated package batch tests
