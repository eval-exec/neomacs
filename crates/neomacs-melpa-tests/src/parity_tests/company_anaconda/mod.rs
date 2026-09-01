use std::time::Duration;

use crate::{COMPANY_ANACONDA_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'company-anaconda)

(defvar neomacs-company-anaconda-test-results nil)
(defvar neomacs-company-anaconda-test-calls nil)

(defun neomacs-company-anaconda-test-reset ()
  "Reset the deterministic Anaconda RPC boundary."
  (setq neomacs-company-anaconda-test-results
        [ ["discounted" "function: inventory.Widget.discounted"
           "discounted(percent)\n\nReturn the discounted price."
           "/workspace/inventory.py" 11]
          ["display_name" "function: inventory.Widget.display_name"
           "display_name()\n\nReturn the visible name."
           "/workspace/inventory.py" 15]
          ["duplicate" "function: inventory.Widget.duplicate"
           "duplicate()\n\nReturn an independent copy."
           "/workspace/inventory.py" 19] ])
  (setq neomacs-company-anaconda-test-calls nil))

(defun neomacs-company-anaconda-test-rpc (command callback)
  "Record COMMAND and asynchronously CALLBACK with deterministic results."
  (push (list command (buffer-substring-no-properties (point-min) (point-max))
              (line-number-at-pos) (current-column))
        neomacs-company-anaconda-test-calls)
  (run-at-time 0 nil callback neomacs-company-anaconda-test-results))

(defun neomacs-company-anaconda-test-plain-candidates ()
  "Return Company candidates without display properties."
  (mapcar #'substring-no-properties company-candidates))

(defun neomacs-company-anaconda-test-candidate (name description doc path line)
  "Build a candidate carrying the package's server struct property."
  (let ((candidate (copy-sequence name))
        (struct (vector name description doc path line)))
    (put-text-property 0 1 'struct struct candidate)
    candidate))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_ANACONDA_MELPA_PIN, "company-anaconda.el")
        .expect("prepare exact Company Anaconda source and dependency graph below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn company_anaconda_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "company_anaconda_package_batch",
        "company_anaconda_parity",
        &workflows::workflow_batch_cases(),
    );
}
