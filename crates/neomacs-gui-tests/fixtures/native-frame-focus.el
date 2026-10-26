;;; native-frame-focus.el --- Observe native frame input routing -*- lexical-binding: t -*-

;; Shared with GNU Emacs: input and focus come from the native backend, not
;; execute-kbd-macro, unread-command-events, or a test-only input API.
(require 'json)
(setq inhibit-startup-screen t
      initial-scratch-message nil)
(menu-bar-mode -1)
(tool-bar-mode -1)

(defvar neomacs-focus-directory (getenv "NEOMACS_GUI_FOCUS_CONTROL"))
(defvar neomacs-focus-commands nil)

(defun neomacs-focus-observe ()
  (let ((state
         `((pid . ,(emacs-pid))
           (primary . ,(with-current-buffer "*focus-primary*" (buffer-string)))
           (secondary . ,(with-current-buffer "*focus-secondary*" (buffer-string)))
           (selected . ,(frame-parameter (selected-frame) 'title))
           (buffer . ,(buffer-name))
           (commands . ,(vconcat (reverse neomacs-focus-commands))))))
    (with-temp-file (expand-file-name "state.pending" neomacs-focus-directory)
      (insert (json-encode state)))
    (rename-file (expand-file-name "state.pending" neomacs-focus-directory)
                 (expand-file-name "state.json" neomacs-focus-directory) t)))

(defun neomacs-focus-command ()
  (push (format "%S@%s" this-command
                (frame-parameter (selected-frame) 'title))
        neomacs-focus-commands))

(defun neomacs-focus-created (frame)
  ;; Do not select FRAME here: that would mask the focus/selection bug.
  (modify-frame-parameters frame '((title . "NEOMACS-FOCUS-SECONDARY")))
  (set-window-buffer (frame-selected-window frame) "*focus-secondary*"))

(defun neomacs-focus-tick ()
  (condition-case err
      (progn
        (neomacs-focus-observe)
        (when (file-exists-p (expand-file-name "stop" neomacs-focus-directory))
          (when (fboundp 'neomacs--write-frame-snapshot)
            (neomacs--write-frame-snapshot
             (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json))
          (kill-emacs 0)))
    (error (message "Native frame focus fixture failed: %S" err) (kill-emacs 1))))

(defun neomacs-focus-start ()
  (modify-frame-parameters nil '((title . "NEOMACS-FOCUS-PRIMARY")))
  (dolist (name '("*focus-primary*" "*focus-secondary*"))
    (with-current-buffer (get-buffer-create name)
      (fundamental-mode)
      (erase-buffer)))
  (switch-to-buffer "*focus-primary*")
  (add-hook 'after-make-frame-functions #'neomacs-focus-created)
  (add-hook 'post-command-hook #'neomacs-focus-command)
  (run-at-time 0 0.05 #'neomacs-focus-tick))

(run-at-time 0.2 nil #'neomacs-focus-start)
