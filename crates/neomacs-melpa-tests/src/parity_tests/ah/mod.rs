use std::time::Duration;

use crate::{AH_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod cursor;
mod lifecycle;
mod quit;
mod theme;

const AH_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn ah_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AH_MELPA_PIN, source_file)
        .expect("prepare pinned ah source below ./tmp")
        .with_timeout(AH_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ah parity test").into()
}

/// Multi-probe batch for `assert_ah_autoload_parity` cases (2a).
pub(crate) fn assert_ah_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ah_oracle("ah-autoloads.el"),
        &name,
        "ah_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_ah_parity` cases (2a).
pub(crate) fn assert_ah_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ah_oracle("ah.el"), &name, "ah_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ah_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [lifecycle::lifecycle_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ah_autoload_batch(&cases);
}

#[test]
fn ah_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        cursor::cursor_public_surface_batch_cases(),
        quit::quit_public_surface_batch_cases(),
        theme::theme_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_ah_batch(&cases);
}

// END generated package batch tests
