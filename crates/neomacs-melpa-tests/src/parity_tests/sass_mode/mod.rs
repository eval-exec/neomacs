use std::time::Duration;

use crate::{CachedMelpaOracle, HAML_MODE_MELPA_PIN, SASS_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SASS_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SASS_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'sass-mode)

(defun neomacs-sass-mode-test-face-runs ()
  "Describe non-nil face runs in the accessible buffer."
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list (buffer-substring-no-properties position next) face)
                runs))
        (setq position next)))
    (nreverse runs)))
"##;

fn sass_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SASS_MODE_MELPA_PIN, "sass-mode.el")
        .expect("prepare exact shallow Sass Mode source below ./tmp")
        .with_melpa_dependency(HAML_MODE_MELPA_PIN)
        .expect("prepare exact shallow Haml Mode dependency below ./tmp")
        .with_prelude(SASS_MODE_TEST_PRELUDE)
        .with_timeout(SASS_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Sass Mode parity test")
        .into()
}

fn assert_sass_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        sass_mode_oracle(),
        &current_test_name(),
        "sass_mode_parity",
        cases,
    );
}

#[test]
fn sass_mode_package_batch() {
    assert_sass_mode_batch(&workflows::workflow_batch_cases());
}
