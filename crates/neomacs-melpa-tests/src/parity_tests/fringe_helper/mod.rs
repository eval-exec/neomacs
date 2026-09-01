use std::time::Duration;

use crate::{CachedMelpaOracle, FRINGE_HELPER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const FRINGE_HELPER_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const FRINGE_HELPER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'fringe-helper)

(defun neomacs-fringe-helper-test-overlay-shape (ov)
  "Describe stable public overlay state for OV."
  (when (overlayp ov)
    (list :start (overlay-start ov)
          :end (overlay-end ov)
          :helper (and (overlay-get ov 'fringe-helper) t)
          :parent (and (overlay-get ov 'fringe-helper-parent) t)
          :display
          (let ((before (overlay-get ov 'before-string)))
            (and before (get-text-property 0 'display before))))))
"####;

fn fringe_helper_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FRINGE_HELPER_MELPA_PIN, "fringe-helper.el")
        .expect("prepare exact shallow fringe-helper source below ./tmp")
        .with_prelude(FRINGE_HELPER_TEST_PRELUDE)
        .with_timeout(FRINGE_HELPER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed fringe-helper parity test")
        .into()
}

fn assert_fringe_helper_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        fringe_helper_oracle(),
        &current_test_name(),
        "fringe_helper_parity",
        cases,
    );
}

#[test]
fn fringe_helper_package_batch() {
    assert_fringe_helper_batch(&workflows::workflow_batch_cases());
}
