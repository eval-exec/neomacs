//! Practical parity for company-go's gocode completion backend.
//!
//! The backend shells out to the external `gocode' daemon.  The workflows
//! cover what a user actually configures and what the elisp computes: the
//! documented defcustoms, the pure CSV candidate pipeline
//! (`company-go--format-meta', `company-go--get-candidates'), the
//! autocomplete invocation contract through a fake gocode script that
//! records its argv and answers canned CSV (so the real arg assembly --
//! buffer file name, the `c<offset>' cursor argument, the csv-with-package
//! formatter, and the user's extra arguments -- is exercised end to end;
//! the cursor argument is recorded as the argument, never as an index into
//! the recorded argv, which quotes the sandbox path -- DIVERGENCES.md 127),
//! and the prefix contract at member-access dots.

use std::time::Duration;

use crate::{COMPANY_GO_MELPA_PIN, COMPANY_MELPA_PIN, CachedMelpaOracle, GO_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'company-go)

(defconst cgo319-test-upstream-tree
  "6a38841c337f3615d18392d0d2d6d3292b9b1092"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst cgo319-test-manifest
  '(("company-go-pkg.el"
     . "985ecf9ef57330a74be0327461850fbea41161b7f82c3f5d8e9c8f8a48655d55")
    ("company-go.el"
     . "9f782059e741ec0edf559687b8e1d10961cfa49fb96a5db7e70e455b7983a261"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun cgo319-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "company-go.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/company-go.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed company-go location: %S" located))
    (dolist (entry cgo319-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed company-go source: %S"
                   (car entry))))))
    (list :upstream-tree cgo319-test-upstream-tree
          :feature (featurep 'company-go)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'company-go package-alist))))
          :company (package-version-join
                    (package-desc-version
                     (cadr (assq 'company package-alist))))
          :go-mode (package-version-join
                    (package-desc-version
                     (cadr (assq 'go-mode package-alist)))))))

(defun cgo319-test-root ()
  "A fresh fixture root under the sandbox."
  (let ((root (file-name-as-directory
               (expand-file-name
                "company-go-fixture"
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (ignore-errors (delete-directory root t))
    (make-directory root t)
    root))

(defun cgo319-test-fake-gocode (root csv)
  "Write a fake gocode under ROOT answering CSV and recording its argv.
The argv lands in argv.txt beside the script; stdout is the canned CSV."
  (let ((script (expand-file-name "gocode" root))
        (recorder (expand-file-name "argv.txt" root)))
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file script
        (insert "#!/bin/sh\n")
        (insert (format "printf '%%s\\n' \"$@\" > %s\n"
                        (shell-quote-argument recorder)))
        (insert (format "cat <<'CGOEOF'\n%s\nCGOEOF\n" csv))))
    (set-file-modes script #o755)
    script))

(defun cgo319-test-reset ()
  "Remove fixtures and restore toggled settings."
  (setq company-go-gocode-command "gocode"
        company-go-gocode-args nil
        company-go-begin-after-member-access t
        company-go-show-annotation nil
        company-go-insert-arguments t)
  (ignore-errors
    (delete-directory
     (file-name-as-directory
      (expand-file-name
       "company-go-fixture"
       (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
     t)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_GO_MELPA_PIN, "company-go.el")
        .expect("prepare pinned company-go source below ./tmp")
        .with_melpa_dependency(COMPANY_MELPA_PIN)
        .expect("prepare pinned company dependency")
        .with_melpa_dependency(GO_MODE_MELPA_PIN)
        .expect("prepare pinned go-mode dependency")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn company_go_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "company_go_package_batch",
        "company_go_parity",
        &cases,
    );
}
