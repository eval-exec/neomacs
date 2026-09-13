;;; native-resize-increments.el --- Reconfigure a resized HiDPI window -*- lexical-binding: t -*-
(defvar resize-increments-main nil)
(defvar resize-increments-floating-width nil)
(defvar resize-increments-fullscreen-size nil)
(defvar resize-increments-decorated nil)

;; Exercise content geometry independently of client-side decoration buffers.
(unless resize-increments-decorated
  (set-frame-parameter nil 'undecorated t))

(defun resize-increments-check-restored ()
  (let ((columns (frame-width resize-increments-main))
        (width (frame-pixel-width resize-increments-main)))
    (message "Restored resize: expected 91 columns / %S pixels, got %S / %S"
             resize-increments-floating-width columns width)
    (kill-emacs (if (and (= columns 91)
                        (= width resize-increments-floating-width)) 0 1))))

(defun resize-increments-check-rejected ()
  (let ((size (list (frame-pixel-width resize-increments-main)
                    (frame-pixel-height resize-increments-main))))
    (if (not (equal size resize-increments-fullscreen-size))
        (progn (message "Rejected resize changed geometry: %S -> %S"
                        resize-increments-fullscreen-size size)
               (kill-emacs 1))
      (message "Duplicate rejected resize retained fullscreen geometry: %S" size)
      (set-frame-parameter resize-increments-main 'fullscreen nil)
      (run-at-time 1 nil #'resize-increments-check-restored))))

(defun resize-increments-request-while-fullscreen ()
  (setq resize-increments-fullscreen-size
        (list (frame-pixel-width resize-increments-main)
              (frame-pixel-height resize-increments-main)))
  (if (<= (car resize-increments-fullscreen-size) resize-increments-floating-width)
      (progn (message "Native fullscreen did not take effect") (kill-emacs 1))
    ;; Wayland rejects both requests and reports the actual fullscreen size
    ;; immediately. Their duplicate completions must not change geometry.
    (set-frame-width resize-increments-main 101)
    (set-frame-width resize-increments-main 101)
    (run-at-time 1 nil #'resize-increments-check-rejected)))
(run-at-time
 1 nil
 (lambda ()
   (setq resize-increments-main (selected-frame))
   (set-frame-width resize-increments-main 91)
   (run-at-time
    1 nil
    (lambda ()
      (condition-case err
          (progn
            (unless (= (frame-width resize-increments-main) 91)
              (error "Resize must complete before reconfigure"))
            (setq resize-increments-floating-width (frame-pixel-width resize-increments-main))
            ;; Fullscreen restoration forces a compositor configure through
            ;; winit's grid snapping after scale-2 hints have been installed.
            (set-frame-parameter resize-increments-main 'fullscreen 'fullboth)
            (run-at-time 1 nil #'resize-increments-request-while-fullscreen))
        (error (message "Resize reconfigure regression: %S" err) (kill-emacs 1)))))))
