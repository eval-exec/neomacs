;;; font_hinting_advances.el --- Native hinting and text scale -*- lexical-binding: t; -*-
(require 'json)
(setq inhibit-startup-screen t native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(defvar hinting-base nil)

(defun hinting-observe ()
  (let* ((font (font-at 1))
         (info (font-info font))
         (left (car (posn-x-y (posn-at-point 1))))
         (right (car (posn-x-y (posn-at-point 9)))))
    `((family . ,(format "%s" (font-get font :family)))
      (weight . ,(format "%s" (font-get font :weight)))
      (slant . ,(format "%s" (font-get font :slant)))
      (width . ,(format "%s" (font-get font :width)))
      (size . ,(font-get font :size))
      (file . ,(aref info 12))
      (frame_width . ,(frame-char-width))
      (window_width . ,(window-font-width))
      (m_width . ,(/ (- right left) 8.0))
      ;; Preserve backend identity diagnostics. GNU's ordinary font APIs do
      ;; not expose the loaded FreeType design coordinates; these properties
      ;; are not substitutes for the separate native coordinate control.
      (font_entity . ,(format "%S" (font-get font :font-entity)))
      (variation_property . ,(format "%S" (font-get font :fontvariations))))))

(defun hinting-start ()
  (switch-to-buffer (get-buffer-create "*font-hinting-advances*"))
  (setq-local mode-line-format nil header-line-format nil cursor-type nil)
  (erase-buffer)
  (insert "mmmmmmmm\n")
  (goto-char 1)
  (set-face-attribute 'default nil :font
                      (font-spec :family "Noto Sans" :size 14
                                 :weight 'regular :slant 'normal :width 'normal))
  (redisplay t)
  (run-at-time 0.3 nil #'hinting-remap))

(defun hinting-remap ()
  (with-selected-window (get-buffer-window "*font-hinting-advances*")
    (redisplay t)
    (setq hinting-base (hinting-observe))
    (setq-local face-remapping-alist '((default (:height 0.5) default)))
    (redisplay t))
  (run-at-time 0.3 nil #'hinting-finish))

(defun hinting-finish ()
  (with-selected-window (get-buffer-window "*font-hinting-advances*")
    (redisplay t)
    (let ((state `((base . ,hinting-base)
                   (scaled . ,(hinting-observe))
                   (form . "(setq-local face-remapping-alist '((default (:height 0.5) default)))"))))
      (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
        (insert (json-encode state)))
      (princ (format "FONT-HINTING-ADVANCES %S\n" state)
             'external-debugging-output)))
  (kill-emacs 0))

(defun hinting-guard (function &rest arguments)
  (condition-case err
      (apply function arguments)
    (error
     (princ (format "FONT-HINTING-ADVANCES-FAIL %S\n" err)
            'external-debugging-output)
     (kill-emacs 1))))
(dolist (function '(hinting-start hinting-remap hinting-finish))
  (advice-add function :around #'hinting-guard))
(run-at-time 0.5 nil #'hinting-start)
(run-at-time 15 nil (lambda () (kill-emacs 2)))
