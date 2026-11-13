;;; -*- lexical-binding: t; -*-
(unless (and (daemonp) (not noninteractive))
  (error "Expected an interactive foreground daemon, not batch mode"))
(setq auto-save-list-file-name nil)
(setq daemon-user-socket
      (expand-file-name "user.sock" (getenv "SHUTDOWN_PROBE_SOCKET_ROOT")))
(setq daemon-user-listener
      (make-network-process :name "shutdown-user-unix-listener"
                            :family 'local :server t :noquery t
                            :service daemon-user-socket))
(unless (file-exists-p daemon-user-socket) (error "User listener path was not created"))
(write-region "user-unix-listener-created\n" nil (getenv "SHUTDOWN_LOG") nil 'silent)
(write-region "ready\n" nil (getenv "SHUTDOWN_READY") nil 'silent)
(let ((deadline (+ (float-time) 5.0)))
  (while (and (not (file-exists-p (getenv "SHUTDOWN_GO")))
              (< (float-time) deadline))
    (sleep-for 0.01)))
(unless (file-exists-p (getenv "SHUTDOWN_GO")) (error "No supervisor GO"))
(kill-emacs 0)
