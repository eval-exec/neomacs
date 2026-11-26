;;; wayland-font-raster.el --- Private desktop raster notifications -*- lexical-binding: t; -*-
(require 'json)
(require 'subr-x)
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(set-fringe-mode 0)

(defvar wayland-font-raster-rows nil)
(defvar wayland-font-raster-text "Hamburgefontsiv 0123456789")

(defun wayland-font-raster-observe ()
  (condition-case error-data
      (let ((phase (with-temp-buffer
                     (insert-file-contents (getenv "NEOMACS_RASTER_CONTROL"))
                     (string-trim (buffer-string)))))
        (if (equal phase "stop")
            (kill-emacs 0)
          (with-current-buffer "*wayland-font-raster*"
            ;; Request ordinary redisplay; do not flush fonts or caches here.
            (redisplay t)
            (let* ((window (get-buffer-window (current-buffer)))
                   (edges (window-inside-pixel-edges window))
                   (rows
                    (mapcar
                     (lambda (entry)
                       (let* ((position (cdr entry))
                              (font (font-at position window))
                              (info (font-info font))
                              (positions
                               (vconcat
                                (mapcar
                                 (lambda (offset)
                                   (let ((xy (posn-x-y
                                              (posn-at-point (+ position offset) window))))
                                     (vector (car xy) (cdr xy))))
                                 (number-sequence 0 (length wayland-font-raster-text)))))
                              (xy (aref positions 0))
                              (next (save-excursion
                                      (goto-char position)
                                      (forward-line 1)
                                      (posn-x-y (posn-at-point (point) window)))))
                         `((name . ,(symbol-name (car entry)))
                           (weight . ,(symbol-name (font-get font :weight)))
                           (family . ,(symbol-name (font-get font :family)))
                           (size . ,(font-get font :size))
                           (file . ,(aref info 12))
                           (glyphs . ,(font-get-glyphs
                                        font 0 (length wayland-font-raster-text)
                                        wayland-font-raster-text))
                           (positions . ,positions)
                           (bounds . ,(vector (nth 0 edges)
                                              (+ (nth 1 edges) (aref xy 1))
                                              (nth 2 edges)
                                              (+ (nth 1 edges) (cdr next)))))))
                     wayland-font-raster-rows))
                   (state `((complete . t) (phase . ,phase)
                            (pid . ,(emacs-pid))
                            (metrics . ((char_width . ,(frame-char-width))
                                        (char_height . ,(frame-char-height))
                                        (columns . ,(frame-width))
                                        (lines . ,(frame-height))
                                        (window_edges . ,(vconcat edges))
                                        (rows . ,(vconcat rows)))))))
              (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
                (insert (json-encode state)))))))
    (error
     (princ (format "WAYLAND-RASTER-FAIL %S\n" error-data)
            'external-debugging-output)
     (kill-emacs 1))))

(switch-to-buffer (get-buffer-create "*wayland-font-raster*"))
(setq-local mode-line-format nil header-line-format nil cursor-type nil)
(set-face-attribute 'default nil :font
                    (font-spec :family "IBM Plex Sans" :size 16 :weight 'normal)
                    :foreground "black" :background "white")
(erase-buffer)
(dolist (weight '(normal bold))
  (push (cons weight (point)) wayland-font-raster-rows)
  (insert (propertize wayland-font-raster-text 'face `(:weight ,weight)) "\n\n"))
(setq wayland-font-raster-rows (nreverse wayland-font-raster-rows))
(goto-char (point-min))
(run-at-time 0.1 0.1 #'wayland-font-raster-observe)
(run-at-time 45 nil (lambda () (kill-emacs 2)))
