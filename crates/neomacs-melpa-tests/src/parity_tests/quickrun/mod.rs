use std::time::Duration;

use crate::{CachedMelpaOracle, QUICKRUN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'quickrun)

(defvar neomacs-quickrun-test-hook-count 0)
(defvar quickrun-test-value nil)
(defun neomacs-quickrun-test-after-run ()
  "Record one successful Quickrun completion."
  (setq neomacs-quickrun-test-hook-count (1+ neomacs-quickrun-test-hook-count)))

(defun neomacs-quickrun-test-wait (&optional expect-hook)
  "Wait for Quickrun's process and EXPECT-HOOK successful completions."
  (let ((limit 500))
    (while (and (> limit 0)
                (or (and (get-buffer quickrun--buffer-name)
                         (get-buffer-process quickrun--buffer-name))
                    (and expect-hook
                         (< neomacs-quickrun-test-hook-count expect-hook))))
      (accept-process-output nil 0.02)
      (setq limit (1- limit)))
    (when (or (and (get-buffer quickrun--buffer-name)
                   (get-buffer-process quickrun--buffer-name))
              (and expect-hook (< neomacs-quickrun-test-hook-count expect-hook)))
      (error "Quickrun timed out"))))

(defun neomacs-quickrun-test-reset ()
  "Reset global Quickrun process and output state."
  (when (get-buffer quickrun--buffer-name)
    (let ((process (get-buffer-process quickrun--buffer-name)))
      (when process (set-process-query-on-exit-flag process nil)))
    (kill-buffer quickrun--buffer-name))
  (setq quickrun--remove-files nil
        quickrun--timeout-timer nil
        neomacs-quickrun-test-hook-count 0))

(defun neomacs-quickrun-test-output ()
  "Return stable Quickrun buffer state."
  (when (get-buffer quickrun--buffer-name)
    (with-current-buffer quickrun--buffer-name
      (list :text (buffer-substring-no-properties (point-min) (point-max))
            :mode major-mode :read-only buffer-read-only
            :truncate truncate-lines
            :process (and (get-buffer-process (current-buffer)) t)))))

(defmacro neomacs-quickrun-test-with-buffer (name text &rest body)
  "Run BODY in a visible temporary buffer NAME containing TEXT."
  (declare (indent 2) (debug t))
  `(let ((buffer (generate-new-buffer ,name)))
     (unwind-protect
         (save-window-excursion
           (set-window-buffer (selected-window) buffer)
           (with-current-buffer buffer
             (insert ,text)
             (setq buffer-file-coding-system 'utf-8-unix)
             ,@body))
       (neomacs-quickrun-test-reset)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(QUICKRUN_MELPA_PIN, "quickrun.el")
        .expect("prepare exact shallow Quickrun source and dependencies below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn quickrun_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "quickrun_package_batch",
        "quickrun_parity",
        &workflows::workflow_batch_cases(),
    );
}
