;;; commit-editing.el --- Commit-buffer input and separator regressions -*- lexical-binding: t; -*-

(defvar neo-commit-old-map-count 0)
(defconst neo-commit-async-probe
  (member (getenv "NEOMACS_COMMIT_PROBE") '("async-map" "async-map-delayed"))
  "Non-nil when this fixture checks asynchronous command dispatch.")
(defconst neo-commit-overlay-probe
  (equal (getenv "NEOMACS_COMMIT_PROBE") "overlay-separator")
  "Non-nil when the small newline belongs to an overlay string.")

;; Markers go to stderr first, so a failing file write cannot hide them, and
;; then to NEOMACS_COMMIT_RESULT_FILE, which is the copy the harness reads.
;; Render-thread stderr diagnostics can tear a line written char by char.
;; `create-lockfiles' is nil so two concurrent runs can never meet a lock.
(defun neo-commit-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-commit-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-commit-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-commit-probe-finish ()
  "Check the completed input sequence or the redisplayed separator geometry."
  (interactive)
  (condition-case err
      (progn
        (if neo-commit-async-probe
            (progn
              (unless (= neo-commit-old-map-count 0)
                (error "First commit key used the previous buffer's map"))
              (unless (equal (with-current-buffer "*commit-message*" (buffer-string)) "aa")
                (error "Commit buffer did not receive both characters")))
          (redisplay t)
          (let* ((summary-y (cdr (posn-x-y (posn-at-point 1))))
                 (separator-y (cdr (posn-x-y (posn-at-point 11))))
                 (body-y (cdr (posn-x-y (posn-at-point 12))))
                 (normal-height
                  (if neo-commit-overlay-probe
                      (- (cdr (posn-x-y (posn-at-point 17))) body-y)
                    (- separator-y summary-y)))
                 (separator-height
                  (if neo-commit-overlay-probe
                      (- body-y summary-y (* 2 normal-height))
                    (- body-y separator-y))))
            (unless (and (> separator-height 0)
                         (< separator-height (/ normal-height 2.0)))
              (error "Separator is not short: normal=%S separator=%S"
                     normal-height separator-height))))
        (neo-commit-emit "COMMIT-EDITING-PASS")
        (kill-emacs 0))
    (error (neo-commit-fail err))))

;; The Rust harness's 60 s process timeout is the single hang-avoidance cap.

(if neo-commit-async-probe
    (progn
      ;; This sentinel is dispatched only after both queued characters, even
      ;; when slow startup makes multiple timers due in one service pass.
      (global-set-key [neo-commit-probe-finish] #'neo-commit-probe-finish)
      (switch-to-buffer (get-buffer-create "*commit-status*"))
      (use-local-map (make-sparse-keymap))
      (local-set-key "a" (lambda () (interactive)
                           (setq neo-commit-old-map-count (1+ neo-commit-old-map-count))))
      (with-current-buffer (get-buffer-create "*commit-message*")
        (fundamental-mode))
      ;; Like a with-editor process filter: display the new buffer, but
      ;; restore the old current buffer while the command loop awaits input.
      ;; The timer is the stimulus (an asynchronous buffer switch), not a
      ;; settle: the sentinel event below ends the probe, not this delay.
      (run-at-time 0.5 nil
                   (lambda ()
                     (save-current-buffer (switch-to-buffer "*commit-message*"))
                     (setq unread-command-events
                           (list ?a ?a 'neo-commit-probe-finish)))))
  (require 'log-edit)
  (switch-to-buffer (get-buffer-create "*commit-separator*"))
  (if neo-commit-overlay-probe
      (progn
        (fundamental-mode)
        (insert "Summary: \n\nbody\nnext\n")
        (overlay-put (make-overlay 11 11) 'before-string
                     (propertize "\n" 'face
                                 '(:height 0.1 :inverse-video t :extend t))))
    ;; Checkout-local settings must not prompt during an isolated GUI probe.
    (let ((enable-dir-local-variables nil))
      (log-edit-mode))
    (insert "Summary: \n\nbody\n")
    (font-lock-ensure))
  (goto-char 10))

(defun neo-commit-probe-when-displayed ()
  "Check separator geometry once redisplay has placed every probed position."
  (condition-case err
      (progn
        (redisplay t)
        (if (and (window-cursor-info)
                 (posn-at-point 1)
                 (posn-at-point 12)
                 (or (not neo-commit-overlay-probe) (posn-at-point 17)))
            (neo-commit-probe-finish)
          (run-at-time 0.05 nil #'neo-commit-probe-when-displayed)))
    (error (neo-commit-fail err))))

(unless neo-commit-async-probe
  (run-at-time 0 nil #'neo-commit-probe-when-displayed))

;; Reproduce startup slow enough to expire both timers in the old fixture,
;; before the command loop could dispatch either queued character.  This busy
;; wait is the stimulus of the async-map-delayed row, not a settle.
(when (equal (getenv "NEOMACS_COMMIT_PROBE") "async-map-delayed")
  (let ((deadline (+ (float-time) 3)))
    (while (< (float-time) deadline))))
