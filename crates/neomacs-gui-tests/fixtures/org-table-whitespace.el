;;; org-table-whitespace.el --- Synthetic table probe -*- lexical-binding: t; -*-
;;; No third-party package, SQL, or user file is loaded.
(defun neo-org-probe-fail (error-data)
  (princ (format "COMMIT-EDITING-FAIL %S\n" error-data) 'external-debugging-output)
  (kill-emacs 1))
(run-at-time 10 nil (lambda () (neo-org-probe-fail "watchdog expired")))
(condition-case error-data
    (progn
      (setq warning-minimum-level :emergency)
      (require 'org)
      (switch-to-buffer (get-buffer-create "*org-table-whitespace-probe*"))
      (insert "| label     |\n| alpha     |\n| longer    |\n\ncontrol   \n")
      (let ((enable-dir-local-variables nil)) (org-mode))
      (setq-local show-trailing-whitespace t)
      (font-lock-ensure)
      ;; org-modern--table uses exactly this display/face for vertical bars.
      (goto-char (point-min))
      (while (search-forward "|" nil t)
        (add-text-properties (1- (point)) (point)
                             '(display (space :width (3))
                               face (:inherit org-table :inverse-video t))))
      (goto-char (point-min))
      (run-at-time
       1 nil
       (lambda ()
         (condition-case error-data
             (progn
               (redisplay t)
               (if (fboundp 'neomacs--frame-snapshot)
                   (let ((snapshot (neomacs--frame-snapshot nil 'text-faces))
                         row highlighted)
                     (princ snapshot 'external-debugging-output)
                     (dolist (line (split-string snapshot "\n"))
                       (when (string-match "\\` *\\([0-9]+\\)|" line)
                         (setq row (string-to-number (match-string 1 line))))
                       (when (string-match-p "trailing-whitespace" line)
                         (push row highlighted)))
                     (when (seq-some (lambda (row) (memq row '(0 1 2))) highlighted)
                       (error "Org table border/padding was classified as trailing whitespace: %S" highlighted))
                     (unless (memq 4 highlighted)
                       (error "Genuine trailing spaces lost their highlight: %S" highlighted))
                     (princ "ORG-TABLE-RENDERED-FACES-PASS\n" 'external-debugging-output))
                 ;; This is explicitly source-only, not a GNU face-color oracle.
                 (progn
                   (unless (equal (get-text-property 1 'display) '(space :width (3)))
                     (error "Missing reference table-bar display property"))
                   (princ "ORG-TABLE-REFERENCE-SOURCE-ONLY\n" 'external-debugging-output)))
               (princ "COMMIT-EDITING-PASS\n" 'external-debugging-output)
               (kill-emacs 0))
           (error (neo-org-probe-fail error-data))))))
  (error (neo-org-probe-fail error-data)))
