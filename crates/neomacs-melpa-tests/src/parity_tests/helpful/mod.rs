use std::time::Duration;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, ELISP_REFS_MELPA_PIN, F_MELPA_PIN, HELPFUL_MELPA_PIN,
    S_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELPFUL_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const HELPFUL_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'helpful)
"####;

fn helpful_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELPFUL_MELPA_PIN, "helpful.el")
        .expect("prepare exact shallow helpful source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare dash")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare s")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare f")
        .with_melpa_dependency(ELISP_REFS_MELPA_PIN)
        .expect("prepare elisp-refs")
        .with_prelude(HELPFUL_TEST_PRELUDE)
        .with_timeout(HELPFUL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed helpful parity test")
        .into()
}

fn assert_helpful_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helpful_oracle(),
        &current_test_name(),
        "helpful_parity",
        cases,
    );
}

#[test]
fn helpful_package_batch() {
    assert_helpful_batch(&workflows::workflow_batch_cases());
}
