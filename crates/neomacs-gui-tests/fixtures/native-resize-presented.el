;;; native-resize-presented.el --- Fullscreen after compositor feedback -*- lexical-binding: t -*-

(defvar native-presented-stage 'startup)
(defvar native-presented-submission 0)
(defvar native-presented-timer nil)

(defun native-presented-enter-stage (stage receipt)
  ;; A slow software compositor still owes one confirmed presentation for
  ;; each transition.  No stage carries its own wall-clock deadline: the next
  ;; stage starts only from the receipt, and the Rust harness's process
  ;; timeout bounds the whole run.
  (setq native-presented-stage stage
        native-presented-submission (plist-get receipt :submission)))

(defvar native-presented-last-state nil)

(defun native-presented-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer
        (insert-file-contents file)
        (read (current-buffer))))))

(defun native-presented-log-state (receipt)
  "Publish the observed presentation state when it changes."
  (let* ((expected (pcase native-presented-stage
                     ('startup 832) ('resized 942)
                     ('fullscreen 3840) ('restored 942)))
         (line (format "NATIVE-PRESENTED-STATE stage=%s width=%S expected=%S submission=%S"
                       native-presented-stage
                       (plist-get receipt :width)
                       expected
                       (plist-get receipt :submission))))
    (unless (equal line native-presented-last-state)
      (setq native-presented-last-state line)
      (princ (concat line "\n") 'external-debugging-output))))

(defun native-presented-advance ()
  (condition-case err
      (let ((receipt (native-presented-receipt)))
        (native-presented-log-state receipt)
        (when receipt
          (unless (and (eq (plist-get receipt :outcome) 'presented)
                       (integerp (plist-get receipt :clock-id))
                       (natnump (plist-get receipt :seconds))
                       (natnump (plist-get receipt :nanoseconds))
                       (< (plist-get receipt :nanoseconds) 1000000000))
            (error "Confirmed presentation must carry its compositor-clock timestamp: %S" receipt)))
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
