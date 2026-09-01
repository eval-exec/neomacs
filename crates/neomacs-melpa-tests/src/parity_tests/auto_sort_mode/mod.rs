use crate::{AUTO_SORT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUTO_SORT_MODE_TEST_PRELUDE: &str = r####"
(require 'sort)

(defvar neomacs-auto-sort-test--events nil)

(defun neomacs-auto-sort-test--root (name)
  "Return a deterministic package-test directory for NAME."
  (file-name-as-directory
   (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-auto-sort-test--write-file (path contents)
  "Create PATH and write exact UTF-8 CONTENTS."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file path
      (insert contents))))

(defun neomacs-auto-sort-test--file-text (path)
  "Read PATH as exact UTF-8 text."
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path))
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-auto-sort-test--hook-count (function hook)
  "Count entries exactly equal to FUNCTION in HOOK."
  (let ((count 0))
    (dolist (entry hook count)
      (when (eq entry function)
        (setq count (1+ count))))))

(defun neomacs-auto-sort-test--before-save ()
  "Record the exact document presented to an ordinary before-save hook."
  (push (list :before
              (buffer-substring-no-properties (point-min) (point-max))
              (buffer-modified-p))
        neomacs-auto-sort-test--events))

(defun neomacs-auto-sort-test--after-save ()
  "Record the exact document presented to an ordinary after-save hook."
  (push (list :after
              (buffer-substring-no-properties (point-min) (point-max))
              (buffer-modified-p))
        neomacs-auto-sort-test--events))

(defun neomacs-auto-sort-test--cleanup (buffers root)
  "Kill BUFFERS without prompts and remove deterministic ROOT."
  (dolist (buffer buffers)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (setq buffer-read-only nil)
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (when (and root (file-exists-p root))
    (delete-directory root t))
  (setq neomacs-auto-sort-test--events nil))
"####;

fn auto_sort_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_SORT_MODE_MELPA_PIN, "auto-sort-mode.el")
        .expect("prepare pinned auto-sort-mode source below ./tmp")
        .with_prelude(AUTO_SORT_MODE_TEST_PRELUDE)
        .with_installed_autoloads()
}

#[test]
fn auto_sort_mode_package_batch() {
    assert_oracle_batch_cases(
        auto_sort_mode_oracle(),
        "auto_sort_mode_package_batch",
        "auto_sort_mode_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
