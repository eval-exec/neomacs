;;; explicit-size-after-font-oracle.el --- Explicit size after a font change -*- lexical-binding: t -*-
;;;
;;; `frame-inhibit-implied-resize' is t. A font height change runs, then
;;; `window-setup-hook' sets width 120 and height 40. Two seconds later the
;;; named X window is measured before any Lisp geometry read. GNU delivers
;;; that size inside `modify-frame-parameters' (`adjust_frame_size' inhibit
;;; 1, src/frame.c). A later tool-bar ConfigureNotify can shrink the native
;;; height by the tool bar's own pixels, so `frame-height' need not stay
;;; 40. Reading `frame-width' before the sample would flush a queued resize.
(require 'subr-x)
(setq inhibit-startup-screen t
      frame-inhibit-implied-resize t
      frame-title-format "oracleb827")
(modify-frame-parameters nil '((name . "oracleb827") (title . "oracleb827")))
(set-face-attribute 'default nil :height 160)
(add-hook 'window-setup-hook
          (lambda ()
            (modify-frame-parameters nil '((width . 120) (height . 40)))))

(defun n827-wh (id)
  "Return (ID WIDTH HEIGHT) for an X window. Width and height are 0 when
xwininfo has no stats."
  (let ((info (shell-command-to-string (format "xwininfo -id %s -stats" id))))
    (list id
          (if (string-match "Width: \\([0-9]+\\)" info)
              (string-to-number (match-string 1 info))
            0)
          (if (string-match "Height: \\([0-9]+\\)" info)
              (string-to-number (match-string 1 info))
            0))))

(run-with-timer
 2 nil
 (lambda ()
   (let* ((sizes (mapcar
                  #'n827-wh
                  (split-string
                   (shell-command-to-string "xdotool search --name oracleb827")
                   "\n" t)))
          (best (car (sort sizes
                           (lambda (a b)
                             (> (* (nth 1 a) (nth 2 a))
                                (* (nth 1 b) (nth 2 b)))))))
          (width (if best (nth 1 best) 0))
          (height (if best (nth 2 best) 0)))
     ;; Lisp geometry is read only after the external sample.
     (princ (format "EXPLICIT-SIZE xwin=%sx%s frame=%sx%s char=%sx%s root=%s\n"
                    width
                    height
                    (frame-width)
                    (frame-height)
                    (frame-char-width)
                    (frame-char-height)
                    (window-total-width (frame-root-window)))
            'external-debugging-output)
     (kill-emacs 0))))
