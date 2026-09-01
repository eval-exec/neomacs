use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, ELISP_REFS_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ELISP_REFS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ELISP_REFS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'elisp-refs)
"####;

fn elisp_refs_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ELISP_REFS_MELPA_PIN, "elisp-refs.el")
        .expect("prepare exact shallow elisp-refs source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare dash")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare s")
        .with_prelude(ELISP_REFS_TEST_PRELUDE)
        .with_timeout(ELISP_REFS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed elisp-refs parity test")
        .into()
}

fn assert_elisp_refs_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        elisp_refs_oracle(),
        &current_test_name(),
        "elisp_refs_parity",
        cases,
    );
}

#[test]
fn elisp_refs_package_batch() {
    assert_elisp_refs_batch(&workflows::workflow_batch_cases());
}
