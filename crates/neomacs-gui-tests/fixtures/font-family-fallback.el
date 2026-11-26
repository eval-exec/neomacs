;;; font-family-fallback.el --- Default family realization -*- lexical-binding: t; -*-
(require 'json)
(setq inhibit-startup-screen t native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(defvar fallback-state nil)
(defvar fallback-errors nil)
(defvar fallback-setter nil)

(defun fallback-raw ()
  ;; Read attributes before font/geometry queries can trigger realization.
  `((family . ,(face-attribute 'default :family (selected-frame)))
    (global_family . ,(face-attribute 'default :family t))
    (foundry . ,(face-attribute 'default :foundry (selected-frame)))
    (height . ,(face-attribute 'default :height (selected-frame)))
    (width . ,(format "%S" (face-attribute 'default :width (selected-frame))))
    (weight . ,(format "%S" (face-attribute 'default :weight (selected-frame))))
    (font . ,(format "%S" (face-attribute 'default :font (selected-frame))))))

(defun fallback-measure ()
  (let* ((raw (fallback-raw))
         (font (font-at (point-min)))
         (info (font-info font))
         (left (car (posn-x-y (posn-at-point (point-min)))))
         (right (car (posn-x-y (posn-at-point (+ (point-min) 8))))))
    `((raw . ,raw)
      (family . ,(format "%s" (font-get font :family)))
      (foundry . ,(format "%s" (font-get font :foundry)))
      (weight . ,(format "%s" (font-get font :weight)))
      (slant . ,(format "%s" (font-get font :slant)))
      (size . ,(font-get font :size))
      (file . ,(aref info 12))
      (frame_font . ,(format "%S" (frame-parameter nil 'font)))
      (frame_width . ,(frame-char-width))
      (window_width . ,(window-font-width))
      (m_width . ,(/ (- right left) 8.0))
      (raw_after_queries . ,(fallback-raw)))))

(defun fallback-record (phase function)
  (condition-case err
      (push (cons phase (funcall function)) fallback-state)
    (error
     ;; Keep the original printed error as evidence and normalize its font
     ;; argument for comparison across the two font object implementations.
     (let ((spec (nth 2 err)))
       (push `((phase . ,phase)
               (condition . ,(symbol-name (car err)))
               (message . ,(cadr err))
               (family . ,(and (fontp spec) (format "%s" (font-get spec :family))))
               (size . ,(and (fontp spec) (font-get spec :size)))
               (user_spec . ,(and (fontp spec) (font-get spec :user-spec)))
               (printed . ,(format "%S" err)))
             fallback-errors)))))

(defun fallback-start ()
  (switch-to-buffer (get-buffer-create "*font-family-fallback*"))
  (setq-local mode-line-format nil header-line-format nil cursor-type nil)
  (insert "mmmmmmmm\n")
  (goto-char (point-min))
  (set-face-attribute 'default (selected-frame) :font
                      (font-spec :family (getenv "FONT_FALLBACK_INITIAL") :size 14))
  (redisplay t)
  (fallback-record "initial" #'fallback-measure)
  (let* ((family (getenv "FONT_FALLBACK_FAMILY"))
         (scope (getenv "FONT_FALLBACK_SCOPE"))
         (frame-form (cond ((equal scope "nil") nil)
                           ((equal scope "t") t)
                           (t '(selected-frame)))))
    (setq fallback-setter
          (cond
           ((equal (getenv "FONT_FALLBACK_OPERATION") "font-spec")
            `(set-face-attribute 'default ,frame-form :font
                                 (font-spec :family ,family :size 14)))
           ((equal (getenv "FONT_FALLBACK_OPERATION") "font-name")
            `(set-face-attribute 'default ,frame-form :font ,family))
           (t `(set-face-attribute 'default ,frame-form :family ,family)))))
  (fallback-record "setter" (lambda () (format "%S" (eval fallback-setter t))))
  ;; This must be the first observation after the setter returns or signals.
  (fallback-record "immediate" #'fallback-raw)
  (redisplay t)
  (fallback-record "redisplayed" #'fallback-measure)
  (run-at-time 0.3 nil #'fallback-remap))

(defun fallback-remap ()
  (with-current-buffer "*font-family-fallback*"
    (fallback-record "settled" #'fallback-measure)
    ;; This remapping supplies no family and must use the realized default.
    (setq-local face-remapping-alist '((default (:height 0.5) default)))
    (redisplay t)
    (fallback-record "remapped" #'fallback-measure))
  (run-at-time 0.3 nil #'fallback-finish))

(defun fallback-finish ()
  (with-current-buffer "*font-family-fallback*"
    (redisplay t)
    (fallback-record "remapped-settled" #'fallback-measure))
  (let ((state `((form . ,(format "%S" fallback-setter))
                 (remap_form . "(setq-local face-remapping-alist '((default (:height 0.5) default)))")
                 (phases . ,(nreverse fallback-state))
                 (errors . ,(vconcat (nreverse fallback-errors))))))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON") (insert (json-encode state)))
    (princ (format "FONT-FAMILY-FALLBACK %S\n" state) 'external-debugging-output))
  (kill-emacs 0))

(defun fallback-guard (function &rest arguments)
  (condition-case err
      (apply function arguments)
    (error
     (fallback-record "uncaught" (lambda () (signal (car err) (cdr err))))
     (fallback-finish))))
(dolist (function '(fallback-start fallback-remap))
  (advice-add function :around #'fallback-guard))
(run-at-time 0.5 nil #'fallback-start)
(run-at-time 15 nil (lambda () (kill-emacs 2)))
