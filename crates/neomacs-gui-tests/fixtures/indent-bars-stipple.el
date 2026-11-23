;;; indent-bars-stipple.el --- #831 frame-origin stipple -*- lexical-binding: t -*-
(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)
(set-frame-size nil 640 360 t)
(set-face-attribute 'default nil :foreground "#101010" :background "#ffffff")
(defun neomacs-indent-bars-stipple-start ()
  (condition-case err
      (progn
        (switch-to-buffer (get-buffer-create "*indent-bars-stipple*"))
        (erase-buffer)
        (setq-local mode-line-format nil cursor-type nil)
        ;; A left fringe that is not a multiple of the character width, so a
        ;; glyph-local tile and a frame-origin tile land on different columns.
        (set-window-fringes nil 5 0)
        ;; Bit 0 of the first byte is the leftmost pixel. A wide font is
        ;; more than one byte; the fill-column fixture's trailing 1 would
        ;; light a later pixel and miss this column.
        (let* ((w (window-font-width))
               (nbytes (/ (+ w 7) 8))
               (stipple `(,w 1 ,(apply #'unibyte-string
                                       (cons 1 (make-list (1- nbytes) 0))))))
          (dotimes (_ 16)
            (insert (propertize (make-string 24 ?\s)
                                'face `(:foreground "#ed1731"
                                                    :background "#ffffff"
                                                    :stipple ,stipple)))
            (insert "\n")))
        (goto-char (point-min))
        (redisplay t)
        (run-at-time 0.4 nil #'neomacs-indent-bars-stipple-finish))
    (error (princ (format "indent-bars stipple fixture: %S\n" err) 'external-debugging-output)
           (kill-emacs 1))))
(defun neomacs-indent-bars-stipple-finish ()
  (let* ((w (window-font-width))
         (left (car (window-inside-pixel-edges))))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode `((graphic . ,(if (display-graphic-p) t :json-false))
                            (font_width . ,w)
                            (text_left . ,left)
                            (frame_width . ,(frame-pixel-width))
                            (phase . ,(mod left w)))))))
  (run-at-time 0.3 nil (lambda () (kill-emacs 0))))
(run-at-time 0.5 nil #'neomacs-indent-bars-stipple-start)
