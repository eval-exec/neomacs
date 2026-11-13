;;; word-wrap-cursor.el --- Native cursor across word-wrap rollback -*- lexical-binding: t -*-

;; Shared with pinned GNU Emacs. The driver sends real desktop keys and reads
;; passive cursor state before any snapshot or explicit redisplay can repair it.
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(defvar wrap704-directory (getenv "NEOMACS_WORD_WRAP_CONTROL"))
(defvar wrap704-buffer nil)
(defvar wrap704-moves 0)

(defun wrap704-write (name value)
  (let ((pending (expand-file-name (concat name ".pending") wrap704-directory)))
    (with-temp-file pending (insert (json-encode value)))
    (rename-file pending (expand-file-name name wrap704-directory) t)))

(defun wrap704-after-command ()
  (when (memq this-command '(right-char left-char forward-char backward-char))
    (setq wrap704-moves (1+ wrap704-moves))))

(defun wrap704-tick ()
  (condition-case err
      (cond
       ((file-exists-p (expand-file-name "stop" wrap704-directory)) (kill-emacs 0))
       ((file-exists-p (expand-file-name "invalidate" wrap704-directory))
        (delete-file (expand-file-name "invalidate" wrap704-directory))
        (with-current-buffer wrap704-buffer
          ;; An unused property changes redisplay's cache key, without changing
          ;; text, point, faces or wrapping. Observe the subsequent normal frame.
          (put-text-property (point-min) (point-max) 'wrap704-unused t)
          (redisplay t))
        (wrap704-write "invalidated" t))
       ((file-exists-p (expand-file-name "request" wrap704-directory))
        (let* ((path (expand-file-name "request" wrap704-directory))
               (request (with-temp-buffer
                          (insert-file-contents-literally path)
                          (string-to-number (buffer-string)))))
          (delete-file path)
          (with-current-buffer wrap704-buffer
            (let ((cursor (window-cursor-info)))
              (wrap704-write
               "state.json"
               `((request . ,request) (pid . ,(emacs-pid))
                 (moves . ,wrap704-moves) (point . ,(point))
                 (cursor . ,cursor) (modified_tick . ,(buffer-modified-tick))
                 (chars_modified_tick . ,(buffer-chars-modified-tick))
                 (text . ,(buffer-substring-no-properties (point-min) (point-max)))
                 (char_width . ,(frame-char-width)) (char_height . ,(frame-char-height))
                 (inside_edges . ,(vconcat (window-inside-pixel-edges))))))))))
    (error (wrap704-write "error.json" (format "%S" err)) (kill-emacs 1))))

(defun wrap704-start ()
  (condition-case err
      (progn
        (set-frame-font "DejaVu Sans Mono-14" nil t)
        (set-window-fringes nil 0 0)
        (set-frame-size nil 30 20)
        (setq wrap704-buffer (get-buffer-create "*word-wrap-cursor*"))
        (switch-to-buffer wrap704-buffer)
        (pcase (getenv "NEOMACS_WORD_WRAP_MODE")
          ("fundamental" (fundamental-mode))
          ("org" (org-mode))
          (mode (error "Unsupported word-wrap fixture mode: %S" mode)))
        (setq-local truncate-lines nil word-wrap t)
        (insert "aaaa bbbbbb\n")
        (goto-char (point-min))
        (set-window-start nil (point-min))
        (setq cursor-type 'box)
        (add-hook 'post-command-hook #'wrap704-after-command)
        (run-at-time 0.03 0.03 #'wrap704-tick))
    (error (wrap704-write "error.json" (format "%S" err)) (kill-emacs 1))))

(run-at-time 0.4 nil #'wrap704-start)
