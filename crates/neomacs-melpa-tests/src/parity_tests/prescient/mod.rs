//! Practical parity for prescient's filtering and sorting.
//!
//! prescient is a pure candidate-ranking engine (literal/regexp/
//! initialism filtering, frequency+recency sorting) — every workflow is
//! batch-observable over in-memory candidate lists and a cache that the
//! prelude points into the sandbox.

use std::time::Duration;

use crate::{CachedMelpaOracle, PRESCIENT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst pr564-test-upstream-tree
  "ba7d18e7cbfc4e6483ce786b6e1698d065ed9499"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst pr564-test-manifest
  '(("prescient-pkg.el"
     . "a8ae4cb97f872f75d39fb4d09d6a5d2ee1821071290b252cc3b6bcb737884990")
    ("prescient.el"
     . "997fa52b730c1903bf82ce607d177baca4b5d6ee1718a8afb43b61056b17f576"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun pr564-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "prescient.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/prescient.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed prescient location: %S" located))
    (dolist (entry pr564-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed prescient source: %S"
                   (car entry))))))
    (list :upstream-tree pr564-test-upstream-tree
          :feature (featurep 'prescient)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'prescient package-alist)))))))

(defvar pr564-test-save-file-backup)

(defun pr564-test-reset ()
  "Clear the in-memory tables, restore the save file, and disable persist."
  (when (bound-and-true-p prescient-persist-mode)
    (prescient-persist-mode -1))
  (setq prescient--history (make-hash-table :test 'equal)
        prescient--frequency (make-hash-table :test 'equal)
        prescient--serial-number 0
        prescient--cache-loaded nil
        prescient-save-file
        (expand-file-name "prescient-save.el"
                          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (ignore-errors
    (delete-file (expand-file-name "prescient-save.el.~1~"
                                    (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PRESCIENT_MELPA_PIN, "prescient.el")
        .expect("prepare pinned prescient source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn prescient_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "prescient_package_batch",
        "prescient_parity",
        &cases,
    );
}
