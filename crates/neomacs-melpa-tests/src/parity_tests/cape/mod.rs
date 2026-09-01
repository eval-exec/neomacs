//! Practical parity for cape.  The package is a family of Capf
//! (completion-at-point-function) providers plus combinators over the
//! standard completion table contract: elisp symbols, file names,
//! dynamic-abbrev words, whole lines, abbreviations, and the wrapping
//! combinators.  Everything is pure Elisp, so the suite runs the real
//! tables and the real `cape-interactive' UI path with only the
//! unattended-minibuffer completing-read stand-in.

use std::time::Duration;

use crate::{CAPE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar cape--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar cape--test-fixtures
  (file-name-as-directory (expand-file-name "cape-fixtures" cape--test-root)))

;; Provenance: pinned upstream 96c26eb54ef27c404554272489b8f9d78f113a2b.
(defconst cape--test-upstream-tree
  "5275a3af96874e280eb82814412ff6a7ce7ff5f9"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst cape--test-manifest
  '(("cape.el" . "61929dd92b7af33914a5a0b36e898bc923d261c6345241cf38e9d7213a37adee"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun cape--test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun cape--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "cape.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/cape.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed cape location: %S" located))
    (dolist (entry cape--test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed cape source: %S"
                   (car entry))))))
    (list :upstream-tree cape--test-upstream-tree
          :feature (featurep 'cape)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'cape package-alist)))))))

(defvar cape--test-messages nil)
(defvar cape--test-reads nil)

(defmacro cape--test-with-ui-capture (&rest body)
  "Run BODY with `message' captured and `completing-read' fed the
first option of the real collection it was offered (the unattended
minibuffer stand-in)."
  `(let ((cape--test-messages nil)
         (cape--test-reads nil))
     (cl-letf (((symbol-function 'message)
                (lambda (fmt &rest args)
                  (push (apply #'format-message fmt args)
                        cape--test-messages)))
               ((symbol-function 'completing-read)
                (lambda (prompt collection &rest _)
                  (push (list :prompt prompt :options collection)
                        cape--test-reads)
                  (car collection))))
       ,@body)))

(defun cape--test-result (&rest plist)
  (append
   plist
   (list :messages (nreverse cape--test-messages)
         :reads (nreverse cape--test-reads))))

(defun cape--test-completions (table string)
  "Run the Capf TABLE on STRING and return its completion strings."
  (sort (all-completions string table) #'string<))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CAPE_MELPA_PIN, "cape.el")
        .expect("prepare pinned cape source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn cape_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "cape_package_batch", "cape_parity", &cases);
}
