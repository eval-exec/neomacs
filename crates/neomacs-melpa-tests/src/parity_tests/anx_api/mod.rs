use std::time::Duration;

use crate::{ANX_API_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod practical;

const ANX_API_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn anx_api_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANX_API_MELPA_PIN, "anx-api.el")
        .expect("prepare pinned anx-api source below ./tmp")
        .with_timeout(ANX_API_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anx-api parity test")
        .into()
}

/// Multi-probe batch for `assert_anx_api_parity` cases (2a).
pub(crate) fn assert_anx_api_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(anx_api_oracle(), &name, "anx_api_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn anx_api_package_batch() {
    let cases: Vec<ParityBatchCase> = [practical::practical_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anx_api_batch(&cases);
}

// END generated package batch tests
