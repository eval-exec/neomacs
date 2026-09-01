//! Practical parity for scratch-el's mode-specific scratch buffers.
//!
//! \`scratch' creates a buffer in the current major mode (or a mapped
//! one), copies the active region in, and pops to it; repeat invocations
//! reuse the existing buffer.  All batch-observable.

use std::time::Duration;

use crate::{CachedMelpaOracle, SCRATCH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; shell-mode is autoloaded lazily; the mode-alist workflow calls it directly.
(require 'shell)

(defconst scf00-test-upstream-tree
  "944053221a06cb4ac8c46692e80db3375e025988"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst scf00-test-manifest
  '(("scratch-pkg.el"
     . "49ca8c527201aecc322211753d0be3df3e3c74f78c30909ad36d2e6daf16551d")
    ("scratch.el"
     . "3ab7ef8d6323154359a21eb8356cb7308f0225c74414d3a70e61cb5abb1ebc10"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun scf00-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "scratch.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/scratch.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed scratch location: %S" located))
    (dolist (entry scf00-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed scratch source: %S"
                   (car entry))))))
    (list :upstream-tree scf00-test-upstream-tree
          :feature (featurep 'scratch)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'scratch package-alist)))))))

(defun scf00-test-reset ()
  "Kill scratch buffers produced by the workflows."
  (dolist (buffer (buffer-list))
    (let ((name (buffer-name buffer)))
      (when (and name (string-prefix-p "*scf00-" name))
        (unless (eq buffer (current-buffer))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (ignore-errors (kill-buffer buffer)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SCRATCH_MELPA_PIN, "scratch.el")
        .expect("prepare pinned scratch source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn scratch_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "scratch_package_batch", "scratch_parity", &cases);
}
