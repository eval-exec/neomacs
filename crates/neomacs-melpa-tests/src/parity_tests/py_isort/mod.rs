use std::time::Duration;

use crate::{CachedMelpaOracle, PY_ISORT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PY_ISORT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PY_ISORT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'py-isort)
"####;

fn py_isort_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PY_ISORT_MELPA_PIN, "py-isort.el")
        .expect("prepare exact shallow py-isort source below ./tmp")
        .with_prelude(PY_ISORT_TEST_PRELUDE)
        .with_timeout(PY_ISORT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed py-isort parity test")
        .into()
}

fn assert_py_isort_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        py_isort_oracle(),
        &current_test_name(),
        "py_isort_parity",
        cases,
    );
}

#[test]
fn py_isort_package_batch() {
    assert_py_isort_batch(&workflows::workflow_batch_cases());
}
