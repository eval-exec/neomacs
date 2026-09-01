use std::time::Duration;

use crate::{CachedMelpaOracle, PARSEBIB_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PARSEBIB_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PARSEBIB_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
"####;

fn parsebib_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PARSEBIB_MELPA_PIN, "parsebib.el")
        .expect("prepare exact shallow parsebib source below ./tmp")
        .with_prelude(PARSEBIB_TEST_PRELUDE)
        .with_timeout(PARSEBIB_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed parsebib parity test")
        .into()
}

fn assert_parsebib_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        parsebib_oracle(),
        &current_test_name(),
        "parsebib_parity",
        cases,
    )
}

#[test]
fn parsebib_package_batch() {
    assert_parsebib_batch(&workflows::workflow_batch_cases());
}
