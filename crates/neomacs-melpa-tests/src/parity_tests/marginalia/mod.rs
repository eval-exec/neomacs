use std::time::Duration;

use crate::{CachedMelpaOracle, MARGINALIA_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const MARGINALIA_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const MARGINALIA_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'marginalia)

(defun neomacs-marginalia-test-plain (value)
  "Return VALUE with text properties removed from any strings within."
  (cond
   ((stringp value) (substring-no-properties value))
   ((consp value)
    (cons (neomacs-marginalia-test-plain (car value))
          (neomacs-marginalia-test-plain (cdr value))))
   (t value)))

(defun neomacs-marginalia-test-annotate (category candidate)
  "Return the plain annotation string for CANDIDATE in CATEGORY."
  (let ((fun (cadr (assq category marginalia-annotators))))
    (unless fun
      (error "No annotator registered for category %s" category))
    (let ((result (funcall fun candidate)))
      (and result (substring-no-properties result)))))

(defun neomacs-marginalia-test-outcome (function)
  "Return FUNCTION's value or its exact signal identity and message."
  (condition-case err
      (list :value (funcall function))
    (error
     (list :signal (car err)
           :message (error-message-string err)))))
"##;

fn marginalia_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MARGINALIA_MELPA_PIN, "marginalia.el")
        .expect("prepare exact shallow Marginalia source and dependencies below ./tmp")
        .with_prelude(MARGINALIA_TEST_PRELUDE)
        .with_timeout(MARGINALIA_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Marginalia parity test")
        .into()
}

fn assert_marginalia_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        marginalia_oracle(),
        &current_test_name(),
        "marginalia_parity",
        cases,
    );
}

#[test]
fn marginalia_package_batch() {
    assert_marginalia_batch(&workflows::workflow_batch_cases());
}
