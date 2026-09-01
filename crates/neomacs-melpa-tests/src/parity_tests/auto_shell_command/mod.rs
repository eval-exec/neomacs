use std::time::Duration;

use crate::{AUTO_SHELL_COMMAND_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUTO_SHELL_COMMAND_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const AUTO_SHELL_COMMAND_TEST_PRELUDE: &str = r####"
(defun neomacs-ascmd-test--load-package ()
  "Load the package through a documented autoloaded command."
  (unless (featurep 'auto-shell-command)
    (ascmd:process-count-clear)))

(defun neomacs-ascmd-test--write-file (path contents)
  "Create PATH's parent directories and write exact UTF-8 CONTENTS."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file path
      (insert contents))))

(defun neomacs-ascmd-test--write-program (path body)
  "Create executable shell program PATH with BODY."
  (neomacs-ascmd-test--write-file
   path
   (concat "#!/bin/sh\nset -eu\n" body))
  (set-file-modes path #o755))

(defun neomacs-ascmd-test--file-text (path)
  "Read PATH as exact UTF-8 text, or nil when it does not exist."
  (when (file-exists-p path)
    (with-temp-buffer
      (let ((coding-system-for-read 'utf-8-unix))
        (insert-file-contents path))
      (buffer-substring-no-properties (point-min) (point-max)))))

(defun neomacs-ascmd-test--buffer-text (buffer-or-name)
  "Read BUFFER-OR-NAME as exact text, or nil when it is absent."
  (let ((buffer (get-buffer buffer-or-name)))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (buffer-substring-no-properties (point-min) (point-max))))))

(defun neomacs-ascmd-test--deferred-processes ()
  "Return live child processes owned by emacs-deferred."
  (let (matches)
    (dolist (process (process-list) (nreverse matches))
      (when (and (process-live-p process)
                 (string-prefix-p "*deferred:*" (process-name process)))
        (setq matches (cons process matches))))))

(defun neomacs-ascmd-test--wait-until (predicate)
  "Wait boundedly for PREDICATE while dispatching processes and timers."
  (catch 'ready
    (dotimes (_ 500)
      (when (funcall predicate)
        (throw 'ready t))
      (accept-process-output nil 0.02))
    nil))

(defun neomacs-ascmd-test--idle-p ()
  "Return non-nil after the package queue and deferred children drain."
  (and (null ascmd:process-queue)
       (null (neomacs-ascmd-test--deferred-processes))))

(defun neomacs-ascmd-test--wait-for-idle ()
  "Wait for all package work and signal a useful timeout on failure."
  (unless (neomacs-ascmd-test--wait-until #'neomacs-ascmd-test--idle-p)
    (error "auto-shell-command did not become idle: queue=%S processes=%S"
           ascmd:process-queue
           (mapcar #'process-name
                   (neomacs-ascmd-test--deferred-processes)))))

(defun neomacs-ascmd-test--wait-for-file (path)
  "Wait until PATH exists and signal a useful timeout on failure."
  (unless (neomacs-ascmd-test--wait-until
           (lambda () (file-exists-p path)))
    (error "timed out waiting for %s" path)))

(defun neomacs-ascmd-test--messages (start)
  "Return exact non-empty message lines written after START."
  (with-current-buffer (messages-buffer)
    (split-string
     (buffer-substring-no-properties (min start (point-max)) (point-max))
     "\n" t)))

(defun neomacs-ascmd-test--cleanup (buffers root)
  "Stop package work, kill BUFFERS, and remove deterministic ROOT."
  (dolist (process (neomacs-ascmd-test--deferred-processes))
    (ignore-errors (set-process-sentinel process nil))
    (ignore-errors (delete-process process)))
  (when (boundp 'deferred:queue)
    (setq deferred:queue nil))
  (dolist (timer (copy-sequence timer-list))
    (when (eq (timer--function timer) 'deferred:worker)
      (cancel-timer timer)))
  (when (boundp 'ascmd:process-queue)
    (setq ascmd:process-queue nil))
  (dolist (buffer buffers)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (setq buffer-read-only nil)
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (dolist (buffer (buffer-list))
    (when (or (string-prefix-p " *deferred:*" (buffer-name buffer))
              (string-equal (buffer-name buffer)
                            "*Auto Shell Command*"))
      (kill-buffer buffer)))
  (when (and root (file-exists-p root))
    (delete-directory root t)))
"####;

fn auto_shell_command_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_SHELL_COMMAND_MELPA_PIN, "auto-shell-command.el")
        .expect("prepare pinned auto-shell-command source below ./tmp")
        .with_prelude(AUTO_SHELL_COMMAND_TEST_PRELUDE)
        .with_installed_autoloads()
        .with_timeout(AUTO_SHELL_COMMAND_TEST_TIMEOUT)
}

#[test]
fn auto_shell_command_package_batch() {
    assert_oracle_batch_cases(
        auto_shell_command_oracle(),
        "auto_shell_command_package_batch",
        "auto_shell_command_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
