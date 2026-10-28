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
(defvar resize-increments-timer nil)

;; A software Wayland compositor acknowledges a configure asynchronously; the
;; settle time depends on the renderer and the runner's load.  Wait for the
;; observable geometry instead of assuming a fixed delay is enough.
(defconst resize-increments-timeout 12)

(defun resize-increments-wait-until (predicate on-success failure)
  "Poll PREDICATE every 20 ms; call ON-SUCCESS or FAILURE at the deadline.
FAILURE runs when the deadline passes without PREDICATE; it reports through
`resize-increments-fail'."
  (let ((deadline (+ (float-time) resize-increments-timeout)))
    (setq resize-increments-timer
          (run-at-time
           0 0.02
           (lambda ()
             (cond
              ((funcall predicate)
               (cancel-timer resize-increments-timer)
               (setq resize-increments-timer nil)
               (funcall on-success))
              ((> (float-time) deadline)
               (cancel-timer resize-increments-timer)
               (setq resize-increments-timer nil)
               (funcall failure))))))))

;; Exercise content geometry independently of client-side decoration buffers.
(unless resize-increments-decorated
  (set-frame-parameter nil 'undecorated t))

(defun resize-increments-check-restored ()
  (resize-increments-wait-until
   (lambda ()
     (and (= (frame-width resize-increments-main) 91)
          (= (frame-pixel-width resize-increments-main)
             resize-increments-floating-width)))
   (lambda () (kill-emacs 0))
   (lambda ()
     (let ((columns (frame-width resize-increments-main))
           (width (frame-pixel-width resize-increments-main)))
       (resize-increments-fail
        "Restored resize: expected 91 columns / %S pixels, got %S / %S"
        resize-increments-floating-width columns width)))))

(defun resize-increments-check-rejected ()
  ;; The duplicate requests were rejected synchronously; a settle window is
  ;; the only witness for a negative "geometry did not change" assertion.
  (run-at-time
   1 nil
   (lambda ()
     (let ((size (list (frame-pixel-width resize-increments-main)
                       (frame-pixel-height resize-increments-main))))
       (if (not (equal size resize-increments-fullscreen-size))
           (resize-increments-fail "Rejected resize changed geometry: %S -> %S"
                                   resize-increments-fullscreen-size size)
         (message "Duplicate rejected resize retained fullscreen geometry: %S" size)
         (set-frame-parameter resize-increments-main 'fullscreen nil)
         (resize-increments-check-restored))))))

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
    (resize-increments-check-rejected)))

(defun resize-increments-after-resize ()
  (setq resize-increments-floating-width
        (frame-pixel-width resize-increments-main))
  ;; Fullscreen restoration forces a compositor configure through winit's
  ;; grid snapping after scale-2 hints have been installed.
  (set-frame-parameter resize-increments-main 'fullscreen 'fullboth)
  (resize-increments-wait-until
   (lambda ()
     (> (frame-pixel-width resize-increments-main)
        resize-increments-floating-width))
   #'resize-increments-request-while-fullscreen
   (lambda () (resize-increments-fail "Native fullscreen did not take effect"))))

(run-at-time
 0.2 nil
 (lambda ()
   (setq resize-increments-main (selected-frame))
   (set-frame-width resize-increments-main 91)
   (resize-increments-wait-until
    (lambda () (= (frame-width resize-increments-main) 91))
    #'resize-increments-after-resize
    (lambda ()
      (resize-increments-fail "Resize reconfigure regression: resize did not complete: width %S"
                              (frame-width resize-increments-main))))))
