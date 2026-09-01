use std::time::Duration;

use crate::{CachedMelpaOracle, ERT_RUNNER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ERT_RUNNER_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const ERT_RUNNER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ert)
(require 'f)
(require 's)
;; Prevent commander from parsing emacs batch args and running the default
;; ert-runner CLI when the package is loaded under the parity harness.
(setq commander-ignore t)
;; ert-runner.el is a script-style package: it never calls (provide 'ert-runner).
;; Load it by path once the package directory is on load-path.
(load "ert-runner" nil t)

(defun neomacs-ert-runner-test-with-temp-root (body)
  "Call BODY with a temporary project root on `default-directory'."
  (let* ((root (make-temp-file "neomacs-ert-runner-" t))
         (default-directory root))
    (unwind-protect
        (funcall body root)
      (ignore-errors
        (delete-directory root t)))))
"####;

fn ert_runner_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ERT_RUNNER_MELPA_PIN, "ert-runner.el")
        .expect("prepare exact shallow ert-runner source below ./tmp")
        .with_prelude(ERT_RUNNER_TEST_PRELUDE)
        .with_timeout(ERT_RUNNER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed ert-runner parity test")
        .into()
}

fn assert_ert_runner_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        ert_runner_oracle(),
        &current_test_name(),
        "ert_runner_parity",
        cases,
    );
}

#[test]
fn ert_runner_package_batch() {
    assert_ert_runner_batch(&workflows::workflow_batch_cases());
}
