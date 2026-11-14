;;; -*- lexical-binding: t; -*-
;; A default-disposition pipe child must die by the shutdown SIGHUP before
;; its stdin pipe closes, matching GNU Emacs 31.1 shut_down_emacs ->
;; kill_buffer_processes (src/emacs.c, src/process.c).
(setq shutdown-probe-root (getenv "SHUTDOWN_PROBE_ROOT"))
(setq auto-save-list-file-name nil)
(setq shutdown-probe-child
      (make-process :name "default-hup-child" :buffer nil :noquery t
                    :connection-type 'pipe
                    :command (list (getenv "SHUTDOWN_CHILD_PROGRAM")
                                   (getenv "SHUTDOWN_CHILD_HELPER")
                                   shutdown-probe-root)))
(let ((deadline (+ (float-time) 5.0)))
  (while (and (not (file-exists-p (expand-file-name "child.ready" shutdown-probe-root)))
              (< (float-time) deadline))
    (accept-process-output shutdown-probe-child 0.01)))
(unless (file-exists-p (expand-file-name "child.ready" shutdown-probe-root))
  (error "Child did not become ready"))
(let ((status (with-temp-buffer
                (insert-file-contents (format "/proc/%d/status" (process-id shutdown-probe-child)))
                (buffer-string)))
      (disposition 0))
  (dolist (field '("SigBlk" "SigIgn" "SigCgt"))
    (when (string-match (concat "^" field ":[ \t]+\\([0-9a-f]+\\)") status)
      (setq disposition (logior disposition
                                (string-to-number (match-string 1 status) 16)))))
  (unless (= 0 (logand 1 disposition))
    (error "Child must keep the default HUP disposition")))
(princ "DEFAULT-HUP=t\n")
(write-region "ready\n" nil (getenv "SHUTDOWN_READY") nil 'silent)
(let ((deadline (+ (float-time) 5.0)))
  (while (and (not (file-exists-p (getenv "SHUTDOWN_GO")))
              (< (float-time) deadline))
    (sleep-for 0.01)))
(unless (file-exists-p (getenv "SHUTDOWN_GO"))
  (error "Supervisor did not release GO"))
(kill-emacs 0)
