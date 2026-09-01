use std::time::Duration;

use crate::{CachedMelpaOracle, YASNIPPET_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YASNIPPET_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn yasnippet_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YASNIPPET_MELPA_PIN, "yasnippet.el")
        .expect("prepare pinned Yasnippet source below ./tmp")
        .with_timeout(YASNIPPET_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Yasnippet parity test")
        .into()
}

pub(crate) fn assert_yasnippet_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(yasnippet_oracle(), &name, "yasnippet_parity", cases);
}

#[test]
fn yasnippet_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_yasnippet_batch(&cases);
}
