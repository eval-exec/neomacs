use std::time::Duration;

use crate::{CachedMelpaOracle, ZERO_X_ZERO_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod commands;
mod configuration;
mod transport;

const ZERO_X_ZERO_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn zero_x_zero_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZERO_X_ZERO_MELPA_PIN, "0x0.el")
        .expect("prepare pinned 0x0 source below ./tmp")
        .with_timeout(ZERO_X_ZERO_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed 0x0 parity test").into()
}

/// Multi-probe batch for `assert_zero_x_zero_parity` cases (2a).
pub(crate) fn assert_zero_x_zero_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(zero_x_zero_oracle(), &name, "zero_x_zero_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn zero_x_zero_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        commands::commands_public_surface_batch_cases(),
        configuration::configuration_public_surface_batch_cases(),
        transport::transport_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_zero_x_zero_batch(&cases);
}

// END generated package batch tests
