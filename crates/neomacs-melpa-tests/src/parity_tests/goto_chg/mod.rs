use std::time::Duration;

use crate::{CachedMelpaOracle, GOTO_CHG_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod navigation;
mod undo_entries;

const GOTO_CHG_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn goto_chg_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GOTO_CHG_MELPA_PIN, "goto-chg.el")
        .expect("prepare pinned goto-chg source below ./tmp")
        .with_timeout(GOTO_CHG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed goto-chg parity test")
        .into()
}

/// Multi-probe batch for `assert_goto_chg_parity` cases (2a).
pub(crate) fn assert_goto_chg_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(goto_chg_oracle(), &name, "goto_chg_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn goto_chg_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        navigation::navigation_public_surface_batch_cases(),
        undo_entries::undo_entries_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_goto_chg_batch(&cases);
}

// END generated package batch tests
