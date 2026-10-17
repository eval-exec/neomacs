;;; prefix-help.el --- Prefix help uses ordinary command dispatch -*- lexical-binding: t; -*-
(defvar issue576-trace nil)
(defun issue576-record (stage)
  "Record STAGE, command keys, and their shift-translation state."
  (when (eq this-command 'describe-prefix-bindings)
    (push (list stage this-command (append (this-command-keys) nil)
                this-command-keys-shift-translated) issue576-trace)))
(defun issue576-pre () "Record prefix help before dispatch." (issue576-record 'pre))
(defun issue576-post () "Record prefix help after dispatch." (issue576-record 'post))
(add-hook 'pre-command-hook #'issue576-pre)
(add-hook 'post-command-hook #'issue576-post)

(defun issue576-finish ()
  "Present the command trace after prefix help has completed."
  (interactive)
  (let ((help-visible (get-buffer-window "*Help*")))
    (remove-hook 'pre-command-hook #'issue576-pre)
    (remove-hook 'post-command-hook #'issue576-post)
    (switch-to-buffer (get-buffer-create "*prefix-help-result*"))
    (delete-other-windows)
    (insert (if help-visible "prefix-help-session-alive\n" "prefix-help-missing\n"))
    (dolist (entry (nreverse issue576-trace))
      (insert (format "%S\n" entry)))
    (put-text-property (point-min) (point-max) 'face '(:background "#00ff00"))
    (goto-char (point-min))
    (redisplay t)
    (run-at-time 0.5 nil
      (lambda ()
        (neomacs--write-frame-snapshot (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT") t 'text-faces)))
    (run-at-time 2 nil (lambda () (kill-emacs 0)))))
(global-set-key [f12] #'issue576-finish)
;; Queue ordinary command-loop input, rather than using a nested keyboard
;; macro whose end-of-input handling could hide the erroneous early return.
(if (getenv "NEOMACS_PREFIX_HELP_SHIFTED")
    (progn
      (global-set-key [f5] (make-sparse-keymap))
      (run-at-time 0.5 nil (lambda () (setq unread-command-events '(S-f5 8 f12)))))
  (run-at-time 0.5 nil (lambda () (setq unread-command-events '(3 8 f12)))))
