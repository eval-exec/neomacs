;;; cursor-geometry-telemetry.el --- Cursor placement log probe (#855) -*- lexical-binding: t; -*-

;; Neomacs only: the `neomacs::cursor_geometry' lines are Neomacs telemetry.
;; Point sits on the `h' of "three", the only `h' in the buffer, so the draw
;; line for the glyph under point is unambiguous. The probe waits for that
;; line to reach NEOMACS_LOG_FILE, then copies it and the matching layout line
;; to the result file for the harness to parse.
;;
;; Markers go to stderr first, so a failing file write cannot hide them, and
;; then to NEOMACS_COMMIT_RESULT_FILE, which is the copy the harness reads.
;; `create-lockfiles' is nil so two concurrent runs can never meet a lock.
(defun neo-cursor-geometry-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-cursor-geometry-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-cursor-geometry-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-cursor-geometry-log-lines (needle)
  "Return the NEOMACS_LOG_FILE lines containing NEEDLE, oldest first."
  (let ((path (getenv "NEOMACS_LOG_FILE")))
    (unless path (error "NEOMACS_LOG_FILE is unset"))
    (when (file-exists-p path)
      (with-temp-buffer
        (insert-file-contents path)
        (let (lines)
          (goto-char (point-min))
          (while (search-forward needle nil t)
            (push (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position))
                  lines)
            (forward-line 1))
          (nreverse lines))))))

(defun neo-cursor-geometry-presentation (line)
  "Return the presentation id LINE names, as a string."
  (and (string-match "presentation=\\([0-9]+\\)" line)
       (match-string 1 line)))

(defun neo-cursor-geometry-ready ()
  "Return (DRAW . LAYOUT) for the latest draw line on `h', or nil."
  (let* ((draw (car (last (neo-cursor-geometry-log-lines
                           "stage=draw presentation="))))
         (presentation (and draw
                            (string-match-p "glyph=char('h' " draw)
                            (neo-cursor-geometry-presentation draw)))
         (layout nil))
    (when presentation
      (dolist (line (neo-cursor-geometry-log-lines "stage=layout presentation="))
        (when (equal (neo-cursor-geometry-presentation line) presentation)
          (setq layout line))))
    (and layout (cons draw layout))))

(defvar neo-cursor-geometry-deadline nil
  "The `float-time' after which readiness has failed.")

(defun neo-cursor-geometry-poll ()
  "Redisplay until the log holds both stages for the cursor on `h'."
  (condition-case err
      (progn
        (redisplay t)
        (let ((lines (and (window-cursor-info) (neo-cursor-geometry-ready))))
          (cond
           (lines
            (neo-cursor-geometry-emit
             (format "CURSOR-GEOMETRY-INFO %S %S" (point) (window-cursor-info)))
            (neo-cursor-geometry-emit (concat "CURSOR-GEOMETRY-DRAW " (car lines)))
            (neo-cursor-geometry-emit (concat "CURSOR-GEOMETRY-LAYOUT " (cdr lines)))
            (neo-cursor-geometry-emit "COMMIT-EDITING-PASS")
            (kill-emacs 0))
           ((> (float-time) neo-cursor-geometry-deadline)
            (error "Readiness expired: no cursor_geometry draw and layout lines for the cursor on `h'"))
           (t (run-at-time 0.05 nil #'neo-cursor-geometry-poll)))))
    (error (neo-cursor-geometry-fail err))))

(condition-case err
    (progn
      (setq create-lockfiles nil)
      (unless (fboundp 'neomacs--frame-snapshot)
        (error "This probe reads Neomacs telemetry; run it on Neomacs"))
      (switch-to-buffer (get-buffer-create "cursor-geometry"))
      (delete-other-windows)
      (fundamental-mode)
      (insert "one\ntwo\nthree\nfour\n")
      (goto-char (point-min))
      (forward-line 2)
      (forward-char 1)
      (setq neo-cursor-geometry-deadline (+ (float-time) 45))
      (run-at-time 0.05 nil #'neo-cursor-geometry-poll))
  (error (neo-cursor-geometry-fail err)))
