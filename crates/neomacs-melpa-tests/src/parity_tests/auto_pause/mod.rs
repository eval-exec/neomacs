use std::time::Duration;

use crate::{AUTO_PAUSE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUTO_PAUSE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const AUTO_PAUSE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defvar neomacs-auto-pause-test--events nil)

(defun neomacs-auto-pause-test--symbol-role (symbol)
  "Describe package-generated SYMBOL without retaining its gensym suffix."
  (let ((name (and (symbolp symbol) (symbol-name symbol))))
    (cond
     ((and name (string-prefix-p "auto-pause-pause-" name)) 'pause)
     ((and name (string-prefix-p "auto-pause-resume-" name)) 'resume)
     ((and name (string-prefix-p "auto-pause-abort-" name)) 'abort)
     ((and name (string-prefix-p "auto-pause-idle-timer-" name)) 'idle-timer)
     (t symbol))))

(defun neomacs-auto-pause-test--normalize-text (text)
  "Normalize only auto-pause's generated symbol suffixes in TEXT."
  (let ((normalized text))
    (dolist (kind '("pause" "resume" "abort" "idle-timer") normalized)
      (setq normalized
            (replace-regexp-in-string
             (concat "auto-pause-" kind "-[0-9]+")
             (concat "auto-pause-" kind "-<id>")
             normalized t t)))))

(defun neomacs-auto-pause-test--messages (start)
  "Return exact package messages after START, with gensyms normalized."
  (with-current-buffer (messages-buffer)
    (let ((text
           (buffer-substring-no-properties
            (min start (point-max)) (point-max))))
      (split-string
       (string-trim
        (neomacs-auto-pause-test--normalize-text text))
       "\n" t))))

(defun neomacs-auto-pause-test--hook-count (function hook)
  "Count entries exactly equal to FUNCTION in HOOK."
  (let ((count 0))
    (dolist (entry hook count)
      (when (eq entry function)
        (setq count (1+ count))))))

(defun neomacs-auto-pause-test--package-advice-name-p (name)
  "Return non-nil when NAME is one of auto-pause's malformed names."
  (member name
          '(("auto-pause-advise-start-process")
            ("auto-pause-advise-set-process-sentinel"))))

(defun neomacs-auto-pause-test--advice-names (symbol)
  "Return auto-pause advice names still installed on SYMBOL."
  (let (names)
    (advice-mapc
     (lambda (_function properties)
       (let ((name (alist-get 'name properties)))
         (when (neomacs-auto-pause-test--package-advice-name-p name)
           (push name names))))
     symbol)
    (nreverse names)))

(defun neomacs-auto-pause-test--remove-package-advice ()
  "Remove auto-pause's retained advice functions by identity."
  (dolist (symbol '(start-process set-process-sentinel))
    (let (functions)
      (advice-mapc
       (lambda (function properties)
         (when (neomacs-auto-pause-test--package-advice-name-p
                (alist-get 'name properties))
           (push function functions)))
       symbol)
      (dolist (function functions)
        (advice-remove symbol function)))))

(defun neomacs-auto-pause-test--observer ()
  "Record execution of an unrelated pre-existing command hook."
  (push :observer neomacs-auto-pause-test--events))

(defun neomacs-auto-pause-test--sentinel (_process event)
  "Record a user sentinel EVENT."
  (push (list :user event) neomacs-auto-pause-test--events))

(defun neomacs-auto-pause-test--failing-sentinel (_process event)
  "Record EVENT and signal like a failing user sentinel."
  (push (list :failing-user event) neomacs-auto-pause-test--events)
  (error "user sentinel failed: %s" event))

(defun neomacs-auto-pause-test--pipe (name)
  "Create a deterministic local pipe process named NAME."
  (make-pipe-process :name name :noquery t :buffer nil))

(defun neomacs-auto-pause-test--delete-process (process)
  "Delete PROCESS without invoking a package sentinel during cleanup."
  (when (processp process)
    (ignore-errors (set-process-sentinel process nil))
    (ignore-errors (delete-process process))))

(defun neomacs-auto-pause-test--write-worker (root)
  "Write the deterministic pause/resume subprocess below ROOT."
  (let ((program (expand-file-name "worker.sh" root)))
    (make-directory root t)
    (with-temp-file program
      (insert
       "#!/bin/sh\n"
       "printf ready > \"$1\"\n"
       "while [ ! -f \"$2\" ]; do sleep 0.02; done\n"
       "printf 'payload Ω\\n'\n"
       "exit 7\n"))
    (set-file-modes program #o755)
    program))

(defun neomacs-auto-pause-test--wait-file (file process)
  "Wait boundedly for FILE while servicing PROCESS."
  (catch 'ready
    (dotimes (_ 200)
      (when (file-exists-p file)
        (throw 'ready t))
      (accept-process-output process 0.02))
    nil))

(defun neomacs-auto-pause-test--wait-status (process status)
  "Wait boundedly until PROCESS reaches STATUS."
  (catch 'ready
    (dotimes (_ 100)
      (accept-process-output nil 0.02)
      (when (eq (process-status process) status)
        (throw 'ready t)))
    nil))

(defun neomacs-auto-pause-test--buffer-text (buffer)
  "Return BUFFER's exact text."
  (with-current-buffer buffer
    (buffer-substring-no-properties (point-min) (point-max))))
"####;

fn auto_pause_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_PAUSE_MELPA_PIN, "auto-pause.el")
        .expect("prepare pinned auto-pause source below ./tmp")
        .with_prelude(AUTO_PAUSE_TEST_PRELUDE)
        .with_timeout(AUTO_PAUSE_TEST_TIMEOUT)
}

#[test]
fn auto_pause_package_batch() {
    assert_oracle_batch_cases(
        auto_pause_oracle(),
        "auto_pause_package_batch",
        "auto_pause_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
