use std::time::Duration;

use crate::{CachedMelpaOracle, PUG_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PUG_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PUG_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'pug-mode)
"####;

fn pug_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PUG_MODE_MELPA_PIN, "pug-mode.el")
        .expect("prepare exact shallow pug-mode source below ./tmp")
        .with_prelude(PUG_MODE_TEST_PRELUDE)
        .with_timeout(PUG_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed pug-mode parity test")
        .into()
}

fn assert_pug_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        pug_mode_oracle(),
        &current_test_name(),
        "pug_mode_parity",
        cases,
    );
}

#[test]
fn pug_mode_package_batch() {
    assert_pug_mode_batch(&workflows::workflow_batch_cases());
}
