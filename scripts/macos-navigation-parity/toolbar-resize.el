;;; toolbar-resize.el --- Paired GUI implied-resize witness -*- lexical-binding: t; -*-

(require 'json)
(require 'tool-bar)
;; Isolate the toolbar consumer from the separately tested build default.
(setq frame-inhibit-implied-resize '(tab-bar-lines))
(setq inhibit-startup-screen t)
(switch-to-buffer (get-buffer-create "*N-607-toolbar-implied-resize*"))
(fundamental-mode)
(erase-buffer)
(insert "Toolbar resize witness\n")
(tool-bar-mode 1)

(defvar toolbar-resize-dir (getenv "TOOLBAR_RESIZE_DIR"))
(defvar toolbar-resize-stage nil)
(defvar toolbar-resize-started nil)
(defvar toolbar-resize-last nil)
(defvar toolbar-resize-stable 0)

(defun toolbar-resize-witness ()
  `((pid . ,(emacs-pid))
    (window_system . ,(format "%s" window-system))
    (frame_height . ,(frame-height))
    (frame_text_lines . ,(frame-text-lines))
    (frame_pixel_height . ,(frame-pixel-height))
    (frame_text_height . ,(frame-text-height))
    (frame_char_height . ,(frame-char-height))
    (tool_bar_lines . ,(frame-parameter nil 'tool-bar-lines))
    (inhibit_policy . ,(format "%S" frame-inhibit-implied-resize))
    (font . ,(format "%S" (frame-parameter nil 'font)))))

(defun toolbar-resize-poll ()
  (unless toolbar-resize-stage
    (dolist (stage '(baseline off on))
      (let ((request (expand-file-name (format "go-%s" stage) toolbar-resize-dir)))
        (when (file-exists-p request)
          (delete-file request)
          (setq toolbar-resize-stage stage
                toolbar-resize-started (float-time)
                toolbar-resize-last nil
                toolbar-resize-stable 0)
          (cond ((eq stage 'off) (tool-bar-mode -1))
                ((eq stage 'on) (tool-bar-mode 1)))))))
  (when toolbar-resize-stage
    (let ((data (toolbar-resize-witness)))
      (setq toolbar-resize-stable
            (if (equal data toolbar-resize-last) (1+ toolbar-resize-stable) 0)
            toolbar-resize-last data)
      ;; Let the native resize acknowledgement return through the event loop.
      ;; No manual resize or forced redisplay may repair the product here.
      (when (and (> (- (float-time) toolbar-resize-started) 2)
                 (>= toolbar-resize-stable 3))
        (let* ((name (format "%s.json" toolbar-resize-stage))
               (pending (expand-file-name "result.pending" toolbar-resize-dir)))
          (with-temp-file pending (insert (json-encode data)))
          (rename-file pending (expand-file-name name toolbar-resize-dir) t))
        (when (eq toolbar-resize-stage 'on)
          (run-at-time 0.5 nil (lambda () (kill-emacs 0))))
        (setq toolbar-resize-stage nil)))))

(run-at-time 0.2 0.2 #'toolbar-resize-poll)
;; Reap only this independently launched editor if its controller disappears.
(run-at-time 120 nil (lambda () (kill-emacs 2)))
