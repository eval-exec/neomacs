use crate::{AUTO_SPACE_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUTO_SPACE_MODE_TEST_PRELUDE: &str = r####"
(defun neomacs-auto-space-test--type (text)
  "Insert TEXT one character at a time through `self-insert-command'."
  (dolist (character (string-to-list text))
    (self-insert-command 1 character)))

(defun neomacs-auto-space-test--hook-count ()
  "Count auto-space entries in the global self-insert hook."
  (let ((count 0))
    (dolist (entry (default-value 'post-self-insert-hook) count)
      (when (eq entry 'auto-space--add-space-between-cjk-and-ascii)
        (setq count (1+ count))))))

(defun neomacs-auto-space-test--reset ()
  "Restore auto-space global state after an isolated workflow."
  (when (bound-and-true-p auto-space-mode)
    (auto-space-mode -1))
  (remove-hook 'post-self-insert-hook
               'auto-space--add-space-between-cjk-and-ascii))

(defun neomacs-auto-space-test--kill-buffers (buffers)
  "Kill live BUFFERS without prompting."
  (dolist (buffer buffers)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"####;

fn auto_space_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_SPACE_MODE_MELPA_PIN, "auto-space-mode.el")
        .expect("prepare pinned auto-space-mode source below ./tmp")
        .with_prelude(AUTO_SPACE_MODE_TEST_PRELUDE)
        .with_installed_autoloads()
}

#[test]
fn auto_space_mode_package_batch() {
    assert_oracle_batch_cases(
        auto_space_mode_oracle(),
        "auto_space_mode_package_batch",
        "auto_space_mode_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
