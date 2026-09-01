use std::time::Duration;

use crate::{CENTERED_CURSOR_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const CENTERED_CURSOR_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const CENTERED_CURSOR_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'centered-cursor-mode)

(defun neomacs-ccm-test-with-window (name line-count workflow)
  "Run WORKFLOW in a selected real window displaying LINE-COUNT numbered lines."
  (let ((buffer (generate-new-buffer (format "ccm-%s.txt" name))))
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (switch-to-buffer buffer)
          (dotimes (index line-count)
            (insert (format "line-%03d payload for centered scrolling\n"
                            (1+ index))))
          (goto-char (point-min))
          (set-window-start (selected-window) (point-min))
          (redisplay t)
          (funcall workflow buffer))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-ccm-test-window-state (&optional window)
  "Return cursor and viewport state for live WINDOW."
  (let ((window (or window (selected-window))))
    (with-current-buffer (window-buffer window)
      (append
       (list
        :point (window-point window)
        :point-line (line-number-at-pos (window-point window))
        :start (window-start window)
        :start-line (line-number-at-pos (window-start window))
        :end (window-end window t)
        :end-line (line-number-at-pos (window-end window t))
        :body-height (window-body-height window)
        :text-height (window-text-height window))
       ;; The package API measures only `selected-window'.  Selecting another
       ;; window just to sample it would mutate the window-point behavior under
       ;; test, so make the ownership of this observation explicit.
       (when (eq window (selected-window))
         (list :selected-visible-lines (ccm-visible-text-lines)))))))

(defun neomacs-ccm-test-hook-member (function hook)
  "Return non-nil when FUNCTION is buffer-locally present in HOOK."
  (and (local-variable-p hook)
       (memq function (symbol-value hook))))

;; Centered Cursor Mode's first activation forces the graphical frame to
;; redisplay its mode line.  Warm that path before either a shared batch or an
;; isolated probe so live window geometry has the same baseline in both modes.
(neomacs-ccm-test-with-window
 "display-warmup" 3
 (lambda (_buffer)
   (let ((ccm-vpos-init 1)
         (ccm-step-delay 0))
     (centered-cursor-mode 1)
     (redisplay t)
     (centered-cursor-mode -1))))
"##;

fn centered_cursor_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CENTERED_CURSOR_MODE_MELPA_PIN, "centered-cursor-mode.el")
        .expect("prepare exact shallow Centered Cursor Mode source below ./tmp")
        .with_prelude(CENTERED_CURSOR_MODE_TEST_PRELUDE)
        .with_timeout(CENTERED_CURSOR_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Centered Cursor Mode parity test")
        .into()
}

fn assert_centered_cursor_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        centered_cursor_mode_oracle(),
        &current_test_name(),
        "centered_cursor_mode_parity",
        cases,
    );
}

#[test]
fn centered_cursor_mode_package_batch() {
    assert_centered_cursor_mode_batch(&workflows::workflow_batch_cases());
}
