use std::time::Duration;

use crate::{CachedMelpaOracle, WHICH_KEY_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod defaults;
mod keymaps;
mod layout;
mod replacements;
mod sorting;

const WHICH_KEY_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn which_key_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WHICH_KEY_MELPA_PIN, "which-key.el")
        .expect("prepare pinned Which-Key source and dependencies below ./tmp")
        .with_timeout(WHICH_KEY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Which-Key parity test")
        .into()
}

/// Multi-probe batch for `assert_which_key_parity` cases (2a).
pub(crate) fn assert_which_key_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(which_key_oracle(), &name, "which_key_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn which_key_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        defaults::defaults_public_surface_batch_cases(),
        keymaps::keymaps_public_surface_batch_cases(),
        layout::layout_public_surface_batch_cases(),
        replacements::replacements_public_surface_batch_cases(),
        sorting::sorting_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_which_key_batch(&cases);
}

// END generated package batch tests
