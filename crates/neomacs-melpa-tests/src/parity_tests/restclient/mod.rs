use std::time::Duration;

use crate::{CachedMelpaOracle, RESTCLIENT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const RESTCLIENT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const RESTCLIENT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
"####;

fn restclient_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RESTCLIENT_MELPA_PIN, "restclient.el")
        .expect("prepare exact shallow restclient source below ./tmp")
        .with_prelude(RESTCLIENT_TEST_PRELUDE)
        .with_timeout(RESTCLIENT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed restclient parity test")
        .into()
}

fn assert_restclient_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        restclient_oracle(),
        &current_test_name(),
        "restclient_parity",
        cases,
    )
}

#[test]
fn restclient_package_batch() {
    assert_restclient_batch(&workflows::workflow_batch_cases());
}
