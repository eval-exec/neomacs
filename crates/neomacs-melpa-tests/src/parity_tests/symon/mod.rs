use std::time::Duration;

use crate::{CachedMelpaOracle, SYMON_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SYMON_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SYMON_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'symon)

(defvar neomacs-symon-fetch-values nil)
(defvar neomacs-symon-calls nil)
(defvar neomacs-symon-life-events nil)

(defun neomacs-symon-test-bool-indices (vector)
  "Return indexes of non-nil entries in bool VECTOR."
  (let (result)
    (dotimes (index (length vector) (nreverse result))
      (when (aref vector index)
        (push index result)))))

(defun neomacs-symon-test-cancel-timers ()
  "Cancel and clear Symon's current timer objects."
  (dolist (timer symon--timer-objects)
    (when (timerp timer)
      (cancel-timer timer)))
  (setq symon--timer-objects nil))
"##;

fn symon_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SYMON_MELPA_PIN, "symon.el")
        .expect("prepare exact shallow symon source below ./tmp")
        .with_prelude(SYMON_TEST_PRELUDE)
        .with_timeout(SYMON_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed symon parity test")
        .into()
}

fn assert_symon_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(symon_oracle(), &current_test_name(), "symon_parity", cases);
}

#[test]
fn symon_package_batch() {
    assert_symon_batch(&workflows::workflow_batch_cases());
}
