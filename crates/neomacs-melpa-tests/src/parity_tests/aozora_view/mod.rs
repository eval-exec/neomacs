use std::time::Duration;

use crate::{AOZORA_VIEW_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AOZORA_VIEW_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AOZORA_VIEW_TEST_PRELUDE: &str = r##"
(defvar byte-compile-current-file nil)

;; aozora-view 20140310.1317 calls this Emacs 24 API while entering its
;; derived mode. Preserve that API's read-only-mode delegation so the tests
;; can exercise the viewer on current GNU Emacs and Neomacs.
(defun toggle-read-only (&optional argument interactive)
  (if interactive
      (call-interactively 'read-only-mode)
    (read-only-mode (or argument 'toggle))))

(defun neomacs-aozora-test-token-state (token)
  (save-excursion
    (goto-char (point-min))
    (search-forward token)
    (let ((position (match-beginning 0)))
      (list
       :text token
       :position position
       :line-number (get-text-property position 'line-number)
       :display (copy-tree
                 (get-text-property position 'display))
       :face (copy-tree
              (get-text-property position 'face))
       :read-only (get-text-property position 'read-only)))))

(defun neomacs-aozora-test-cleanup (root)
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer))
          (source
           (with-current-buffer buffer
             (and
              (boundp 'aozora-view-text-file)
              aozora-view-text-file))))
      (when
          (or
           (and file (string-prefix-p root file))
           (and source (string-prefix-p root source)))
        (with-current-buffer buffer
          (setq buffer-read-only nil)
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when
      (file-exists-p root)
    (delete-directory root t)))
"##;

fn aozora_view_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AOZORA_VIEW_MELPA_PIN, "aozora-view.el")
        .expect("prepare pinned aozora-view source below ./tmp")
        .with_prelude(AOZORA_VIEW_TEST_PRELUDE)
        .with_timeout(AOZORA_VIEW_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed aozora-view parity test")
        .into()
}

/// Multi-probe batch for `assert_aozora_view_parity` cases (2a).
pub(crate) fn assert_aozora_view_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aozora_view_oracle(), &name, "aozora_view_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aozora_view_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aozora_view_batch(&cases);
}

// END generated package batch tests
