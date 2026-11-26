;;; font-hinting-weight.el --- Same-file raster weight oracle -*- lexical-binding: t; -*-
(require 'json)
(setq inhibit-startup-screen t
      native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(set-fringe-mode 0)

(defvar font-hinting-weight-positions nil)

(defun font-hinting-weight-start ()
  (switch-to-buffer (get-buffer-create "*font-hinting-weight*"))
  (setq-local mode-line-format nil header-line-format nil
              cursor-type nil line-spacing nil)
  ;; Original report: Plex12pt. Later glyph dump: FiraCode semi-light13.5pt.
  (set-face-attribute 'default nil
                      :family (or (getenv "NEOMACS_FONT_WEIGHT_FAMILY") "IBM Plex Sans")
                      :height (string-to-number (or (getenv "NEOMACS_FONT_WEIGHT_HEIGHT") "120"))
                      :weight (intern (or (getenv "NEOMACS_FONT_WEIGHT_FIRST_WEIGHT") "normal")) :slant 'normal
                      :foreground "black" :background "white")
  ;; Hold the physical size fixed in the no-Xft-resource arm, whose server
  ;; geometry need not have exactly96DPI. Integer font-spec sizes are pixels.
  (when (getenv "NEOMACS_FONT_WEIGHT_PIXEL_SIZE")
    (set-face-attribute 'default nil :font
                        (font-spec :family (getenv "NEOMACS_FONT_WEIGHT_FAMILY")
                                   :size (string-to-number (getenv "NEOMACS_FONT_WEIGHT_PIXEL_SIZE"))
                                   :weight (intern (getenv "NEOMACS_FONT_WEIGHT_FIRST_WEIGHT"))
                                   :slant 'normal)))
  (erase-buffer)
  (dolist (weight (list (intern (or (getenv "NEOMACS_FONT_WEIGHT_FIRST_WEIGHT") "normal")) 'bold))
    (push (cons weight (point)) font-hinting-weight-positions)
    (insert (propertize "Hamburgefontsiv 0123456789" 'face `(:weight ,weight))
            "\n\n"))
  (setq font-hinting-weight-positions (nreverse font-hinting-weight-positions))
  (goto-char (point-min))
  (redisplay t)
  (run-at-time 0.5 nil #'font-hinting-weight-observe))

(defun font-hinting-weight-observe ()
  (with-current-buffer "*font-hinting-weight*"
    (redisplay t)
    (let* ((window (get-buffer-window (current-buffer)))
           (edges (window-inside-pixel-edges window))
           (rows
            (mapcar
             (lambda (entry)
               (let* ((position (cdr entry))
                      (font (font-at position window))
                      (info (font-info font))
                      (xy (posn-x-y (posn-at-point position window)))
                      (next (save-excursion
                              (goto-char position)
                              (forward-line 1)
                              (posn-x-y (posn-at-point (point) window)))))
                 `((name . ,(symbol-name (car entry)))
                   (family . ,(symbol-name (font-get font :family)))
                   (weight . ,(symbol-name (font-get font :weight)))
                   (slant . ,(symbol-name (font-get font :slant)))
                   (size . ,(font-get font :size))
                   (file . ,(aref info 12))
                   ;; Full text width, one actual display row. No fixed frame
                   ;; origin or assumed line height enters the ink measurement.
                   (bounds . ,(vector (nth 0 edges)
                                      (+ (nth 1 edges) (cdr xy))
                                      (nth 2 edges)
                                      (+ (nth 1 edges) (cdr next)))))))
             font-hinting-weight-positions))
           (state `((rows . ,(vconcat rows))
                    (window_edges . ,(vconcat edges))
                    (frame_char_height . ,(frame-char-height)))))
      (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
        (insert (json-encode state)))
      (princ (format "FONT-HINTING-WEIGHT %S\n" state)
             'external-debugging-output)))
  (when (getenv "NEOMACS_FONT_WEIGHT_GNU_PNG")
    (with-temp-file (getenv "NEOMACS_FONT_WEIGHT_GNU_PNG")
      (set-buffer-multibyte nil)
      (insert (x-export-frames nil 'png))))
  ;; The oracle is the visible window. Leave time for the harness to grab it.
  (run-at-time 12 nil (lambda () (kill-emacs 0))))

(defun font-hinting-weight-guard (function &rest arguments)
  (condition-case error-data
      (apply function arguments)
    (error
     (princ (format "FONT-HINTING-WEIGHT-FAIL %S\n" error-data)
            'external-debugging-output)
     (kill-emacs 1))))

(dolist (function '(font-hinting-weight-start font-hinting-weight-observe))
  (advice-add function :around #'font-hinting-weight-guard))
(run-at-time 0.5 nil #'font-hinting-weight-start)
(run-at-time 30 nil (lambda () (kill-emacs 2)))
