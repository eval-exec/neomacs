use std::time::Duration;

use crate::{CachedMelpaOracle, LIVE_PY_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'live-py-mode)

(defun neomacs-live-py-test-fixture (name text)
  "Create a Python source file named NAME containing TEXT."
  (let* ((root (file-name-as-directory
                (expand-file-name
                 (concat "live-py-" name)
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (file (expand-file-name "package/module.py" root)))
    (when (file-exists-p root) (delete-directory root t))
    (make-directory (file-name-directory file) t)
    (with-temp-file file (insert text))
    (list :root root :file file)))

(defun neomacs-live-py-test-window-state (source trace)
  "Describe source and TRACE window alignment."
  (mapcar
   (lambda (window)
     (with-selected-window window
       (list :kind (cond ((eq (current-buffer) source) 'source)
                         ((eq (current-buffer) trace) 'trace)
                         (t (buffer-name)))
             :start-line (line-number-at-pos (window-start))
             :point-line (line-number-at-pos)
             :column (current-column)
             :hscroll (window-hscroll)
             :truncate truncate-lines)))
   (window-list)))

(defun neomacs-live-py-test-state ()
  "Describe the current Live Py source and trace state."
  (let ((source (current-buffer))
        (trace (get-buffer live-py-trace-name)))
    (list :mode live-py-mode
          :lighter live-py-lighter
          :dir live-py-dir
          :path live-py-path
          :version live-py-version
          :args live-py-args
          :driver live-py-driver
          :module live-py-module
          :trace-name (and live-py-trace-name
                           (replace-regexp-in-string
                            "_[[:alnum:]_-]+\\*$" "_*" live-py-trace-name))
          :trace (and trace (with-current-buffer trace (buffer-string)))
          :hooks (list (and (memq #'live-py-after-change-function
                                  after-change-functions) t)
                       (and (memq #'live-py-post-command-function
                                  post-command-hook) t)
                       (and (memq #'live-py-mode-off kill-buffer-hook) t))
          :windows (neomacs-live-py-test-window-state source trace)
          :source-truncate truncate-lines
          :trace-truncate (and trace
                               (buffer-local-value 'truncate-lines trace)))))

(defmacro neomacs-live-py-test-run (name text &rest body)
  "Run BODY in a visited visible Live Py fixture."
  (declare (indent 2) (debug t))
  `(let* ((fixture (neomacs-live-py-test-fixture ,name ,text))
          (buffer (find-file-noselect (plist-get fixture :file)))
          result)
     (unwind-protect
         (setq result
               (save-window-excursion
                 (delete-other-windows)
                 (set-window-buffer (selected-window) buffer)
                 (with-current-buffer buffer
                   (python-mode)
                   ;; User options are ordinary globals in this package.  Bind
                   ;; their defaults per workflow so a configured driver/path
                   ;; never leaks into the next batch case.
                   (let ((live-py-driver nil)
                         (live-py-dir nil)
                         (live-py-path nil)
                         (live-py-version nil)
                         (live-py-args ""))
                     (setq-local truncate-lines nil
                                 live-py-update-all-delay nil)
                     ,@body))))
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (when live-py-mode (live-py-mode -1))
           (set-buffer-modified-p nil))
         (kill-buffer buffer))
       (when (file-exists-p (plist-get fixture :root))
         (delete-directory (plist-get fixture :root) t)))
     result))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LIVE_PY_MODE_MELPA_PIN, "live-py-mode.el")
        .expect("prepare exact shallow Live Py Mode source and bundled Space Tracer below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn live_py_mode_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "live_py_mode_package_batch",
        "live_py_mode_parity",
        &workflows::workflow_batch_cases(),
    );
}
