use std::time::Duration;

use crate::{CachedMelpaOracle, MONOKAI_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const MONOKAI_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const MONOKAI_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'monokai-theme)
"####;

fn monokai_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MONOKAI_THEME_MELPA_PIN, "monokai-theme.el")
        .expect("prepare exact shallow monokai-theme source below ./tmp")
        .with_prelude(MONOKAI_THEME_TEST_PRELUDE)
        .with_timeout(MONOKAI_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed monokai-theme parity test")
        .into()
}

fn assert_monokai_theme_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        monokai_theme_oracle(),
        &current_test_name(),
        "monokai_theme_parity",
        cases,
    );
}

#[test]
fn monokai_theme_package_batch() {
    assert_monokai_theme_batch(&workflows::workflow_batch_cases());
}
