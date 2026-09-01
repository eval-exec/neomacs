use std::time::Duration;

use crate::{CachedMelpaOracle, GENERAL_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod bindings;
mod configuration;
mod definers;
mod dispatch;
mod integrations;

const GENERAL_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn general_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GENERAL_MELPA_PIN, "general.el")
        .expect("prepare pinned General source and dependencies below ./tmp")
        .with_timeout(GENERAL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed General parity test")
        .into()
}

/// Multi-probe batch for `assert_general_parity` cases (2a).
pub(crate) fn assert_general_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(general_oracle(), &name, "general_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn general_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        bindings::bindings_public_surface_batch_cases(),
        configuration::configuration_public_surface_batch_cases(),
        definers::definers_public_surface_batch_cases(),
        dispatch::dispatch_public_surface_batch_cases(),
        integrations::integrations_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_general_batch(&cases);
}

// END generated package batch tests
