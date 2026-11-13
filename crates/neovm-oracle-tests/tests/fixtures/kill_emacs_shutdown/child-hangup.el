;;; -*- lexical-binding: t; -*-
;; Run in a fresh supervised editor, with a private writable directory.
(setq shutdown-probe-root (getenv "SHUTDOWN_PROBE_ROOT"))
(setq shutdown-probe-events (expand-file-name "events.log" shutdown-probe-root))
(setq shutdown-probe-asv (expand-file-name "handled.auto-save" shutdown-probe-root))
(setq shutdown-probe-save-count 0)
(setq auto-save-list-file-name nil)

(defun shutdown-probe-event (text)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region (concat text "\n") nil shutdown-probe-events t 'silent)))

(defun shutdown-probe-hup-state ()
  ;; Linux queues a blocked SIGHUP synchronously in kill(2).  Reading these
  ;; pending bits does not depend on when the child next gets CPU time.
  (let ((status (with-temp-buffer
                  (insert-file-contents
                   (format "/proc/%d/status" (process-id shutdown-probe-child)))
                  (buffer-string)))
        (blocked 0)
        (pending 0))
    (when (string-match "^SigBlk:[ \t]+\\([0-9a-f]+\\)" status)
      (setq blocked (string-to-number (match-string 1 status) 16)))
    (dolist (name '("SigPnd" "ShdPnd"))
      (when (string-match (concat "^" name ":[ \t]+\\([0-9a-f]+\\)") status)
        (setq pending (logior pending (string-to-number (match-string 1 status) 16)))))
    (list (/= 0 (logand 1 blocked)) (/= 0 (logand 1 pending)))))

(defun shutdown-probe-auto-save-handler (operation &rest args)
  (if (eq operation 'write-region)
      (progn
        (setq shutdown-probe-save-count (1+ shutdown-probe-save-count))
        (shutdown-probe-event
         (format "AUTOSAVE-%d=%S" shutdown-probe-save-count
                 (shutdown-probe-hup-state)))
        nil)
    (let ((inhibit-file-name-handlers
           (cons 'shutdown-probe-auto-save-handler inhibit-file-name-handlers))
          (inhibit-file-name-operation operation))
      (apply operation args))))

(setq file-name-handler-alist
      (cons (cons (concat "\\`" (regexp-quote shutdown-probe-asv) "\\'")
                  'shutdown-probe-auto-save-handler)
            file-name-handler-alist))
(setq shutdown-probe-child
      (make-process :name "shutdown-hup-probe" :buffer nil :noquery t
                    :connection-type 'pipe
                    :command (list (getenv "SHUTDOWN_CHILD_PROGRAM")
                                   (getenv "SHUTDOWN_CHILD_HELPER")
                                   shutdown-probe-root)))
(let ((deadline (+ (float-time) 5.0)))
  (while (and (not (file-exists-p
                   (expand-file-name "child.ready" shutdown-probe-root)))
              (< (float-time) deadline))
    (accept-process-output shutdown-probe-child 0.01)))
(unless (file-exists-p (expand-file-name "child.ready" shutdown-probe-root))
  (error "Child never became ready"))
(shutdown-probe-event "READY")
(setq shutdown-probe-buffer (generate-new-buffer "shutdown-hup-buffer"))
(with-current-buffer shutdown-probe-buffer
  (setq buffer-auto-save-file-name shutdown-probe-asv)
  (insert "first\n"))
;; Positive control: the handler executes, HUP is really blocked, and no HUP
;; is pending before deliberate shutdown.
(do-auto-save t)
(with-current-buffer shutdown-probe-buffer (insert "second\n"))
(setq kill-emacs-hook (list (lambda () (shutdown-probe-event "KILL-HOOK"))))
(shutdown-probe-event "BEFORE-KILL")
(kill-emacs 0)
