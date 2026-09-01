use std::time::Duration;

use crate::{AUTO_SAVE_VISITED_LOCAL_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUTO_SAVE_VISITED_LOCAL_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const AUTO_SAVE_VISITED_LOCAL_MODE_TEST_PRELUDE: &str = r####"
(defvar neomacs-asvlm-test--events nil)
(defvar neomacs-asvlm-test--predicate-calls 0)
(defvar neomacs-asvlm-test--file-handler-events nil)

(defun neomacs-asvlm-test--write-file (path contents)
  "Create PATH's parent directories and write exact UTF-8 CONTENTS."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file path
      (insert contents))))

(defun neomacs-asvlm-test--file-text (path)
  "Read PATH as exact UTF-8 text."
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path))
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-asvlm-test--timer-state (buffer)
  "Describe BUFFER's package timer without unstable object identity."
  (if (not (buffer-live-p buffer))
      '(:buffer-live nil)
    (with-current-buffer buffer
      (let ((timer auto-save-visited-local--timer))
        (list
         :buffer-live t
         :present (and (timerp timer) t)
         :idle-seconds (and (timerp timer) (float-time (timer--time timer)))
         :repeat (and (timerp timer) (timer--repeat-delay timer))
         :function (and (timerp timer) (timer--function timer))
         :argument-is-buffer
         (and (timerp timer) (eq (car (timer--args timer)) buffer))
         :registered (and (timerp timer) (memq timer timer-idle-list) t))))))

(defun neomacs-asvlm-test--fire-buffer-timer (buffer)
  "Fire BUFFER's live package timer through Emacs's timer dispatcher."
  (when (buffer-live-p buffer)
    (let ((timer (buffer-local-value 'auto-save-visited-local--timer buffer)))
      (when (and (timerp timer) (memq timer timer-idle-list))
        (timer-event-handler timer)))))

(defun neomacs-asvlm-test--buffer-state (buffer)
  "Return BUFFER's exact user-visible and mode state."
  (if (not (buffer-live-p buffer))
      '(:live nil)
    (with-current-buffer buffer
      (list
       :live t
       :text (buffer-substring-no-properties (point-min) (point-max))
       :point (point)
       :modified (buffer-modified-p)
       :read-only buffer-read-only
       :mode (and (bound-and-true-p auto-save-visited-local-mode) t)
       :kill-hook
       (and
        (memq 'auto-save-visited-local--stop-timer kill-buffer-hook)
        t)
       :timer (neomacs-asvlm-test--timer-state buffer)))))

(defun neomacs-asvlm-test--messages (start)
  "Return exact non-empty message lines written after START."
  (with-current-buffer (messages-buffer)
    (split-string
     (buffer-substring-no-properties (min start (point-max)) (point-max))
     "\n" t)))

(defun neomacs-asvlm-test--before-save ()
  "Record the save environment seen by a practical before-save hook."
  (push
   (list :before (buffer-name) inhibit-message message-log-max)
   neomacs-asvlm-test--events))

(defun neomacs-asvlm-test--after-save ()
  "Record the save environment seen by a practical after-save hook."
  (push
   (list :after (buffer-name) inhibit-message message-log-max)
   neomacs-asvlm-test--events))

(defun neomacs-asvlm-test--ready-document-p ()
  "Accept a document only after its status is READY."
  (setq neomacs-asvlm-test--predicate-calls
        (1+ neomacs-asvlm-test--predicate-calls))
  (save-excursion
    (goto-char (point-min))
    (search-forward "status: READY" nil t)))

(defun neomacs-asvlm-test--reject-save ()
  "Model a project save integration rejecting invalid content."
  (error "formatter rejected save Ω"))

(defun neomacs-asvlm-test--remote-file-handler (operation &rest arguments)
  "Model the two file operations auto-save performs for a Tramp name."
  (push (cons operation arguments) neomacs-asvlm-test--file-handler-events)
  (cond
   ((eq operation 'file-writable-p) t)
   ((eq operation 'file-remote-p) "/neomacs-asvlm-remote:")
   (t
    (let ((inhibit-file-name-handlers
           (cons 'neomacs-asvlm-test--remote-file-handler
                 inhibit-file-name-handlers))
          (inhibit-file-name-operation operation))
      (apply operation arguments)))))

(defun neomacs-asvlm-test--cleanup (buffers root)
  "Kill BUFFERS without prompts and remove the deterministic ROOT."
  (dolist (buffer buffers)
    (when (buffer-live-p buffer)
      (let ((timer
             (and
              (boundp 'auto-save-visited-local--timer)
              (buffer-local-value
               'auto-save-visited-local--timer buffer))))
        (when (timerp timer)
          (cancel-timer timer)))))
  (dolist (timer (copy-sequence timer-idle-list))
    (when (eq (timer--function timer)
              'auto-save-visited-local--save-buffer-wrapper)
      (cancel-timer timer)))
  (dolist (buffer buffers)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (setq buffer-read-only nil)
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (when (and root (file-exists-p root))
    (delete-directory root t))
  (setq neomacs-asvlm-test--events nil
        neomacs-asvlm-test--predicate-calls 0
        neomacs-asvlm-test--file-handler-events nil))
"####;

fn auto_save_visited_local_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        AUTO_SAVE_VISITED_LOCAL_MODE_MELPA_PIN,
        "auto-save-visited-local-mode.el",
    )
    .expect("prepare pinned auto-save-visited-local-mode source below ./tmp")
    .with_prelude(AUTO_SAVE_VISITED_LOCAL_MODE_TEST_PRELUDE)
    .with_installed_autoloads()
    .with_timeout(AUTO_SAVE_VISITED_LOCAL_MODE_TEST_TIMEOUT)
}

#[test]
fn auto_save_visited_local_mode_package_batch() {
    assert_oracle_batch_cases(
        auto_save_visited_local_mode_oracle(),
        "auto_save_visited_local_mode_package_batch",
        "auto_save_visited_local_mode_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
