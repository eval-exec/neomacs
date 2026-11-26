;;; desktop-text-scaling.el --- Point fonts follow desktop text DPI -*- lexical-binding: t; -*-
(require 'json)
(setq inhibit-startup-screen t
      native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)

(defvar desktop-text-scaling-points nil)

(defun desktop-text-scaling-state ()
  ;; Timer callbacks need not run with the displayed buffer current.
  (with-current-buffer "*desktop-text-scaling*"
    (let ((font (font-at (point-min))))
      `((family . ,(font-get font :family))
        (size . ,(font-get font :size))
        (width . ,(frame-char-width))
        (line . ,(frame-char-height))))))

(defun desktop-text-scaling-snapshot (suffix)
  (when (fboundp 'neomacs--write-frame-snapshot)
    (with-selected-window (get-buffer-window "*desktop-text-scaling*")
      (neomacs--write-frame-snapshot
       (concat (getenv "NEOMACS_GUI_STATE_JSON") suffix) nil 'json))))

(defun desktop-text-scaling-finish ()
  (redisplay t)
  (desktop-text-scaling-snapshot ".pixels.frame.json")
  (let ((state `((points . ,desktop-text-scaling-points)
                 (pixels . ,(desktop-text-scaling-state)))))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode state)))
    (princ (format "DESKTOP-TEXT-SCALING %S\n" state)
           'external-debugging-output))
  (kill-emacs 0))

(defun desktop-text-scaling-pixel-font ()
  (redisplay t)
  (setq desktop-text-scaling-points (desktop-text-scaling-state))
  (desktop-text-scaling-snapshot ".points.frame.json")
  ;; font.c:339-362 returns an integer size directly, bypassing frame DPI.
  (set-face-attribute 'default nil :font
                      (font-spec :family "DejaVu Sans Mono" :size 16))
  (redisplay t)
  (run-at-time 0.3 nil #'desktop-text-scaling-finish))

(defun desktop-text-scaling-start ()
  (switch-to-buffer (get-buffer-create "*desktop-text-scaling*"))
  (setq-local mode-line-format nil cursor-type nil)
  (insert "mmmmmmmm\n")
  (goto-char (point-min))
  ;; A named size is in printer's points; compare against GNU effective DPI.
  (set-frame-font "DejaVu Sans Mono-12" nil t)
  (redisplay t)
  (run-at-time 0.3 nil #'desktop-text-scaling-pixel-font))

(defun desktop-text-scaling-guard (function &rest arguments)
  (condition-case error-data
      (apply function arguments)
    (error
     (princ (format "DESKTOP-TEXT-SCALING-FAIL %S\n" error-data)
            'external-debugging-output)
     (kill-emacs 1))))

(dolist (function '(desktop-text-scaling-start
                    desktop-text-scaling-pixel-font
                    desktop-text-scaling-finish))
  (advice-add function :around #'desktop-text-scaling-guard))
(run-at-time 0.5 nil #'desktop-text-scaling-start)
(run-at-time 20 nil (lambda () (kill-emacs 2)))
