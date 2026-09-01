use std::time::Duration;

use crate::{CachedMelpaOracle, PIP_REQUIREMENTS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PIP_REQUIREMENTS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PIP_REQUIREMENTS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'pip-requirements)

(defun neomacs-pip-requirements-test-fontify (text)
  "Insert TEXT, enter Pip Requirements Mode, and fully fontify it."
  (insert text)
  (let ((pip-packages '("requests" "rich" "ruff" "urllib3")))
    (pip-requirements-mode))
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-pip-requirements-test-token (text)
  "Describe every occurrence of TEXT in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (let (result)
      (while (search-forward text nil t)
        (let ((position (- (point) (length text))))
          (push
           (list :line (line-number-at-pos position)
                 :face (get-text-property position 'face))
           result)))
      (nreverse result))))
"##;

fn pip_requirements_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PIP_REQUIREMENTS_MELPA_PIN, "pip-requirements.el")
        .expect("prepare exact shallow Pip Requirements source and dependency below ./tmp")
        .with_prelude(PIP_REQUIREMENTS_TEST_PRELUDE)
        .with_timeout(PIP_REQUIREMENTS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Pip Requirements parity test")
        .into()
}

fn assert_pip_requirements_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        pip_requirements_oracle(),
        &current_test_name(),
        "pip_requirements_parity",
        cases,
    );
}

#[test]
fn pip_requirements_package_batch() {
    assert_pip_requirements_batch(&workflows::workflow_batch_cases());
}
