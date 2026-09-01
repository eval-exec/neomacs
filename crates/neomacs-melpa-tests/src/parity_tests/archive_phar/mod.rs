use std::time::Duration;

use crate::{ARCHIVE_PHAR_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARCHIVE_PHAR_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn archive_phar_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARCHIVE_PHAR_MELPA_PIN, "archive-phar.el")
        .expect("prepare pinned archive-phar source below ./tmp")
        .with_timeout(ARCHIVE_PHAR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed archive-phar parity test")
        .into()
}

/// Multi-probe batch for `assert_archive_phar_parity` cases (2a).
pub(crate) fn assert_archive_phar_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(archive_phar_oracle(), &name, "archive_phar_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn archive_phar_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_archive_phar_batch(&cases);
}

// END generated package batch tests
