use std::time::Duration;

use crate::{CachedMelpaOracle, ESHELL_PROMPT_EXTRAS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ESHELL_PROMPT_EXTRAS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ESHELL_PROMPT_EXTRAS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'eshell-prompt-extras)
"####;

fn eshell_prompt_extras_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ESHELL_PROMPT_EXTRAS_MELPA_PIN, "eshell-prompt-extras.el")
        .expect("prepare exact shallow eshell-prompt-extras source below ./tmp")
        .with_prelude(ESHELL_PROMPT_EXTRAS_TEST_PRELUDE)
        .with_timeout(ESHELL_PROMPT_EXTRAS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed eshell-prompt-extras parity test")
        .into()
}

fn assert_eshell_prompt_extras_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        eshell_prompt_extras_oracle(),
        &current_test_name(),
        "eshell_prompt_extras_parity",
        cases,
    );
}

#[test]
fn eshell_prompt_extras_package_batch() {
    assert_eshell_prompt_extras_batch(&workflows::workflow_batch_cases());
}
