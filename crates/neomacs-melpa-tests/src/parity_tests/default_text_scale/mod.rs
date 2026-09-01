//! Practical parity for default-text-scale.
//!
//! The package adjusts the default face height across graphical frames;
//! \`default-text-scale-increment' explicitly ERRORS from a
//! non-graphical frame, which is exactly what a batch editor is.  The
//! workflows pin that documented behavior (both editors agree), the
//! mode's lifecycle and keymap, the reset-with-prefix path that does
//! work in batch, and the propagation of the error through the
//! autoloaded commands.

use std::time::Duration;

use crate::{CachedMelpaOracle, DEFAULT_TEXT_SCALE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst dtsbfc-test-upstream-tree
  "224204197a626e852e5afb38691fbb222549bc56"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst dtsbfc-test-manifest
  '(("default-text-scale-pkg.el"
     . "bf489ffc792bb45b5778ae9bb7914189aae1dc286a7a71b32e9ddf3b499e574e")
    ("default-text-scale.el"
     . "1cd96664a1f8b0ab09c817c06362e9850e282988f3cb570935020cdc84589d8d"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun dtsbfc-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "default-text-scale.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/default-text-scale.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed default-text-scale location: %S"
             located))
    (dolist (entry dtsbfc-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed default-text-scale source: %S"
                   (car entry))))))
    (list :upstream-tree dtsbfc-test-upstream-tree
          :feature (featurep 'default-text-scale)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'default-text-scale package-alist)))))))

(defun dtsbfc-test-reset ()
  "Leave the mode off and the hook removed."
  (when (bound-and-true-p default-text-scale-mode)
    (ignore-errors (default-text-scale-mode -1)))
  (remove-hook 'after-make-frame-functions
               #'default-text-scale--update-for-new-frame)
  (setq default-text-scale--complement 0
        default-text-scale-amount 10))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DEFAULT_TEXT_SCALE_MELPA_PIN, "default-text-scale.el")
        .expect("prepare pinned default-text-scale source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn default_text_scale_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "default_text_scale_package_batch",
        "default_text_scale_parity",
        &cases,
    );
}
