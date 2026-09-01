use std::time::Duration;

use crate::{CachedMelpaOracle, UNFILL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const UNFILL_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const UNFILL_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'unfill)
"####;

fn unfill_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(UNFILL_MELPA_PIN, "unfill.el")
        .expect("prepare exact shallow unfill source below ./tmp")
        .with_prelude(UNFILL_TEST_PRELUDE)
        .with_timeout(UNFILL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed unfill parity test")
        .into()
}

fn assert_unfill_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        unfill_oracle(),
        &current_test_name(),
        "unfill_parity",
        cases,
    );
}

#[test]
fn unfill_package_batch() {
    assert_unfill_batch(&workflows::workflow_batch_cases());
}
