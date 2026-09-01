//! Practical parity for dart-mode's editing surface.
//!
//! The workflows enter the documented way (`find-file' a `.dart' file via
//! the autoloaded `auto-mode-alist' entry) and pin the mode's setup: the
//! prog-mode derivation with Dart's syntax-table entries (C++-style block
//! comments, string quotes), the electric-indent configuration, the
//! comment variables, `font-lock-defaults', the two-space tab policy, and
//! the syntax-propertize function that fences raw/multiline strings.
//! Indentation runs through `dart-indent-line-relative'; a real Dart
//! program is fontified line-by-line like hy-mode's corpus.

use std::time::Duration;

use crate::{CachedMelpaOracle, DART_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst dart793-test-upstream-tree
  "cf2e800047a5a23401538241424b34c68335cd30"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst dart793-test-manifest
  '(("dart-mode-pkg.el"
     . "1028a3777c7f68ab97009083658d043aebb1aacc850c3bb9459cc4ff3d779736")
    ("dart-mode.el"
     . "11b057abcee7adeaf4711165de93d925584290d93cd5a079f22064c8128d2580"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun dart793-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "dart-mode.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/dart-mode.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed dart-mode location: %S" located))
    (dolist (entry dart793-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed dart-mode source: %S"
                   (car entry))))))
    (list :upstream-tree dart793-test-upstream-tree
          :feature (featurep 'dart-mode)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'dart-mode package-alist)))))))

(defun dart793-test-face-runs (beg end)
  "Compact (TEXT FACES) runs over [BEG, END) of the current buffer."
  (let ((runs nil)
        (pos beg))
    (while (< pos end)
      (let* ((faces (let ((value (get-text-property pos 'face)))
                      (if (listp value) value (list value))))
             (start pos))
        (while (and (< pos end)
                    (equal (let ((value (get-text-property pos 'face)))
                             (if (listp value) value (list value)))
                           faces))
          (cl-incf pos))
        (push (list (buffer-substring-no-properties start pos) faces)
              runs)))
    (nreverse runs)))

(defun dart793-test-line-runs (needle)
  "Face runs of the whole line whose content matches NEEDLE first."
  (save-excursion
    (goto-char (point-min))
    (if (not (search-forward needle nil t))
        (list :needle needle :not-found)
      (let ((bol (line-beginning-position))
            (eol (line-end-position)))
        (list :needle needle
              :line (buffer-substring-no-properties bol eol)
              :runs (dart793-test-face-runs bol eol))))))

(defun dart793-test-fixture (name)
  "Create fixture NAME under the sandbox and return its path."
  (let ((root (file-name-as-directory
               (expand-file-name
                "dart-mode-fixtures"
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (make-directory root t)
    (expand-file-name name root)))

(defun dart793-test-reset ()
  "Kill fixture buffers and remove the fixture root."
  (dolist (buffer (buffer-list))
    (let ((name (buffer-name buffer)))
      (when (string-suffix-p ".dart" name)
        (unless (eq buffer (current-buffer))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (ignore-errors (kill-buffer buffer))))))
  (ignore-errors
    (delete-directory
     (file-name-as-directory
      (expand-file-name
       "dart-mode-fixtures"
       (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
     t)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DART_MODE_MELPA_PIN, "dart-mode.el")
        .expect("prepare pinned dart-mode source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn dart_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "dart_mode_package_batch",
        "dart_mode_parity",
        &cases,
    );
}
