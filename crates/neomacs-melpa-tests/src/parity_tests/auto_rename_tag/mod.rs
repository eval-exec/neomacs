use std::time::Duration;

use crate::{AUTO_RENAME_TAG_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUTO_RENAME_TAG_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const AUTO_RENAME_TAG_TEST_PRELUDE: &str = r####"
(defvar iedit-mode nil)

(defun neomacs-auto-rename-tag-test--replace (old new &optional occurrence)
  "Replace OCCURRENCE of OLD with NEW through ordinary buffer edits."
  (goto-char (point-min))
  (dotimes (_ (or occurrence 1))
    (search-forward old))
  (let ((end (point))
        (start (- (point) (length old))))
    (delete-region start end)
    (insert new)))

(defun neomacs-auto-rename-tag-test--text ()
  "Return the current buffer text without mode-added properties."
  (buffer-substring-no-properties (point-min) (point-max)))

(defun neomacs-auto-rename-tag-test--file-text (file)
  "Return FILE's exact literal contents."
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun neomacs-auto-rename-tag-test--hook-count (function hook)
  "Count entries exactly equal to FUNCTION in HOOK."
  (let ((count 0))
    (dolist (entry hook count)
      (when (eq entry function)
        (setq count (1+ count))))))

(defun neomacs-auto-rename-tag-test--run-edit
    (mode text old new &optional occurrence command)
  "Run one realistic tag edit and return its exact buffer state."
  (with-temp-buffer
    (funcall mode)
    (insert text)
    (auto-rename-tag-mode 1)
    (let ((this-command command))
      (neomacs-auto-rename-tag-test--replace old new occurrence))
    (list
     :text (neomacs-auto-rename-tag-test--text)
     :point (point)
     :activated auto-rename-tag--pre-command-activated
     :previous auto-rename-tag--record-prev-word)))

(defun neomacs-auto-rename-tag-test--cleanup-file (buffer root)
  "Kill BUFFER without prompting and remove ROOT."
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (set-buffer-modified-p nil))
    (kill-buffer buffer))
  (when (and root (file-exists-p root))
    (delete-directory root t)))
"####;

fn auto_rename_tag_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_RENAME_TAG_MELPA_PIN, "auto-rename-tag.el")
        .expect("prepare pinned auto-rename-tag source below ./tmp")
        .with_prelude(AUTO_RENAME_TAG_TEST_PRELUDE)
        .with_installed_autoloads()
        .with_timeout(AUTO_RENAME_TAG_TEST_TIMEOUT)
}

#[test]
fn auto_rename_tag_package_batch() {
    assert_oracle_batch_cases(
        auto_rename_tag_oracle(),
        "auto_rename_tag_package_batch",
        "auto_rename_tag_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
