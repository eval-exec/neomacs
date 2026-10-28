;;; native-resize-increments.el --- Reconfigure a resized HiDPI window -*- lexical-binding: t -*-
(defun resize-increments-fail (format-string &rest args)
  ;; The echo area dies with the editor; put the reason in the artifact log.
  (let ((text (apply #'format format-string args)))
    (princ (format "RESIZE-INCREMENTS-FAIL %s\n" text) 'external-debugging-output)
    (message "%s" text)
    (kill-emacs 1)))

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
    (if (and (= columns 91) (= width resize-increments-floating-width))
        (kill-emacs 0)
      (resize-increments-fail "Restored resize: expected 91 columns / %S pixels, got %S / %S"
                              resize-increments-floating-width columns width))))

(defun resize-increments-check-rejected ()
  (let ((size (list (frame-pixel-width resize-increments-main)
                    (frame-pixel-height resize-increments-main))))
    (if (not (equal size resize-increments-fullscreen-size))
        (resize-increments-fail "Rejected resize changed geometry: %S -> %S"
                                resize-increments-fullscreen-size size)
      (message "Duplicate rejected resize retained fullscreen geometry: %S" size)
      (set-frame-parameter resize-increments-main 'fullscreen nil)
      (run-at-time 1 nil #'resize-increments-check-restored))))

(defun resize-increments-request-while-fullscreen ()
  (setq resize-increments-fullscreen-size
        (list (frame-pixel-width resize-increments-main)
              (frame-pixel-height resize-increments-main)))
  (if (<= (car resize-increments-fullscreen-size) resize-increments-floating-width)
      (resize-increments-fail "Native fullscreen did not take effect")
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
              (error "Resize must complete before reconfigure: width %S"
                     (frame-width resize-increments-main)))
            (setq resize-increments-floating-width (frame-pixel-width resize-increments-main))
            ;; Fullscreen restoration forces a compositor configure through
            ;; winit's grid snapping after scale-2 hints have been installed.
            (set-frame-parameter resize-increments-main 'fullscreen 'fullboth)
            (run-at-time 1 nil #'resize-increments-request-while-fullscreen))
        (error (resize-increments-fail "Resize reconfigure regression: %S" err)))))))
