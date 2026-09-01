use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_MELPA_PIN, TREEMACS_EVIL_MELPA_PIN, TREEMACS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TREEMACS_EVIL_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const TREEMACS_EVIL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'treemacs-evil)

(defun neomacs-treemacs-evil-test-state-binding (key)
  "Return the command bound to KEY in the global Treemacs Evil state map."
  (lookup-key evil-treemacs-state-map (kbd key)))

(defun neomacs-treemacs-evil-test-mode-binding (key)
  "Return Treemacs mode's state-local command bound to KEY."
  (let ((map (evil-get-auxiliary-keymap treemacs-mode-map 'treemacs)))
    (and map (lookup-key map (kbd key)))))

(defun neomacs-treemacs-evil-test-buffer ()
  "Create a small real Treemacs-mode buffer with navigable buttons."
  (let ((neomacs-treemacs-evil-test-created-buffer
         (generate-new-buffer " *treemacs-evil-test*")))
    (save-window-excursion
      ;; Treemacs is a window-oriented application.  Enter its mode while the
      ;; destination buffer is displayed, just as the public `treemacs'
      ;; command does.  The public command marks the destination as owned by
      ;; Treemacs before calling its protected major-mode initializer.
      (switch-to-buffer neomacs-treemacs-evil-test-created-buffer)
      (setq-local treemacs--in-this-buffer t)
      (cl-letf (((symbol-function 'treemacs--on-window-config-change)
                 #'ignore))
        (treemacs-mode))
      (with-current-buffer neomacs-treemacs-evil-test-created-buffer
        (let ((inhibit-read-only t))
          (erase-buffer)
          (insert-text-button "Project"
                              :state 'root-node-open
                              :path "/project"
                              :depth 0)
          (insert "\n")
          (insert-text-button "src"
                              :state 'dir-node-open
                              :path "/project/src"
                              :depth 1)
          (insert "\n")
          (insert-text-button "main.el"
                              :state 'file-node-closed
                              :path "/project/src/main.el"
                              :depth 2)
          (insert "\n"))
        (goto-char (point-min))
        (evil-local-mode 1)
        (evil-change-to-initial-state)))
    neomacs-treemacs-evil-test-created-buffer))

(defun neomacs-treemacs-evil-test-kill (buffer)
  "Kill BUFFER without modification prompts."
  (when (buffer-live-p buffer)
    (with-current-buffer buffer (set-buffer-modified-p nil))
    (kill-buffer buffer)))
"##;

fn treemacs_evil_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TREEMACS_EVIL_MELPA_PIN, "treemacs-evil.el")
        .expect("prepare exact shallow Treemacs Evil source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare exact shallow Evil dependency below ./tmp")
        .with_melpa_dependency(TREEMACS_MELPA_PIN)
        .expect("prepare exact shallow Treemacs dependency below ./tmp")
        .with_prelude(TREEMACS_EVIL_TEST_PRELUDE)
        .with_timeout(TREEMACS_EVIL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Treemacs Evil parity test")
        .into()
}

fn assert_treemacs_evil_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        treemacs_evil_oracle(),
        &current_test_name(),
        "treemacs_evil_parity",
        cases,
    );
}

#[test]
fn treemacs_evil_package_batch() {
    assert_treemacs_evil_batch(&workflows::workflow_batch_cases());
}
