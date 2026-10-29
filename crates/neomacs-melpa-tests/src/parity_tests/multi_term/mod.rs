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
  "Write a deterministic terminal program to PROGRAM.
The driver owns its input dispositions: `stty -isig' makes the VQUIT
byte (0x1c) that `term-quit-subjob' writes arrive as ordinary input,
which the driver logs; SIGHUP from `kill-buffer' is then its only
terminator, so the exit status is deterministic on every host."
  (with-temp-file program
    (insert "#!/bin/sh\n"
            "stty -echo -icanon -isig min 1 time 0\n"
            "log=${NEOMACS_MULTI_TERM_QUIT_LOG:-quit-request.log}\n"
            "printf 'READY\\n'\n"
            "line=\n"
            "while :; do\n"
            "  byte=$(dd bs=1 count=1 2>/dev/null | od -An -tu1 | tr -d ' \\n')\n"
            "  if [ -z \"$byte\" ]; then break; fi\n"
            "  if [ \"$byte\" = 28 ]; then\n"
            "    printf 'QUIT-REQUEST\\n' >> \"$log\"\n"
            "    continue\n"
            "  fi\n"
            "  if [ \"$byte\" = 13 ] || [ \"$byte\" = 10 ]; then\n"
            "    case \"$line\" in\n"
            "      quit) printf 'BYE\\n'; exit 0 ;;\n"
            "      '') : ;;\n"
            "      *) printf 'ECHO:%s\\n' \"$line\" ;;\n"
            "    esac\n"
            "    line=\n"
            "    continue\n"
            "  fi\n"
            "  line=$line$(printf \"\\\\$(printf '%03o' \"$byte\")\")\n"
            "done\n"))
  (set-file-modes program #o755))

(defvar neomacs-multi-term-test-quit-log nil
  "File the sandbox terminal driver logs a delivered VQUIT byte to.")

(defvar neomacs-multi-term-test-quit-witnessed nil
  "Non-nil once the terminal driver logged the VQUIT byte of a buffer kill.")

(defun neomacs-multi-term-test-quit-witness ()
  "Wait for the terminal driver to log the VQUIT byte.
This runs from the victim buffer's `kill-buffer-hook' after Multi-term's
own hook has typed the quit character into the pty, and before
`kill_buffer_processes' sends SIGHUP (GNU runs `kill-buffer-hook' first,
src/buffer.c:1986 vs :2073).  The driver runs with `stty -isig', so the
byte arrives as data; the log therefore proves the quit request reached
the pty before the hangup."
  (let ((deadline (+ (float-time) 5.0)))
    (while (and neomacs-multi-term-test-quit-log
                (not (file-exists-p neomacs-multi-term-test-quit-log))
                (< (float-time) deadline))
      (sleep-for 0.01))
    (setq neomacs-multi-term-test-quit-witnessed
          (and neomacs-multi-term-test-quit-log
               (file-exists-p neomacs-multi-term-test-quit-log)
               (with-temp-buffer
                 (insert-file-contents neomacs-multi-term-test-quit-log)
                 (goto-char (point-min))
                 (search-forward "QUIT-REQUEST" nil t))
               t))))

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
          (default-directory root)
          (neomacs-multi-term-test-quit-log
           (expand-file-name "quit-request.log" root))
          (process-environment
           (cons (concat "NEOMACS_MULTI_TERM_QUIT_LOG="
                         neomacs-multi-term-test-quit-log)
                 process-environment)))
     (make-directory root t)
     (ignore-errors (delete-file neomacs-multi-term-test-quit-log))
     (setq neomacs-multi-term-test-quit-witnessed nil)
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
