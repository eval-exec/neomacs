use std::time::Duration;

use crate::{CachedMelpaOracle, ZERO_X_C_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod commands;
mod conversion;
mod inference;
mod live;

const ZERO_X_C_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn zero_x_c_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZERO_X_C_MELPA_PIN, "0xc.el")
        .expect("prepare pinned 0xc source below ./tmp")
        .with_timeout(ZERO_X_C_TEST_TIMEOUT)
}

fn zero_x_c_live_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZERO_X_C_MELPA_PIN, "0xc-live.el")
        .expect("prepare pinned 0xc-live source below ./tmp")
        .with_timeout(ZERO_X_C_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed 0xc parity test").into()
}

/// Multi-probe batch for `assert_zero_x_c_parity` cases (2a).
pub(crate) fn assert_zero_x_c_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(zero_x_c_oracle(), &name, "zero_x_c_parity", cases);
}

/// Multi-probe batch for `assert_zero_x_c_live_parity` cases (2a).
pub(crate) fn assert_zero_x_c_live_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(zero_x_c_live_oracle(), &name, "zero_x_c_live_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn zero_x_c_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        commands::commands_public_surface_batch_cases(),
        conversion::conversion_public_surface_batch_cases(),
        inference::inference_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_zero_x_c_batch(&cases);
}

#[test]
fn zero_x_c_live_package_batch() {
    let cases: Vec<ParityBatchCase> = [live::live_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_zero_x_c_live_batch(&cases);
}

// END generated package batch tests
