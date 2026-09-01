use std::time::Duration;

use crate::{CachedMelpaOracle, WEB_BEAUTIFY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const WEB_BEAUTIFY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const WEB_BEAUTIFY_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'web-beautify)
"####;

fn web_beautify_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WEB_BEAUTIFY_MELPA_PIN, "web-beautify.el")
        .expect("prepare exact shallow web-beautify source below ./tmp")
        .with_prelude(WEB_BEAUTIFY_TEST_PRELUDE)
        .with_timeout(WEB_BEAUTIFY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed web-beautify parity test")
        .into()
}

fn assert_web_beautify_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        web_beautify_oracle(),
        &current_test_name(),
        "web_beautify_parity",
        cases,
    );
}

#[test]
fn web_beautify_package_batch() {
    assert_web_beautify_batch(&workflows::workflow_batch_cases());
}
