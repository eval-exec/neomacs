//! Practical parity for session.  The package saves variables, rings,
//! histories, registers, and buffer places into a session file and
//! restores them, plus the undo-based last-change jumping.  Everything
//! is pure Elisp: the suite runs the real save/restore round trip into
//! a sandboxed session file.

use std::time::Duration;

use crate::{CachedMelpaOracle, SESSION_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar session--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar session--test-fixtures
  (file-name-as-directory (expand-file-name "session-fixtures"
                                            session--test-root)))
(defvar session--test-file
  (expand-file-name "session-file" session--test-root))

;; Provenance: pinned upstream 3be207c50dfe964de3cbf5cd8fa9b07fc7d2e609.
(defconst session--test-upstream-tree
  "07c7cdf82e023796be74671577b9d0e1fde6c19e"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst session--test-manifest
  '(("session.el" . "8e1eafc3ed9d069b7785c89e605cd472118a00c8975fb30e8929358b40bfb01d"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun session--test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun session--test-read (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path)
      (buffer-string))))

(defun session--test-normalize (text)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name session--test-root))
   "@@ROOT@@" text t t))

(defun session--test-cleanup ()
  "Kill the buffers the workflows visit."
  (dolist (buf (buffer-list))
    (when (and (buffer-file-name buf)
               (string-prefix-p (directory-file-name session--test-fixtures)
                                (buffer-file-name buf)))
      (with-current-buffer buf
        (set-buffer-modified-p nil)
        (kill-buffer)))))

(defun session--test-setup ()
  "Configure the sandboxed session file and enable the package."
  ;; The default -ring/-history sweep would save the editor's whole
  ;; batch history, which legitimately differs between processes; the
  ;; documented include list scopes the save to the case's own data.
  (setq session-save-file session--test-file
        session-use-package t
        session-globals-include '(regexp-search-ring minibuffer-history))
  (session-initialize))

(defun session--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "session.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/session.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed session location: %S" located))
    (dolist (entry session--test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed session source: %S"
                   (car entry))))))
    (list :upstream-tree session--test-upstream-tree
          :feature (featurep 'session)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'session package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SESSION_MELPA_PIN, "session.el")
        .expect("prepare pinned session source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn session_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "session_package_batch", "session_parity", &cases);
}
