;;; frame_native_average_width.el --- Native frame-column provenance -*- lexical-binding: t; -*-
(require 'json)
(require 'cl-lib)
(setq inhibit-startup-screen t native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(scroll-bar-mode -1)
(blink-cursor-mode -1)
(defvar frame-average-states nil)
(defvar frame-average-immediate nil)

(defun frame-average-set-font (size)
  (set-face-attribute 'default (selected-frame) :font
                      (font-spec :family "Noto Sans Mono" :size size
                                 :weight 'regular :width 'normal :slant 'normal))
  ;; Observe the synchronous font publication before redisplay can replace it.
  (setq frame-average-immediate (frame-char-width))
  (redisplay t))

(defun frame-average-observe ()
  (with-current-buffer "*frame-native-average-width*"
    (let* ((frame-width-before-queries (frame-char-width))
           (font (font-at 1))
           (info (font-info font))
           (positions (mapcar (lambda (p) (car (posn-x-y (posn-at-point p))))
                              '(1 2 3 4 5))))
      `((family . ,(format "%s" (font-get font :family)))
        (weight . ,(format "%s" (font-get font :weight)))
        (slant . ,(format "%s" (font-get font :slant)))
        (width . ,(format "%s" (font-get font :width)))
        (size . ,(font-get font :size))
        (file . ,(aref info 12))
        (font_entity . ,(format "%S" (font-get font :font-entity)))
        (immediate_frame_width . ,frame-average-immediate)
        (frame_width_before_queries . ,frame-width-before-queries)
        (frame_width . ,(frame-char-width))
        (window_width . ,(window-font-width))
        (font_average_width . ,(aref info 11))
        (font_max_width . ,(aref info 7))
        (columns . ,(frame-width))
        (text_pixels . ,(frame-text-width))
        (frame_pixels . ,(frame-pixel-width))
        (advances . ,(vconcat (cl-mapcar #'- (cdr positions) (butlast positions))))))))

(defun frame-average-start ()
  (switch-to-buffer (get-buffer-create "*frame-native-average-width*"))
  (setq-local mode-line-format nil header-line-format nil cursor-type nil)
  (erase-buffer)
  (insert "AMi.\n")
  (goto-char 1)
  (frame-average-set-font 14)
  (run-at-time 0.3 nil #'frame-average-resize-14))

(defun frame-average-resize-14 ()
  (set-frame-size (selected-frame) 80 20)
  (run-at-time 0.3 nil #'frame-average-record-14))

(defun frame-average-record-14 ()
  (redisplay t)
  (push (cons 'size14 (frame-average-observe)) frame-average-states)
  (frame-average-set-font 13)
  (run-at-time 0.3 nil #'frame-average-resize-13))

(defun frame-average-resize-13 ()
  (set-frame-size (selected-frame) 80 20)
  (run-at-time 0.3 nil #'frame-average-finish))

(defun frame-average-finish ()
  (redisplay t)
  (push (cons 'size13 (frame-average-observe)) frame-average-states)
  (let ((state (nreverse frame-average-states)))
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON") (insert (json-encode state)))
    (princ (format "FRAME-NATIVE-AVERAGE %S\n" state) 'external-debugging-output))
  (kill-emacs 0))

(defun frame-average-guard (function &rest arguments)
  (condition-case err
      (apply function arguments)
    (error
     (princ (format "FRAME-NATIVE-AVERAGE-FAIL %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))
(dolist (function '(frame-average-start frame-average-resize-14 frame-average-record-14
                   frame-average-resize-13 frame-average-finish))
  (advice-add function :around #'frame-average-guard))
(run-at-time 0.5 nil #'frame-average-start)
(run-at-time 15 nil (lambda () (kill-emacs 2)))
