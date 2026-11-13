;;; -*- lexical-binding: t; -*-
(setq probe-log (getenv "SHUTDOWN_LOG"))
(defun probe-record (line)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region (concat line "\n") nil probe-log t 'silent)))
(setq auto-save-list-file-name nil)
(setq kill-emacs-hook
      (list (lambda () (probe-record "HOOK-THROW") (throw 'probe-stop 'thrown))))
(probe-record (format "RETURN=%S" (catch 'probe-stop (kill-emacs 3))))
(probe-record "STILL-ALIVE")
(setq kill-emacs-hook (list (lambda () (probe-record "SECOND-KILL-HOOK"))))
(kill-emacs 0)
