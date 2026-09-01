use std::time::Duration;

use crate::{CachedMelpaOracle, GROOVY_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GROOVY_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const GROOVY_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
"####;

fn groovy_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GROOVY_MODE_MELPA_PIN, "groovy-mode.el")
        .expect("prepare exact shallow groovy-mode source below ./tmp")
        .with_prelude(GROOVY_MODE_TEST_PRELUDE)
        .with_timeout(GROOVY_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed groovy-mode parity test")
        .into()
}

fn assert_groovy_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        groovy_mode_oracle(),
        &current_test_name(),
        "groovy_mode_parity",
        cases,
    )
}

#[test]
fn groovy_mode_package_batch() {
    assert_groovy_mode_batch(&workflows::workflow_batch_cases());
}
