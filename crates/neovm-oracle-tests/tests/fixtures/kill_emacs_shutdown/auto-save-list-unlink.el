;;; -*- lexical-binding: t; -*-
(setq auto-save-list-file-name (getenv "SHUTDOWN_ARTIFACT"))
(princ (format "before-kill auto-save-list=%S exists=%S\n"
               auto-save-list-file-name
               (file-exists-p auto-save-list-file-name)))
(write-region "ready\n" nil (getenv "SHUTDOWN_READY") nil 'silent)
(while (not (file-exists-p (getenv "SHUTDOWN_GO")))
  (sleep-for 0.01))
(kill-emacs 0)
