use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_MELPA_PIN, EVIL_TEXTOBJ_LINE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EVIL_TEXTOBJ_LINE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const EVIL_TEXTOBJ_LINE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-textobj-line)

(defun neomacs-evil-textobj-line-test-with-buffer (text needle function)
  "Run FUNCTION in a live Evil buffer containing TEXT at NEEDLE."
  (let ((buffer (generate-new-buffer " *evil-textobj-line-workflow*")))
    (unwind-protect
        (progn
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (text-mode)
          (insert text)
          (goto-char (point-min))
          (when needle
            (search-forward needle)
            (goto-char (match-beginning 0)))
          (evil-local-mode 1)
          (evil-normal-state)
          (funcall function))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when (bound-and-true-p evil-local-mode)
            (evil-local-mode -1)))
        (kill-buffer buffer)))))

(defun neomacs-evil-textobj-line-test-range (command)
  "Describe COMMAND's raw Evil range and selected text."
  (save-excursion
    (let ((range (funcall command nil nil nil nil)))
      (list command
            :begin (evil-range-beginning range)
            :end (evil-range-end range)
            :type (evil-type range)
            :text (buffer-substring-no-properties
                   (evil-range-beginning range)
                   (evil-range-end range))))))
"####;

fn evil_textobj_line_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_TEXTOBJ_LINE_MELPA_PIN, "evil-textobj-line.el")
        .expect("prepare exact shallow evil-textobj-line source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare exact shallow Evil dependency below ./tmp")
        .with_prelude(EVIL_TEXTOBJ_LINE_TEST_PRELUDE)
        .with_timeout(EVIL_TEXTOBJ_LINE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed evil-textobj-line parity test")
        .into()
}

fn assert_evil_textobj_line_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        evil_textobj_line_oracle(),
        &current_test_name(),
        "evil_textobj_line_parity",
        cases,
    );
}

#[test]
fn evil_textobj_line_package_batch() {
    assert_evil_textobj_line_batch(&workflows::workflow_batch_cases());
}
