use std::time::Duration;

use crate::{CTABLE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const CTABLE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CTABLE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ctable)

(defun neomacs-ctable-test-buffer-text (component)
  "Return COMPONENT's rendered buffer text without properties."
  (with-current-buffer (ctbl:cp-get-buffer component)
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-ctable-test-kill (component)
  "Kill COMPONENT's destination buffer when it is still live."
  (when component
    (let ((buffer (ctbl:cp-get-buffer component)))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-ctable-test-fire-header (component column)
  "Invoke COLUMN's public header action for COMPONENT."
  (ctbl:fire-column-header-action component column))
"##;

fn ctable_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CTABLE_MELPA_PIN, "ctable.el")
        .expect("prepare exact shallow ctable source below ./tmp")
        .with_prelude(CTABLE_TEST_PRELUDE)
        .with_timeout(CTABLE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed ctable parity test")
        .into()
}

fn assert_ctable_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        ctable_oracle(),
        &current_test_name(),
        "ctable_parity",
        cases,
    );
}

#[test]
fn ctable_package_batch() {
    assert_ctable_batch(&workflows::workflow_batch_cases());
}
