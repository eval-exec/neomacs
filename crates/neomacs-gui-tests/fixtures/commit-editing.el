;;; commit-editing.el --- Commit-buffer input and separator regressions -*- lexical-binding: t; -*-

(defvar neo-commit-old-map-count 0)
(defconst neo-commit-async-probe
  (member (getenv "NEOMACS_COMMIT_PROBE") '("async-map" "async-map-delayed"))
  "Non-nil when this fixture checks asynchronous command dispatch.")
(defconst neo-commit-overlay-probe
  (equal (getenv "NEOMACS_COMMIT_PROBE") "overlay-separator")
  "Non-nil when the small newline belongs to an overlay string.")

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
        (princ "COMMIT-EDITING-PASS\n" 'external-debugging-output)
        (kill-emacs 0))
    (error
     (princ (format "COMMIT-EDITING-FAIL %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))

;; Install the watchdog before setup can enter any input wait.
(run-at-time 10 nil
             (lambda ()
               (princ "COMMIT-EDITING-FAIL watchdog expired\n" 'external-debugging-output)
               (kill-emacs 1)))

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

(unless neo-commit-async-probe
  (run-at-time 2 nil #'neo-commit-probe-finish))

;; Reproduce startup slow enough to expire both timers in the old fixture,
;; before the command loop could dispatch either queued character.
(when (equal (getenv "NEOMACS_COMMIT_PROBE") "async-map-delayed")
  (let ((deadline (+ (float-time) 3)))
    (while (< (float-time) deadline))))
