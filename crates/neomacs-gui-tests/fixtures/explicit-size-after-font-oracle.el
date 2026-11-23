;;; explicit-size-after-font-oracle.el --- Explicit size after a font change -*- lexical-binding: t -*-
;;;
;;; A font height change runs, then an explicit 120x40 is applied. With
;;; N827_WHEN unset, that size runs from `window-setup-hook', before GTK
;;; realizes the tool bar. With N827_WHEN=redisplay, a timer applies it
;;; after the first redisplay. N827_INHIBIT selects t, tool-bar-lines, or
;;; default. The default is t, which is the window-setup-hook sample.
;;; Reading `frame-width' before the sample would flush a queued resize.
(require 'subr-x)
(setq inhibit-startup-screen t
      frame-title-format "oracleb827")
(let ((inhibit (getenv "N827_INHIBIT")))
  (cond
   ((equal inhibit "tool-bar-lines")
    (setq frame-inhibit-implied-resize '(tool-bar-lines)))
   ((equal inhibit "default") nil)
   (t (setq frame-inhibit-implied-resize t))))
(modify-frame-parameters nil '((name . "oracleb827") (title . "oracleb827")))
(set-face-attribute 'default nil :height 160)
(defun n827-apply-size ()
  (modify-frame-parameters nil '((width . 120) (height . 40))))
(if (equal (getenv "N827_WHEN") "redisplay")
    (run-with-timer 1 nil #'n827-apply-size)
  (add-hook 'window-setup-hook #'n827-apply-size))

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
 (if (equal (getenv "N827_WHEN") "redisplay") 3 2)
 nil
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
     ;; `frame-text-height` does not include the tool bar. Each editor's
     ;; own `tool-bar-size` is printed so the row can recover the shared
     ;; text grid without comparing GTK widget pixels.
     (let* ((geometry (frame-geometry))
            (tool (cdr (cdr (assq 'tool-bar-size geometry))))
            (menu (cdr (cdr (assq 'menu-bar-size geometry)))))
       (princ (format "EXPLICIT-SIZE xwin=%sx%s frame=%sx%s char=%sx%s text=%s tool=%s menu=%s root=%s\n"
                      width
                      height
                      (frame-width)
                      (frame-height)
                      (frame-char-width)
                      (frame-char-height)
                      (frame-text-height)
                      tool
                      menu
                      (window-total-width (frame-root-window)))
              'external-debugging-output))
     (kill-emacs 0))))
