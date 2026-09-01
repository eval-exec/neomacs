use std::time::Duration;

use crate::{AT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod core;
mod mixins;
mod reflection;

const AT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn at_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AT_MELPA_PIN, "@-mixins.el")
        .expect("prepare pinned @ and @-mixins sources below ./tmp")
        .with_timeout(AT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed @ parity test").into()
}

/// Multi-probe batch for `assert_at_parity` cases (2a).
pub(crate) fn assert_at_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(at_oracle(), &name, "at_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn at_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        core::core_public_surface_batch_cases(),
        mixins::mixins_public_surface_batch_cases(),
        reflection::reflection_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_at_batch(&cases);
}

// END generated package batch tests
