;;; -*- lexical-binding: t; -*-
;; Keep the message on one line: GNU deliberately throws to top-level after
;; a nested complete redisplay from dsafe__call. This row isolates the ordinary
;; message/preparation result while the outer window-end iterator survives.
(setq inhibit-startup-screen t)
(defvar qf-once nil)
(defvar qf-calls 0)
(defvar qf-message 'not-called)
(defvar qf-query-error nil)
(defvar qf-end nil)
(defvar qf-stage 'loaded)

(defun qf-write (result)
  (with-temp-file (getenv "NEOMACS_MINI_QUERY_ORACLE_RESULT")
    (let ((print-length nil) (print-level nil))
      (prin1 result (current-buffer))
      (insert "\n"))))

(defun qf-probe ()
  (condition-case outer
      (progn
        (switch-to-buffer (get-buffer-create " *mini-query-flow*"))
        (fundamental-mode)
        (setq-local fontification-functions nil)
        (setq-local truncate-lines t)
        (erase-buffer)
        (dotimes (_ 80) (insert "fontification query source line\n"))
        (goto-char (point-min))
        (redisplay t)
        (setq qf-once nil qf-calls 0 qf-message 'not-called qf-query-error nil)
        (setq-local fontification-functions
                    (list (lambda (_start)
                            (setq qf-calls (1+ qf-calls))
                            (unless qf-once
                              (setq qf-once t)
                              (put-text-property (point-min) (point-max) 'fontified t)
                              (let ((inhibit-redisplay nil))
                                (setq qf-message
                                      (condition-case err
                                          (list 'ok (message "query fontify"))
                                        (error (list 'error (error-message-string err))))))))))
        (goto-char (point-max))
        (insert "stale query\n")
        (goto-char (point-min))
        (put-text-property (point-min) (point-max) 'fontified nil)
        (setq qf-stage 'querying)
        (setq qf-end
              (condition-case err
                  (window-end nil t)
                (error (setq qf-query-error (error-message-string err)) nil)))
        (qf-write (list qf-once (> qf-calls 0) qf-message
                        (integerp qf-end) qf-query-error (current-message)))
        (kill-emacs 0))
    (error
     (qf-write (list 'fatal (error-message-string outer)))
     (kill-emacs 2))))

(run-at-time 1 nil #'qf-probe)
(run-at-time 20 nil
             (lambda ()
               (qf-write (list 'timeout qf-stage qf-calls qf-once qf-message))
               (kill-emacs 3)))
