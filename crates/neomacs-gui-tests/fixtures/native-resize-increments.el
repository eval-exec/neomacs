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
(defvar resize-increments-restore-submission 0)
(defvar resize-increments-decorated nil)
(defvar resize-increments-timer nil)

(defun resize-increments-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer (insert-file-contents file) (read (current-buffer))))))

(defun resize-increments-presented (after)
  "Non-nil once the native display has presented a frame after AFTER."
  (or (not (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT"))
      (let ((receipt (resize-increments-receipt)))
        (and receipt
             (eq (plist-get receipt :outcome) 'presented)
             (> (plist-get receipt :submission) after)
             (integerp (plist-get receipt :clock-id))
             (natnump (plist-get receipt :seconds))
             (natnump (plist-get receipt :nanoseconds))
             (< (plist-get receipt :nanoseconds) 1000000000)))))

(defun resize-increments-wait-until (predicate on-success)
  "Poll PREDICATE every 20 ms and call ON-SUCCESS.
No wall-clock stage deadline: the predicate carries the geometry change and
the compositor's presentation receipt, and the harness's process timeout is
the only bound on a missing receipt."
  (setq resize-increments-timer
        (run-at-time
         0 0.02
         (lambda ()
           (when (funcall predicate)
             (cancel-timer resize-increments-timer)
             (setq resize-increments-timer nil)
             (funcall on-success))))))

;; Exercise content geometry independently of client-side decoration buffers.
(unless resize-increments-decorated
  (set-frame-parameter nil 'undecorated t))

(defun resize-increments-check-restored ()
  (resize-increments-wait-until
   (lambda ()
     (and (= (frame-width resize-increments-main) 91)
          (= (frame-pixel-width resize-increments-main)
             resize-increments-floating-width)
          (resize-increments-presented resize-increments-restore-submission)))
   (lambda () (kill-emacs 0))))

(defun resize-increments-check-rejected ()
  ;; The duplicate requests were rejected synchronously; a settle window is
  ;; the only witness for a negative "geometry did not change" assertion.
  ;; After it, remember the fullscreen presentation serial so the restored
  ;; frame must present a newer one before the test exits.
  (run-at-time
   1 nil
   (lambda ()
     (let ((size (list (frame-pixel-width resize-increments-main)
                       (frame-pixel-height resize-increments-main))))
       (if (not (equal size resize-increments-fullscreen-size))
           (resize-increments-fail "Rejected resize changed geometry: %S -> %S"
                                   resize-increments-fullscreen-size size)
         (setq resize-increments-restore-submission
               (or (plist-get (resize-increments-receipt) :submission) 0))
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
  ;; grid snapping after scale-2 hints have been installed.  Wait for the
  ;; fullscreen frame's own presentation receipt before submitting the
  ;; duplicate width requests that must be rejected as fullscreen.
  (let ((submission (or (plist-get (resize-increments-receipt) :submission) 0)))
    (set-frame-parameter resize-increments-main 'fullscreen 'fullboth)
    (resize-increments-wait-until
     (lambda ()
       (and (> (frame-pixel-width resize-increments-main)
               resize-increments-floating-width)
            (resize-increments-presented submission)))
     #'resize-increments-request-while-fullscreen)))

(run-at-time
 0.2 nil
 (lambda ()
   (setq resize-increments-main (selected-frame))
   (let ((submission (or (plist-get (resize-increments-receipt) :submission) 0)))
     (set-frame-width resize-increments-main 91)
     (resize-increments-wait-until
      (lambda ()
        (and (= (frame-width resize-increments-main) 91)
             (resize-increments-presented submission)))
      #'resize-increments-after-resize))))
