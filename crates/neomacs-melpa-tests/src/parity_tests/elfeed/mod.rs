use std::time::Duration;

use crate::{CachedMelpaOracle, ELFEED_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ELFEED_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ELFEED_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
"####;

fn elfeed_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ELFEED_MELPA_PIN, "elfeed.el")
        .expect("prepare exact shallow elfeed source below ./tmp")
        .with_prelude(ELFEED_TEST_PRELUDE)
        .with_timeout(ELFEED_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed elfeed parity test")
        .into()
}

fn assert_elfeed_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        elfeed_oracle(),
        &current_test_name(),
        "elfeed_parity",
        cases,
    )
}

#[test]
fn elfeed_package_batch() {
    assert_elfeed_batch(&workflows::workflow_batch_cases());
}
