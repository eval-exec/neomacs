;;; -*- lexical-binding: t; -*-
(setq probe-log (getenv "SHUTDOWN_LOG"))
(defun probe-record (line)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region (concat line "\n") nil probe-log t 'silent)))
(setq auto-save-list-file-name nil)
(setq probe-original-hook-symbol 'kill-emacs-hook)
(set probe-original-hook-symbol
     (list (lambda () (probe-record "ORIGINAL-HOOK"))))
(unintern probe-original-hook-symbol obarray)
(set (intern "kill-emacs-hook") nil)
(probe-record (format "SETUP original=%S replacement=%S distinct=%S"
                      (and (symbol-value probe-original-hook-symbol) t)
                      (symbol-value (intern "kill-emacs-hook"))
                      (not (eq probe-original-hook-symbol (intern "kill-emacs-hook")))))
(kill-emacs 0)
