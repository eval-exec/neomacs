use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod core;
mod editing;
mod ex_search;
mod keymaps;
mod registers;
mod repeat_commands;
mod types;
mod utilities;

const EVIL_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn evil_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_MELPA_PIN, "evil.el")
        .expect("prepare pinned Evil source and dependencies below ./tmp")
        .with_timeout(EVIL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Evil parity test").into()
}

/// Multi-probe batch for `assert_evil_parity` cases (2a).
pub(crate) fn assert_evil_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(evil_oracle(), &name, "evil_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn evil_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        core::core_public_surface_batch_cases(),
        editing::editing_public_surface_batch_cases(),
        ex_search::ex_search_public_surface_batch_cases(),
        keymaps::keymaps_public_surface_batch_cases(),
        registers::registers_public_surface_batch_cases(),
        repeat_commands::repeat_commands_public_surface_batch_cases(),
        types::types_public_surface_batch_cases(),
        utilities::utilities_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_evil_batch(&cases);
}

// END generated package batch tests
