use std::time::Duration;

use crate::{CachedMelpaOracle, TOML_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TOML_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const TOML_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'toml-mode)
"####;

fn toml_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TOML_MODE_MELPA_PIN, "toml-mode.el")
        .expect("prepare exact shallow toml-mode source below ./tmp")
        .with_prelude(TOML_MODE_TEST_PRELUDE)
        .with_timeout(TOML_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed toml-mode parity test")
        .into()
}

fn assert_toml_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        toml_mode_oracle(),
        &current_test_name(),
        "toml_mode_parity",
        cases,
    );
}

#[test]
fn toml_mode_package_batch() {
    assert_toml_mode_batch(&workflows::workflow_batch_cases());
}
