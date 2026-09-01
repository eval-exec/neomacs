use std::time::Duration;

use crate::{ANZU_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod practical;

const ANZU_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn anzu_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANZU_MELPA_PIN, "anzu.el")
        .expect("prepare pinned anzu source below ./tmp")
        .with_timeout(ANZU_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed anzu parity test").into()
}

/// Multi-probe batch for `assert_anzu_parity` cases (2a).
pub(crate) fn assert_anzu_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(anzu_oracle(), &name, "anzu_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn anzu_package_batch() {
    let cases: Vec<ParityBatchCase> = [practical::practical_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anzu_batch(&cases);
}

// END generated package batch tests
