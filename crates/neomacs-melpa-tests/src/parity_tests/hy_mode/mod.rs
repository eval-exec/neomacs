use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, HY_MODE_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const HY_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Every workflow enters the documented way: `find-file' a `.hy' file, which
/// the package's autoloaded `auto-mode-alist' entry turns into `hy-mode'.
/// The mode is a pure editing surface (syntax table, font-lock, lisp
/// indentation with Hy's own indent specs, keymap, and the shell/describe
/// command bindings), so the whole public surface is batch-observable; the
/// workflows deliberately never launch the `hy' binary, only pin the
/// commands and keymap entries that would.
///
/// hy-mode derives its syntax table from `lisp-mode-syntax-table' and adds
/// Hy-specific entries (`{' `}' `[' `]' as parentheses, `~' and `@' as
/// quotes, `,' `|' `#' as symbol constituents) plus a
/// `syntax-propertize-function' that fences `#[d[...]]' bracket strings with
/// string-fence syntax.  Indentation runs through `lisp-indent-line' with
/// `lisp-indent-function' bound to `hy-indent-function', which reads the
/// two public spec lists (`hy-indent--exactly', `hy-indent--fuzzily') and
/// falls back to `calculate-lisp-indent' otherwise.
const HY_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst hy-test-upstream-tree
  "2245e7658c4a87285218aa72a71c368a5d504245"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst hy-test-manifest
  '(("hy-base.el"
     . "5b7d6e290cbbb46f1db65834ec59d64b5753f338433aa46a558693b6d468f9e0")
    ("hy-font-lock.el"
     . "8cea10cff978012c6a7cf947e8c1bd20953e31d3b047a71c66434fef301b6efc")
    ("hy-jedhy.el"
     . "9b5ec7d51bc227283856e33b5bed6251a7a6755a7ad6cdecd544314fe3935806")
    ("hy-mode.el"
     . "7116f9f438783ff088bba22041635d8f643049198310b9f39b064ad10d13914a")
    ("hy-mode-pkg.el"
     . "4c1e1421fddc7137d2d664b761f05bc8128e4e72ac5cb13b34a68566ec351b6d")
    ("hy-shell.el"
     . "e46ad9c2fb43aa9025e375c819fc3e48c6f454ec1c47aa26f7ac7379956bff0c"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun hy-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "hy-mode.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/hy-mode.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed hy-mode location: %S" located))
    (dolist (entry hy-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed hy-mode source: %S"
                   (car entry))))))
    (list :upstream-tree hy-test-upstream-tree
          :feature (featurep 'hy-mode)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'hy-mode package-alist))))
          :dash (package-version-join
                 (package-desc-version (cadr (assq 'dash package-alist))))
          :s (package-version-join
              (package-desc-version (cadr (assq 's package-alist)))))))

(defun hy-test-face-runs (beg end)
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
        (push (list (buffer-substring-no-properties start pos)
                    faces)
              runs)))
    (nreverse runs)))

(defun hy-test-line-runs (needle)
  "Face runs of the whole line whose content matches NEEDLE first."
  (save-excursion
    (goto-char (point-min))
    (if (not (search-forward needle nil t))
        (list :needle needle :not-found)
      (let ((bol (line-beginning-position))
            (eol (line-end-position)))
        (list :needle needle
              :line (buffer-substring-no-properties bol eol)
              :runs (hy-test-face-runs bol eol))))))

(defun hy-test-open (name)
  "Create fixture NAME in the sandbox, visit it, and turn font-lock on."
  (let* ((root (file-name-as-directory
                (expand-file-name
                 "hy-mode-fixtures"
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (file (expand-file-name name root)))
    (unless (file-directory-p root)
      (make-directory root t))
    file))

(defun hy-test-reset ()
  "Kill fixture buffers and leave the mode surface untouched."
  (dolist (buffer (buffer-list))
    (let ((name (buffer-name buffer)))
      (when (string-suffix-p ".hy" name)
        (unless (eq buffer (current-buffer))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (ignore-errors (kill-buffer buffer))))))
  (ignore-errors (delete-directory
                  (file-name-as-directory
                   (expand-file-name
                    "hy-mode-fixtures"
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                  t)))
"##;

fn hy_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HY_MODE_MELPA_PIN, "hy-mode.el")
        .expect("prepare pinned hy-mode source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency")
        .with_prelude(HY_MODE_TEST_PRELUDE)
        .with_timeout(HY_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed hy-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_hy_mode_parity` cases (2a).
pub(crate) fn assert_hy_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(hy_mode_oracle(), &name, "hy_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn hy_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_hy_mode_batch(&cases);
}

// END generated package batch tests
