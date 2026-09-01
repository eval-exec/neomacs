use std::time::Duration;

use crate::{CachedMelpaOracle, FLYSPELL_CORRECT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const FLYSPELL_CORRECT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const FLYSPELL_CORRECT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'flyspell)
(require 'flyspell-correct)
"####;

fn flyspell_correct_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYSPELL_CORRECT_MELPA_PIN, "flyspell-correct.el")
        .expect("prepare exact shallow flyspell-correct source below ./tmp")
        .with_prelude(FLYSPELL_CORRECT_TEST_PRELUDE)
        .with_timeout(FLYSPELL_CORRECT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed flyspell-correct parity test")
        .into()
}

fn assert_flyspell_correct_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        flyspell_correct_oracle(),
        &current_test_name(),
        "flyspell_correct_parity",
        cases,
    );
}

#[test]
fn flyspell_correct_package_batch() {
    assert_flyspell_correct_batch(&workflows::workflow_batch_cases());
}
