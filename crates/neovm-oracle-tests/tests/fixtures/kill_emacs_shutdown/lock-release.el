;;; -*- lexical-binding: t; -*-
(setq create-lockfiles t)
(find-file (getenv "SHUTDOWN_TARGET"))
(insert "modified before deliberate exit\n")
(lock-buffer)
(princ (format "before-kill modified=%S truename=%S lock=%S\n"
               (buffer-modified-p) buffer-file-truename
               (file-symlink-p (getenv "SHUTDOWN_ARTIFACT"))))
(write-region "ready\n" nil (getenv "SHUTDOWN_READY") nil 'silent)
(while (not (file-exists-p (getenv "SHUTDOWN_GO")))
  (sleep-for 0.01))
(kill-emacs 0)
