//! Practical parity for ov's overlay sugar.  Every workflow is a pure
//! buffer-overlay operation: creating overlays by line/match/regexp/
//! region, setting properties, counting and navigating, and clearing by
//! property or region.

use std::time::Duration;

use crate::{CachedMelpaOracle, OV_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst ov297-test-upstream-tree
  "2b6bdc185bd29de48a90f2bccaf098428c923fc1"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst ov297-test-manifest
  '(("ov-pkg.el"
     . "b0cd2f33960478d61aab5fd3bea4a2f7e0bf426cef8ec542524325c8bc5bf1d5")
    ("ov.el"
     . "5f0d070475575581c135137834920b9890db011febeb7f8f8602e1276ecbb134"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun ov297-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "ov.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/ov.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed ov location: %S" located))
    (dolist (entry ov297-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed ov source: %S" (car entry))))))
    (list :upstream-tree ov297-test-upstream-tree
          :feature (featurep 'ov)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'ov package-alist)))))))

(defun ov297-test-overlay-state ()
  "Compact (BEG END PROPS) list of every live overlay."
  (mapcar (lambda (ov)
            (list (overlay-start ov)
                  (overlay-end ov)
                  (overlay-get ov 'face)))
          (sort (overlays-in (point-min) (point-max))
                (lambda (a b) (< (overlay-start a) (overlay-start b))))))

(defun ov297-test-setup ()
  "A standard four-line buffer with point on line 2."
  (insert "alpha line\nbeta line\ngamma line\ndelta line\n")
  (goto-char (point-min))
  (forward-line 1))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(OV_MELPA_PIN, "ov.el")
        .expect("prepare pinned ov source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn ov_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "ov_package_batch", "ov_parity", &cases);
}
