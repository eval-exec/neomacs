use std::time::Duration;

use crate::{CachedMelpaOracle, INFLECTIONS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const INFLECTIONS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const INFLECTIONS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'inflections)
"####;

fn inflections_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(INFLECTIONS_MELPA_PIN, "inflections.el")
        .expect("prepare exact shallow inflections source below ./tmp")
        .with_prelude(INFLECTIONS_TEST_PRELUDE)
        .with_timeout(INFLECTIONS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed inflections parity test")
        .into()
}

fn assert_inflections_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        inflections_oracle(),
        &current_test_name(),
        "inflections_parity",
        cases,
    );
}

#[test]
fn inflections_package_batch() {
    assert_inflections_batch(&workflows::workflow_batch_cases());
}
