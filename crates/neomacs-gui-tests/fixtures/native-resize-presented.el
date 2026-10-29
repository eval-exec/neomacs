;;; native-resize-presented.el --- Fullscreen after compositor feedback -*- lexical-binding: t -*-

(defvar native-presented-deadline (+ (float-time) 12))
(defvar native-presented-stage 'startup)
(defvar native-presented-submission 0)
(defvar native-presented-timer nil)

(defun native-presented-enter-stage (stage receipt)
  ;; A slow software compositor still owes one confirmed presentation for
  ;; each transition. Time spent in earlier stages must not consume the
  ;; restoration budget; the Rust harness separately bounds the whole run.
  (setq native-presented-stage stage
        native-presented-submission (plist-get receipt :submission)
        native-presented-deadline (+ (float-time) 12)))

(defun native-presented-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer
        (insert-file-contents file)
        (read (current-buffer))))))

(defun native-presented-advance ()
  (condition-case err
      (let ((receipt (native-presented-receipt)))
        (when receipt
          (unless (and (eq (plist-get receipt :outcome) 'presented)
                       (integerp (plist-get receipt :clock-id))
                       (natnump (plist-get receipt :seconds))
                       (natnump (plist-get receipt :nanoseconds))
                       (< (plist-get receipt :nanoseconds) 1000000000))
            (error "Confirmed presentation must carry its compositor-clock timestamp: %S" receipt)))
        (when (> (float-time) native-presented-deadline)
          ;; Name the width this stage is waiting for: a receipt that was
          ;; delivered at a different width is a metric/predicate mismatch,
          ;; not a missing presentation.
          (error "No compositor-confirmed presentation in stage %S (expected width %S); last receipt %S"
                 native-presented-stage
                 (pcase native-presented-stage
                   ('startup 832)
                   ('resized 942)
                   ('fullscreen 3840)
                   ('restored 942))
                 receipt))
        (when (and receipt
                   (> (plist-get receipt :submission) native-presented-submission)
                   (= (plist-get receipt :scale) 2))
          (let ((width (plist-get receipt :width)))
            (pcase native-presented-stage
              ('startup
               ;; 80 columns of the pinned fonts-ubuntu 0.869 Ubuntu Mono 13
               ;; cell (10 px), two 8-pixel fringes and GNU's 16-pixel toolkit
               ;; scroll bar (src/xfns.c:2393-2438); the pre-0.869 9-pixel
               ;; cell gave 752.
               (when (= width 832)
                 (native-presented-enter-stage 'resized receipt)
                 (set-frame-width nil 91)))
              ('resized
               ;; 91 columns of the same 10-pixel cell plus the same chrome.
               (when (= width 942)
                 (native-presented-enter-stage 'fullscreen receipt)
                 (set-frame-parameter nil 'fullscreen 'fullboth)))
              ('fullscreen
               (when (= width 3840)
                 (native-presented-enter-stage 'restored receipt)
                 (set-frame-parameter nil 'fullscreen nil)))
              ('restored
               (when (= width 942)
                 (unless (= (frame-width) 91)
                   (error "Restored presented frame must have 91 columns"))
                 (message "Confirmed scale-2 presentation across startup, resize, fullscreen and restoration: %S" receipt)
                 (cancel-timer native-presented-timer)
                 (kill-emacs 0)))))))
    (error
     (princ (format "PRESENTATION-REGRESSION %S\n" err) 'external-debugging-output)
     (message "Presentation regression: %S" err)
     (kill-emacs 1))))

;; Poll a protocol receipt, not elapsed startup time. No stage advances merely
;; because a timer fired or the window's requested dimensions changed.
(setq native-presented-timer (run-at-time 0 0.02 #'native-presented-advance))
