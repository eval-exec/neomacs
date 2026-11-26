;;; treesit-debug-mode-line.el --- Issue #543 -*- lexical-binding: t -*-

(defvar treesit-debug-last-state nil)
(defvar treesit-debug-timer nil)

(defun treesit-debug-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer (insert-file-contents file) (read (current-buffer))))))

(defun treesit-debug-presented (after)
  (or (not (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT"))
      (let ((receipt (treesit-debug-receipt)))
        (and receipt
             (eq (plist-get receipt :outcome) 'presented)
             (> (plist-get receipt :submission) after)
             ;; The readback image and the matrix must be the resized
             ;; surface, not an earlier 80-column presentation.
             (= (plist-get receipt :width) (frame-outer-width))
             (integerp (plist-get receipt :clock-id))
             (natnump (plist-get receipt :seconds))
             (natnump (plist-get receipt :nanoseconds))
             (< (plist-get receipt :nanoseconds) 1000000000)))))

(defun treesit-debug-log-state (submission)
  "Publish the observed frame/presentation state when it changes."
  (let* ((receipt (treesit-debug-receipt))
         (line (format
                "TREESIT-DEBUG-STATE stage=mode-line columns=%S width=%S outer=%S height=%S min-columns=%S presentation-submission=%S submission=%S"
                (frame-width) (frame-pixel-width) (frame-outer-width)
                (frame-pixel-height) treesit-debug-min-columns
                (plist-get receipt :submission) submission)))
    (unless (equal line treesit-debug-last-state)
      (setq treesit-debug-last-state line)
      (princ (concat line "\n") 'external-debugging-output))))

;; The mode line string is 118 `a's followed by "…END": the frame only has
;; to be wide enough to show those 122 columns in full.  The tile mode of the
;; private Sway session chooses the surface extent (1800x900 output), so the
;; requested 180 columns become whatever the compositor assigns; requiring 180
;; waits forever when Sway tiles the window wider or narrower than the request.
(defconst treesit-debug-min-columns 122)

(defun treesit-debug-finish ()
  (redisplay t)
  (with-temp-file (getenv "NEOMACS_TEST_TREESIT_READY") (insert "ready"))
  (when (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON")
    (neomacs--write-frame-snapshot
     (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
    (run-at-time
     1 nil
     (lambda ()
       (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
                  (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".png") t)
       (kill-emacs 0)))))

(defun treesit-debug-wait (submission)
  "Wait for a wide frame and its next native presentation.
The mode line is laid out at the frame width that was actually presented;
under load a bare redisplay can race the compositor configure and capture
the initial 80-column width, so the snapshot waits for both the settled
width and a newer presented surface instead of a sleep."
  (setq treesit-debug-timer
        (run-at-time
         0 0.02
         (lambda ()
           (treesit-debug-log-state submission)
           (when (and (>= (frame-width) treesit-debug-min-columns)
                      (treesit-debug-presented submission))
             (cancel-timer treesit-debug-timer)
             (setq treesit-debug-timer nil)
             (treesit-debug-finish))))))

(run-at-time
 1 nil
 (lambda ()
   (condition-case err
       (progn
         (require 'treesit)
         (setq user-emacs-directory (getenv "NEOMACS_TEST_GRAMMAR_USER_DIR")
               treesit-extra-load-path nil)
         (switch-to-buffer (get-buffer-create "issue543-go"))
         (erase-buffer)
         (insert "package main\nfunc main() {}\n")
         (let ((root (treesit-parser-root-node (treesit-parser-create 'go))))
           (unless (and (equal (treesit-node-type root) "source_file")
                        (not (treesit-node-check root 'has-error)))
             (error "Go grammar did not parse")))
         (goto-char (point-max))
         (insert "// Parsed go: source_file\n")
         (goto-char (point-min))
         (when (display-graphic-p) (set-frame-font "DejaVu Sans Mono-10" nil t))
         (set-frame-size nil 180 30)
         (set-face-attribute 'mode-line nil :foreground "black" :background "white" :box nil)
         (setq-local mode-line-format (list (concat (make-string 118 ?a) "…END")))
         ;; Capture the last presented submission before this resize/mode-line
         ;; change, force the redisplay, then wait for a newer presentation.
         (let ((submission (or (plist-get (treesit-debug-receipt) :submission) 0)))
           (redisplay t)
           (treesit-debug-wait submission)))
     (error (message "Issue 543: %S" err) (kill-emacs 1)))))
