//! Practical parity for noflet's dynamic function rebinding.
//!
//! noflet is `flet' with an escape hatch: each binding can call the
//! ORIGINAL function through `this-fn'.  The workflows override real
//! functions, delegate through `this-fn', nest overrides, and verify the
//! original definitions are restored on exit.

use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, NOFLET_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; noflet.el calls dash's -map/-take-while at LOAD time under eager
;; macro expansion without requiring dash; a user's init has dash loaded
;; first (it is a de-facto load-time dependency the package never
;; declared).  Require it before the source load or GNU 31 refuses the
;; file outright.
(require 'dash)

(defconst nf7ae-test-upstream-tree
  "06ef64caedc804601aba7df0638d386f23803848"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst nf7ae-test-manifest
  '(("noflet-pkg.el"
     . "dd56089540ee6853002c6d36b6100b5b37e09a4ba2633de0f7ea7f4946576d73")
    ("noflet.el"
     . "13e12bc4cddc61db1afc4a3ec6e50787b000a494c6461644369e469c871c8666"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun nf7ae-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "noflet.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/noflet.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed noflet location: %S" located))
    (dolist (entry nf7ae-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed noflet source: %S"
                   (car entry))))))
    (list :upstream-tree nf7ae-test-upstream-tree
          :feature (featurep 'noflet)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'noflet package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NOFLET_MELPA_PIN, "noflet.el")
        .expect("prepare pinned noflet source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash load-time dependency")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn noflet_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "noflet_package_batch", "noflet_parity", &cases);
}
