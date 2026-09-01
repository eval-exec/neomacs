use std::time::Duration;

use crate::{CONCURRENT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'concurrent)
(setq deferred:tick-time 0.001)

(defun neomacs-concurrent-test-sync (deferred)
  "Synchronously finish DEFERRED after clearing stale queue state."
  (deferred:sync! deferred))

(defun neomacs-concurrent-test-error (function)
  "Return FUNCTION's value or exact error identity and message."
  (condition-case err
      (list :value (funcall function))
    (error (list :signal (car err) :message (error-message-string err)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CONCURRENT_MELPA_PIN, "concurrent.el")
        .expect("prepare exact shallow Concurrent source and Deferred dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn concurrent_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "concurrent_package_batch",
        "concurrent_parity",
        &workflows::workflow_batch_cases(),
    );
}
