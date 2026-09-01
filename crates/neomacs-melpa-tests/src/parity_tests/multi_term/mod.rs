use std::time::Duration;

use crate::{CachedMelpaOracle, MULTI_TERM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const MULTI_TERM_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const MULTI_TERM_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'term)
(require 'multi-term)

(defun neomacs-multi-term-test-write-driver (program)
  "Write a deterministic terminal program to PROGRAM."
  (with-temp-file program
    (insert "#!/bin/sh\n"
            "stty -echo\n"
            "printf 'READY\\n'\n"
            "while IFS= read -r line; do\n"
            "  case \"$line\" in\n"
            "    quit) printf 'BYE\\n'; exit 0 ;;\n"
            "    *) printf 'ECHO:%s\\n' \"$line\" ;;\n"
            "  esac\n"
            "done\n"))
  (set-file-modes program #o755))

(defun neomacs-multi-term-test-buffer-text (buffer)
  "Return BUFFER's visible terminal text without properties."
  (with-current-buffer buffer
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-multi-term-test-wait-for-output (buffer regexp)
  "Wait until live terminal BUFFER contains REGEXP, then return its text."
  (let ((deadline (+ (float-time) 5.0))
        matched)
    (while (and (buffer-live-p buffer)
                (not (setq matched
                           (with-current-buffer buffer
                             (save-excursion
                               (goto-char (point-min))
                               (re-search-forward regexp nil t)))))
                (< (float-time) deadline))
      (let ((process (get-buffer-process buffer)))
        (when process
          (accept-process-output process 0.05))))
    (unless matched
      (error "Timed out waiting for %S in %S; saw %S"
             regexp
             (and (buffer-live-p buffer) (buffer-name buffer))
             (and (buffer-live-p buffer)
                  (neomacs-multi-term-test-buffer-text buffer))))
    (neomacs-multi-term-test-buffer-text buffer)))

(defun neomacs-multi-term-test-wait-until (predicate description)
  "Wait until PREDICATE succeeds or signal with DESCRIPTION."
  (let ((deadline (+ (float-time) 5.0)))
    (while (and (not (funcall predicate))
                (< (float-time) deadline))
      (accept-process-output nil 0.05))
    (unless (funcall predicate)
      (error "Timed out waiting for %s" description))))

(defmacro neomacs-multi-term-test-with-workspace (&rest body)
  "Run BODY with real Multi-term machinery and a sandbox terminal program."
  (declare (indent 0) (debug t))
  `(let* ((root (file-name-as-directory
                 (expand-file-name
                  "multi-term-workspace"
                  (or (getenv "NEOMACS_TEST_SANDBOX_ROOT")
                      (error "NEOMACS_TEST_SANDBOX_ROOT is required")))))
          (program (expand-file-name "multi-term-driver" root))
          (initial-buffers (buffer-list))
          (other-window-advice
           (ad-find-some-advice
            'other-window 'after 'multi-term-dedicated-other-window-advice))
          (other-window-advice-enabled
           (and other-window-advice (ad-advice-enabled other-window-advice)))
          (other-window-advice-active (ad-is-active 'other-window))
          (term-mode-hook (copy-sequence term-mode-hook))
          (kill-buffer-hook (copy-sequence kill-buffer-hook))
          (term-raw-map (copy-keymap term-raw-map))
          (multi-term-program program)
          (multi-term-program-switches nil)
          (multi-term-buffer-name "terminal")
          (multi-term-buffer-list nil)
          (multi-term-dedicated-buffer nil)
          (multi-term-dedicated-window nil)
          (multi-term-dedicated-close-buffer nil)
          (multi-term-try-create nil)
          (multi-term-switch-after-close 'NEXT)
          (multi-term-default-dir root)
          (multi-term-dedicated-window-height 8)
          (multi-term-dedicated-max-window-height 20)
          (multi-term-dedicated-skip-other-window-p nil)
          (multi-term-dedicated-select-after-open-p nil)
          (multi-term-dedicated-close-back-to-open-buffer-p nil)
          (default-directory root))
     (make-directory root t)
     (neomacs-multi-term-test-write-driver program)
     (unwind-protect
         (save-window-excursion
           (delete-other-windows)
           ,@body)
       (unwind-protect
           (dolist (buffer (buffer-list))
             (unless (memq buffer initial-buffers)
               (let ((process (get-buffer-process buffer)))
                 (when process
                   (set-process-query-on-exit-flag process nil)
                   (set-process-sentinel process #'ignore)
                   (when (process-live-p process)
                     (delete-process process))))
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer
                   (setq buffer-read-only nil)
                   (let ((kill-buffer-hook nil)
                         (kill-buffer-query-functions nil))
                     (set-buffer-modified-p nil)
                     (kill-buffer buffer))))))
         (if other-window-advice-enabled
             (ad-enable-advice
              'other-window 'after 'multi-term-dedicated-other-window-advice)
           (ad-disable-advice
            'other-window 'after 'multi-term-dedicated-other-window-advice))
         (if other-window-advice-active
             (ad-activate 'other-window)
           (ad-deactivate 'other-window))))))
"####;

fn multi_term_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MULTI_TERM_MELPA_PIN, "multi-term.el")
        .expect("prepare exact shallow multi-term source below ./tmp")
        .with_prelude(MULTI_TERM_TEST_PRELUDE)
        .with_timeout(MULTI_TERM_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed multi-term parity test")
        .into()
}

fn assert_multi_term_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        multi_term_oracle(),
        &current_test_name(),
        "multi_term_parity",
        cases,
    );
}

#[test]
fn multi_term_package_batch() {
    assert_multi_term_batch(&workflows::workflow_batch_cases());
}
