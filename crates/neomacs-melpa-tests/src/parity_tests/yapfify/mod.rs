use std::time::Duration;

use crate::{CachedMelpaOracle, YAPFIFY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const YAPFIFY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const YAPFIFY_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'yapfify)
"####;

fn yapfify_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YAPFIFY_MELPA_PIN, "yapfify.el")
        .expect("prepare exact shallow yapfify source below ./tmp")
        .with_prelude(YAPFIFY_TEST_PRELUDE)
        .with_timeout(YAPFIFY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed yapfify parity test")
        .into()
}

fn assert_yapfify_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        yapfify_oracle(),
        &current_test_name(),
        "yapfify_parity",
        cases,
    );
}

#[test]
fn yapfify_package_batch() {
    assert_yapfify_batch(&workflows::workflow_batch_cases());
}
