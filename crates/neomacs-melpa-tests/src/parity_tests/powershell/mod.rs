//! Practical parity for powershell.  The package is a PowerShell editing
//! mode: the indent command with continuation lines, the quoting and
//! escaping region helpers, the regexp conversion, and the mode's
//! font-lock/imenu setup.  Everything is pure Elisp.

use std::time::Duration;

use crate::{CachedMelpaOracle, POWERSHELL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar powershell--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

;; Provenance: pinned upstream ae60e11c96cc1767f05ce0cab6a917240ce2e37a.
(defconst powershell--test-upstream-tree
  "7fe94817c4ca016ba7e6e9c02658d234af0f9ac8"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst powershell--test-manifest
  '(("powershell.el" . "0f64ab6c38d1a49b9023bbc05f141531fff2666de14619c0a77c102197afd6e5"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun powershell--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "powershell.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/powershell.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed powershell location: %S" located))
    (dolist (entry powershell--test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed powershell source: %S"
                   (car entry))))))
    (list :upstream-tree powershell--test-upstream-tree
          :feature (featurep 'powershell)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'powershell package-alist)))))))

(defun powershell--test-buffer (text)
  "Create a temp buffer in powershell-mode holding TEXT, return it."
  (with-current-buffer (generate-new-buffer "*powershell-test*")
    (powershell-mode)
    (insert text)
    (current-buffer)))

(defun powershell--test-select-region (beg end)
  "Select BEG..END so the interactive region helpers run."
  (setq mark-active t)
  (set-mark beg)
  (goto-char end))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(POWERSHELL_MELPA_PIN, "powershell.el")
        .expect("prepare pinned powershell source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn powershell_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "powershell_package_batch",
        "powershell_parity",
        &cases,
    );
}
