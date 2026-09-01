use std::time::Duration;

use crate::{CachedMelpaOracle, ZERO_B_LAYOUT_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod keybindings;
mod layouts;
mod state;

const ZERO_B_LAYOUT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn zero_b_layout_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZERO_B_LAYOUT_MELPA_PIN, "0blayout.el")
        .expect("prepare pinned 0blayout source below ./tmp")
        .with_timeout(ZERO_B_LAYOUT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed 0blayout parity test")
        .into()
}

/// Multi-probe batch for `assert_zero_b_layout_parity` cases (2a).
pub(crate) fn assert_zero_b_layout_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(zero_b_layout_oracle(), &name, "zero_b_layout_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn zero_b_layout_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        keybindings::keybindings_public_surface_batch_cases(),
        layouts::layouts_public_surface_batch_cases(),
        state::state_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_zero_b_layout_batch(&cases);
}

// END generated package batch tests
