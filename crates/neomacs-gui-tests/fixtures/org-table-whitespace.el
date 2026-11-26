;;; org-table-whitespace.el --- Synthetic table probe -*- lexical-binding: t; -*-
;;; No third-party package, SQL, or user file is loaded.
;; Markers go to stderr first, so a failing file write cannot hide them, and
;; then to NEOMACS_COMMIT_RESULT_FILE, which is the copy the harness reads.
;; Render-thread stderr diagnostics can tear a line written char by char.
;; `create-lockfiles' is nil so two concurrent runs can never meet a lock.
(defun neo-org-probe-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-org-probe-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-org-probe-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))
;; The Rust harness's 60 s process timeout is the single hang-avoidance cap.

(defun neo-org-probe-when-displayed (continuation)
  "Redisplay until the table is on screen, then CONTINUATION."
  (condition-case err
      (progn
        (redisplay t)
        (if (and (window-cursor-info) (pos-visible-in-window-p (point-max)))
            (funcall continuation)
          (run-at-time 0.05 nil #'neo-org-probe-when-displayed continuation)))
    (error (neo-org-probe-fail err))))
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
       0 nil #'neo-org-probe-when-displayed
       (lambda ()
         (condition-case error-data
             (progn
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
                     (neo-org-probe-emit "ORG-TABLE-RENDERED-FACES-PASS"))
                 ;; This is explicitly source-only, not a GNU face-color oracle.
                 (progn
                   (unless (equal (get-text-property 1 'display) '(space :width (3)))
                     (error "Missing reference table-bar display property"))
                   (neo-org-probe-emit "ORG-TABLE-REFERENCE-SOURCE-ONLY")))
               (neo-org-probe-emit "COMMIT-EDITING-PASS")
               (kill-emacs 0))
           (error (neo-org-probe-fail error-data))))))
  (error (neo-org-probe-fail error-data)))
