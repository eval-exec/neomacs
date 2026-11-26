;;; cursor-stretch-policy.el --- Fresh/replay width policy -*- lexical-binding: t; -*-
;; Markers go to stderr first, so a failing file write cannot hide them, and
;; then to NEOMACS_COMMIT_RESULT_FILE, which is the copy the harness reads.
;; Render-thread stderr diagnostics can tear a line written char by char.
;; `create-lockfiles' is nil so two concurrent runs can never meet a lock.
(defun neo-stretch-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-stretch-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-stretch-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-stretch-telemetry-ready-p ()
  "Non-nil once layout telemetry can be measured.
GNU has none.  On Neomacs the file exists, or its directory is absent so it
never can, and the stats read then fails explicitly."
  (or (not (fboundp 'neomacs--frame-snapshot))
      (let ((path (getenv "NEOMACS_LAYOUT_STATS_FILE")))
        (or (null path)
            (file-exists-p path)
            (not (file-directory-p (file-name-directory path)))))))

(defvar neo-stretch-ready nil "Non-nil once the readiness poll has passed.")

;; An explicit diagnostic below the harness's 60 s hang cap: a regression that
;; never creates the telemetry file fails here instead of timing out.
(run-at-time 45 nil
             (lambda ()
               (unless neo-stretch-ready
                 (neo-stretch-fail
                  (list 'error "Missing layout telemetry: never created"
                        (getenv "NEOMACS_LAYOUT_STATS_FILE"))))))

(defun neo-stretch-when-ready (continuation)
  "Redisplay until cursor geometry and telemetry are ready, then CONTINUATION."
  (condition-case err
      (progn
        (redisplay t)
        (if (not (and (window-cursor-info) (neo-stretch-telemetry-ready-p)))
            (run-at-time 0.05 nil #'neo-stretch-when-ready continuation)
          (setq neo-stretch-ready t)
          (funcall continuation)))
    (error (neo-stretch-fail err))))

(defun neo-stretch-stats-size ()
  "Require telemetry on Neomacs; GNU is explicitly geometry-only."
  (when (fboundp 'neomacs--frame-snapshot)
    (let* ((path (getenv "NEOMACS_LAYOUT_STATS_FILE"))
           (attributes (and path (file-attributes path))))
      (unless attributes (error "Missing layout telemetry"))
      (file-attribute-size attributes))))
(condition-case error-data
    (let* ((probe (or (getenv "NEOMACS_COMMIT_PROBE") ""))
           (narrow (string-match-p "narrow" probe))
           (enabled (string-match-p "enabled" probe))
           (width (if narrow 0.25 5)))
      (setq warning-minimum-level :emergency x-stretch-cursor (and enabled t))
      (blink-cursor-mode -1)
      (set-frame-parameter nil 'cursor-type 'box)
      (switch-to-buffer (get-buffer-create "*stretch-cursor-policy*"))
      (fundamental-mode)
      (insert "אxב\n")
      (put-text-property 2 3 'display (list 'space :width width))
      (goto-char 2)
      (run-at-time
       0 nil #'neo-stretch-when-ready
       (lambda ()
         (condition-case error-data
             (progn
               (let ((cold (window-cursor-info)))
                 (goto-char 1)
                 (redisplay t)
                 (let ((before (neo-stretch-stats-size)))
                   (goto-char 2)
                   (redisplay t)
                   (let* ((after (neo-stretch-stats-size))
                          (replayed (window-cursor-info))
                          (actual (aref replayed 3))
                          (column (frame-char-width)))
                     (unless (= (aref cold 3) actual)
                       (error "Stretch width changed on replay: cold=%S replay=%S" cold replayed))
                     (unless (and (> actual 0)
                                  (if narrow (< actual column)
                                    (if enabled (> actual column) (<= actual column))))
                       (error "Invalid stretch-width policy: enabled=%S narrow=%S width=%S column=%S" enabled narrow actual column))
                     (if before
                         (neo-stretch-emit (format "CURSOR-REPLAY-STATS %d %d" before after))
                       (neo-stretch-emit "CURSOR-REPLAY-REFERENCE-GEOMETRY-ONLY"))
                     (neo-stretch-emit "COMMIT-EDITING-PASS")
                     (kill-emacs 0)))))
           (error (neo-stretch-fail error-data))))))
  (error (neo-stretch-fail error-data)))
