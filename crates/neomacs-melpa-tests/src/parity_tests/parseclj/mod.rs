use std::time::Duration;

use crate::{CachedMelpaOracle, PARSECLJ_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PARSECLJ_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PARSECLJ_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'parseclj)
(require 'parseclj-lex)
"####;

fn parseclj_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PARSECLJ_MELPA_PIN, "parseclj.el")
        .expect("prepare exact shallow parseclj source below ./tmp")
        .with_prelude(PARSECLJ_TEST_PRELUDE)
        .with_timeout(PARSECLJ_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed parseclj parity test")
        .into()
}

fn assert_parseclj_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        parseclj_oracle(),
        &current_test_name(),
        "parseclj_parity",
        cases,
    );
}

#[test]
fn parseclj_package_batch() {
    assert_parseclj_batch(&workflows::workflow_batch_cases());
}
