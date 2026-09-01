use std::time::Duration;

use crate::{CachedMelpaOracle, JULIA_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const JULIA_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const JULIA_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
"####;

fn julia_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JULIA_MODE_MELPA_PIN, "julia-mode.el")
        .expect("prepare exact shallow julia-mode source below ./tmp")
        .with_prelude(JULIA_MODE_TEST_PRELUDE)
        .with_timeout(JULIA_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed julia-mode parity test")
        .into()
}

fn assert_julia_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        julia_mode_oracle(),
        &current_test_name(),
        "julia_mode_parity",
        cases,
    )
}

#[test]
fn julia_mode_package_batch() {
    assert_julia_mode_batch(&workflows::workflow_batch_cases());
}
