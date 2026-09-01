//! Practical parity for caml.  The package is an OCaml editing mode:
//! the indent command, phrase (;;) movement, comments, the
//! match-form skeleton, and the current-defun index.
//! Everything is pure Elisp, so the suite runs the real commands on
//! realistic OCaml source with no external stand-ins.

use std::time::Duration;

use crate::{CAML_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar caml--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar caml--test-fixtures
  (file-name-as-directory (expand-file-name "caml-fixtures" caml--test-root)))

;; Provenance: pinned upstream 744333dc4c4bd8b93e037efa8f7362b0903b96a2.
(defconst caml--test-upstream-tree
  "e635b82cce1666662555900bbf12d084989d73ed"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst caml--test-manifest
  '(("caml.el" . "5ad9422936cef0475babe63fc9951243f71a2a6a6d04a3c142050a198edbaf4a"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun caml--test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun caml--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "caml.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/caml.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed caml location: %S" located))
    (dolist (entry caml--test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed caml source: %S"
                   (car entry))))))
    (list :upstream-tree caml--test-upstream-tree
          :feature (featurep 'caml)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'caml package-alist)))))))

(defun caml--test-ml-buffer (text)
  "Create a temp buffer in caml-mode holding TEXT, return it."
  (with-current-buffer (generate-new-buffer "*caml-test*")
    (caml-mode)
    (insert text)
    (current-buffer)))

(defun caml--test-result (&rest plist)
  (append plist nil))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CAML_MELPA_PIN, "caml.el")
        .expect("prepare pinned caml source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn caml_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "caml_package_batch", "caml_parity", &cases);
}
