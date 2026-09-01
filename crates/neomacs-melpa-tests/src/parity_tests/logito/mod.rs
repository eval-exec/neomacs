use std::time::Duration;

use crate::{CachedMelpaOracle, LOGITO_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const LOGITO_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const LOGITO_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'eieio)
(require 'logito)
"####;

fn logito_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LOGITO_MELPA_PIN, "logito.el")
        .expect("prepare exact shallow logito source below ./tmp")
        .with_prelude(LOGITO_TEST_PRELUDE)
        .with_timeout(LOGITO_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed logito parity test")
        .into()
}

fn assert_logito_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        logito_oracle(),
        &current_test_name(),
        "logito_parity",
        cases,
    );
}

#[test]
fn logito_package_batch() {
    assert_logito_batch(&workflows::workflow_batch_cases());
}
