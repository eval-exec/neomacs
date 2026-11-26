;;; startup-phases.el --- Isolated font/geometry phase observations -*- lexical-binding: t; -*-

(require 'json)
(setq inhibit-startup-screen t
      package-enable-at-startup nil)

(defun geometry-startup-record (phase)
  (let* ((directory (getenv "GEOMETRY_BENCH_DIR"))
         (path (expand-file-name (concat "phase-" phase ".json") directory))
         (pending (concat path ".pending"))
         (data `((phase . ,phase)
                 (pid . ,(emacs-pid))
                 (window_system . ,(format "%s" window-system))
                 (font . ,(format "%S" (frame-parameter nil 'font)))
                 (frame_char_height . ,(frame-char-height))
                 (frame_pixel_height . ,(frame-pixel-height))
                 (frame_rows . ,(frame-height))
                 (frame_inhibit_implied_resize
                  . ,(if (boundp 'frame-inhibit-implied-resize)
                         (format "%S" frame-inhibit-implied-resize)
                       "UNBOUND")))))
    (with-temp-file pending (insert (json-encode data)))
    (rename-file pending path t)))

(geometry-startup-record "early-init")
(add-hook 'after-init-hook
          (lambda () (geometry-startup-record "after-init")))
(add-hook 'window-setup-hook
          (lambda () (geometry-startup-record "window-setup")))
