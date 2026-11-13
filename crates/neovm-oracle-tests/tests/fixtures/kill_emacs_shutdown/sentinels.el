;;; -*- lexical-binding: t; -*-
(setq probe-log (getenv "SHUTDOWN_LOG"))
(setq probe-auto (getenv "SHUTDOWN_AUTOSAVE"))
(defun probe-record (line)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region (concat line "\n") nil probe-log t 'silent)))
(defun probe-sentinel (process _message)
  (probe-record (concat "sentinel:" (process-name process))))
(defun probe-handler (operation &rest args)
  (if (eq operation 'write-region)
      (progn
        (probe-record "shutdown-save")
        (let ((process (make-pipe-process :name "shutdown-new"
                                          :noquery t
                                          :sentinel #'probe-sentinel)))
          (probe-record (if (process-live-p process) "created-live" "created-dead"))
          (delete-process process))
        (probe-record "after-delete")
        nil)
    (let ((inhibit-file-name-handlers (cons 'probe-handler inhibit-file-name-handlers))
          (inhibit-file-name-operation operation))
      (apply operation args))))
(let ((process (make-pipe-process :name "control" :noquery t
                                  :sentinel #'probe-sentinel)))
  (delete-process process))
(setq file-name-handler-alist (list (cons (concat "\\`" (regexp-quote probe-auto) "\\'") 'probe-handler)))
(setq auto-save-list-file-name nil)
(setq kill-emacs-hook
      (list (lambda ()
              (probe-record "kill-hook")
              (let ((process (make-pipe-process :name "hook-control" :noquery t
                                                :sentinel #'probe-sentinel)))
                (delete-process process)))))
(set-buffer (get-buffer-create "shutdown-sentinel-buffer"))
(setq buffer-auto-save-file-name probe-auto)
(insert "modified")
(kill-emacs 0)
