use std::time::Duration;

use crate::{CachedMelpaOracle, EMBARK_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EMBARK_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const EMBARK_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
"####;

fn embark_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EMBARK_MELPA_PIN, "embark.el")
        .expect("prepare exact shallow embark source below ./tmp")
        .with_prelude(EMBARK_TEST_PRELUDE)
        .with_timeout(EMBARK_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed embark parity test")
        .into()
}

fn assert_embark_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        embark_oracle(),
        &current_test_name(),
        "embark_parity",
        cases,
    )
}

#[test]
fn embark_package_batch() {
    assert_embark_batch(&workflows::workflow_batch_cases());
}
