;;; commit-editing.el --- Commit-buffer input and separator regressions -*- lexical-binding: t; -*-

(defvar neo-commit-old-map-count 0)

(defun neo-commit-probe-finish ()
  (condition-case err
      (progn
        (if (equal (getenv "NEOMACS_COMMIT_PROBE") "async-map")
            (progn
              (unless (= neo-commit-old-map-count 0)
                (error "First commit key used the previous buffer's map"))
              (unless (equal (with-current-buffer "*commit-message*" (buffer-string)) "aa")
                (error "Commit buffer did not receive both characters")))
          (redisplay t)
          (let* ((summary-y (cdr (posn-x-y (posn-at-point 1))))
                 (separator-y (cdr (posn-x-y (posn-at-point 11))))
                 (body-y (cdr (posn-x-y (posn-at-point 12))))
                 (normal-height (- separator-y summary-y))
                 (separator-height (- body-y separator-y)))
            (unless (and (> separator-height 0)
                         (< separator-height (/ normal-height 2.0)))
              (error "Separator is not short: normal=%S separator=%S"
                     normal-height separator-height))))
        (princ "COMMIT-EDITING-PASS\n" 'external-debugging-output)
        (kill-emacs 0))
    (error
     (princ (format "COMMIT-EDITING-FAIL %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))

(if (equal (getenv "NEOMACS_COMMIT_PROBE") "async-map")
    (progn
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
                     (setq unread-command-events (list ?a ?a)))))
  (require 'log-edit)
  (switch-to-buffer (get-buffer-create "*commit-separator*"))
  (log-edit-mode)
  (insert "Summary: \n\nbody\n")
  (font-lock-ensure)
  (goto-char 10))

(run-at-time 2 nil #'neo-commit-probe-finish)
