use std::time::Duration;

use crate::{CachedMelpaOracle, FUZZY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const FUZZY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const FUZZY_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'fuzzy)
"####;

fn fuzzy_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FUZZY_MELPA_PIN, "fuzzy.el")
        .expect("prepare exact shallow fuzzy source below ./tmp")
        .with_prelude(FUZZY_TEST_PRELUDE)
        .with_timeout(FUZZY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed fuzzy parity test")
        .into()
}

fn assert_fuzzy_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(fuzzy_oracle(), &current_test_name(), "fuzzy_parity", cases);
}

#[test]
fn fuzzy_package_batch() {
    assert_fuzzy_batch(&workflows::workflow_batch_cases());
}
