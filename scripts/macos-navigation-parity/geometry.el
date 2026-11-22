;;; geometry.el --- Native GUI metric witness for N-607 -*- lexical-binding: t; -*-

(require 'json)
(setq inhibit-startup-screen t)
(switch-to-buffer (get-buffer-create "*N-607-frame-geometry*"))
(fundamental-mode)
(erase-buffer)
(insert "Menlo geometry witness\n")
(goto-char (point-min))
(set-window-start (selected-window) (point-min))

(defvar geometry-dir (getenv "GEOMETRY_BENCH_DIR"))
(defvar geometry-timer nil)

(defun geometry-context (function &rest arguments)
  (condition-case err
      (format "%S" (apply function arguments))
    (error (format "ERROR %S" err))))

(defun geometry-write-witness ()
  (when (file-exists-p (expand-file-name "go" geometry-dir))
    (cancel-timer geometry-timer)
    (redisplay t)
    (let* ((pending (expand-file-name "result.pending" geometry-dir))
           ;; Capture in the selected text buffer, before serialization binds
           ;; a temporary buffer. line-pixel-height must observe this line.
           (data
            `((pid . ,(emacs-pid))
            (window_system . ,(format "%s" window-system))
            (frame_pixel_height . ,(frame-pixel-height))
            (frame_char_height . ,(frame-char-height))
            (line_pixel_height . ,(line-pixel-height))
            (frame_pixel_width . ,(frame-pixel-width))
            (frame_char_width . ,(frame-char-width))
            (frame_rows . ,(frame-height))
            (frame_columns . ,(frame-width))
            (window_pixel_height . ,(window-pixel-height))
            (body_pixel_height . ,(window-body-height nil t))
            (frame_text_height . ,(geometry-context #'frame-text-height nil))
            (window_line_height . ,(geometry-context #'window-line-height))
            (line_spacing . ,(format "%S" line-spacing))
            (default_face_height . ,(geometry-context #'face-attribute 'default :height))
            (font_info . ,(geometry-context #'font-info (face-font 'default)))
            (parameters . ,(mapcar
                            (lambda (key)
                              (cons (symbol-name key)
                                    (format "%S" (frame-parameter nil key))))
                            '(font internal-border-width border-width
                              menu-bar-lines tool-bar-lines tab-bar-lines
                              left-fringe right-fringe scroll-bar-width
                              right-divider-width bottom-divider-width))))))
      (with-temp-file pending (insert (json-encode data)))
      (rename-file pending (expand-file-name "result.json" geometry-dir) t))
    (run-at-time 0.5 nil (lambda () (kill-emacs 0)))))

(setq geometry-timer (run-at-time 0.1 0.1 #'geometry-write-witness))
;; Only this independently launched editor is reaped if its controller vanishes.
(run-at-time 90 nil (lambda () (kill-emacs 2)))
