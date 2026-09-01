use std::time::Duration;

use crate::{CachedMelpaOracle, SLIM_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SLIM_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SLIM_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'slim-mode)

(defun neomacs-slim-mode-test-with-buffer (body)
  "Call BODY in a temporary Slim buffer with nested structure."
  (with-temp-buffer
    (insert
     "doctype html\n"
     "html\n"
     "  head\n"
     "    title Hello\n"
     "  body\n"
     "    #main\n"
     "      p Welcome\n"
     "      ul\n"
     "        li One\n"
     "        li Two\n")
    (slim-mode)
    (goto-char (point-min))
    (funcall body)))
"####;

fn slim_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SLIM_MODE_MELPA_PIN, "slim-mode.el")
        .expect("prepare exact shallow slim-mode source below ./tmp")
        .with_prelude(SLIM_MODE_TEST_PRELUDE)
        .with_timeout(SLIM_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed slim-mode parity test")
        .into()
}

fn assert_slim_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        slim_mode_oracle(),
        &current_test_name(),
        "slim_mode_parity",
        cases,
    );
}

#[test]
fn slim_mode_package_batch() {
    assert_slim_mode_batch(&workflows::workflow_batch_cases());
}
