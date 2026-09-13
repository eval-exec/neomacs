;;; native-resize-presented.el --- Fullscreen after compositor feedback -*- lexical-binding: t -*-

(defvar native-presented-deadline (+ (float-time) 12))
(defvar native-presented-stage 'startup)
(defvar native-presented-submission 0)
(defvar native-presented-timer nil)

(defun native-presented-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer
        (insert-file-contents file)
        (read (current-buffer))))))

(defun native-presented-advance ()
  (condition-case err
      (let ((receipt (native-presented-receipt)))
        (when (> (float-time) native-presented-deadline)
          (error "No compositor-confirmed presentation in stage %S; last receipt %S"
                 native-presented-stage receipt))
        (when (and receipt
                   (> (plist-get receipt :submission) native-presented-submission)
                   (= (plist-get receipt :scale) 2))
          (let ((width (plist-get receipt :width)))
            (pcase native-presented-stage
              ('startup
               (when (= width 745)
                 (setq native-presented-submission (plist-get receipt :submission)
                       native-presented-stage 'resized)
                 (set-frame-width nil 91)))
              ('resized
               (when (= width 844)
                 (setq native-presented-submission (plist-get receipt :submission)
                       native-presented-stage 'fullscreen)
                 (set-frame-parameter nil 'fullscreen 'fullboth)))
              ('fullscreen
               (when (= width 3840)
                 (setq native-presented-submission (plist-get receipt :submission)
                       native-presented-stage 'restored)
                 (set-frame-parameter nil 'fullscreen nil)))
              ('restored
               (when (= width 844)
                 (unless (= (frame-width) 91)
                   (error "Restored presented frame must have 91 columns"))
                 (message "Confirmed scale-2 presentation across startup, resize, fullscreen and restoration: %S" receipt)
                 (cancel-timer native-presented-timer)
                 (kill-emacs 0)))))))
    (error (message "Presentation regression: %S" err) (kill-emacs 1))))

;; Poll a protocol receipt, not elapsed startup time. No stage advances merely
;; because a timer fired or the window's requested dimensions changed.
(setq native-presented-timer (run-at-time 0 0.02 #'native-presented-advance))
