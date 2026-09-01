use std::time::Duration;

use crate::{CachedMelpaOracle, PRETTIER_JS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PRETTIER_JS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRETTIER_JS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'prettier-js)
"####;

fn prettier_js_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PRETTIER_JS_MELPA_PIN, "prettier-js.el")
        .expect("prepare exact shallow prettier-js source below ./tmp")
        .with_prelude(PRETTIER_JS_TEST_PRELUDE)
        .with_timeout(PRETTIER_JS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed prettier-js parity test")
        .into()
}

fn assert_prettier_js_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        prettier_js_oracle(),
        &current_test_name(),
        "prettier_js_parity",
        cases,
    );
}

#[test]
fn prettier_js_package_batch() {
    assert_prettier_js_batch(&workflows::workflow_batch_cases());
}
