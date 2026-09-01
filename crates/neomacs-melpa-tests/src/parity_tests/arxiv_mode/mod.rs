use std::time::Duration;

use crate::{ARXIV_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARXIV_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn arxiv_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARXIV_MODE_MELPA_PIN, "arxiv-mode.el")
        .expect("prepare pinned arxiv-mode source below ./tmp")
        .with_timeout(ARXIV_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed arxiv-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_arxiv_mode_parity` cases (2a).
pub(crate) fn assert_arxiv_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(arxiv_mode_oracle(), &name, "arxiv_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn arxiv_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arxiv_mode_batch(&cases);
}

// END generated package batch tests
