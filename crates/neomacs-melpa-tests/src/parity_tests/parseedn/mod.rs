use std::time::Duration;

use crate::{CachedMelpaOracle, PARSECLJ_MELPA_PIN, PARSEEDN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PARSEEDN_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PARSEEDN_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'parseedn)
"####;

fn parseedn_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PARSEEDN_MELPA_PIN, "parseedn.el")
        .expect("prepare exact shallow parseedn source below ./tmp")
        .with_melpa_dependency(PARSECLJ_MELPA_PIN)
        .expect("prepare exact shallow parseclj dependency below ./tmp")
        .with_prelude(PARSEEDN_TEST_PRELUDE)
        .with_timeout(PARSEEDN_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed parseedn parity test")
        .into()
}

fn assert_parseedn_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        parseedn_oracle(),
        &current_test_name(),
        "parseedn_parity",
        cases,
    );
}

#[test]
fn parseedn_package_batch() {
    assert_parseedn_batch(&workflows::workflow_batch_cases());
}
