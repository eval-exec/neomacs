use std::time::Duration;

use crate::{CachedMelpaOracle, TRANSIENT_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod layout;
mod state;

const TRANSIENT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn transient_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TRANSIENT_MELPA_PIN, "transient.el")
        .expect("prepare pinned Transient source and dependencies below ./tmp")
        .with_prelude("(setq transient-error-on-insert-failure t)")
        .with_timeout(TRANSIENT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Transient parity test")
        .into()
}

/// Multi-probe batch for `assert_transient_parity` cases (2a).
pub(crate) fn assert_transient_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(transient_oracle(), &name, "transient_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn transient_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        layout::layout_public_surface_batch_cases(),
        state::state_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_transient_batch(&cases);
}

// END generated package batch tests
