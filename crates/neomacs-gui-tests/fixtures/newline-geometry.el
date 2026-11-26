;;; newline-geometry.el --- Newline font and replay regressions -*- lexical-binding: t; -*-

;; Markers go to stderr first, so a failing file write cannot hide them, and
;; then to NEOMACS_COMMIT_RESULT_FILE, which is the copy the harness reads.
;; Render-thread stderr diagnostics can tear a line written char by char.
;; `create-lockfiles' is nil so two concurrent runs can never meet a lock.
(defun neo-newline-probe-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-newline-probe-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-newline-probe-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-newline-probe-telemetry-ready-p ()
  "Non-nil once layout telemetry can be measured.
GNU has none.  On Neomacs the file exists, or its directory is absent so it
never can, and the stats read then fails explicitly."
  (or (not (fboundp 'neomacs--frame-snapshot))
      (let ((path (getenv "NEOMACS_LAYOUT_STATS_FILE")))
        (or (null path)
            (file-exists-p path)
            (not (file-directory-p (file-name-directory path)))))))

(defvar neo-newline-probe-ready nil "Non-nil once the readiness poll has passed.")

;; An explicit diagnostic below the harness's 60 s hang cap: a regression that
;; never creates the telemetry file fails here instead of timing out.
(run-at-time 45 nil
             (lambda ()
               (unless neo-newline-probe-ready
                 (neo-newline-probe-fail
                  (list 'error "Missing layout telemetry: never created"
                        (getenv "NEOMACS_LAYOUT_STATS_FILE"))))))

(defun neo-newline-probe-when-ready (continuation)
  "Redisplay until cursor geometry and telemetry are ready, then CONTINUATION."
  (condition-case err
      (progn
        (redisplay t)
        (if (not (and (window-cursor-info) (neo-newline-probe-telemetry-ready-p)))
            (run-at-time 0.05 nil #'neo-newline-probe-when-ready continuation)
          (setq neo-newline-probe-ready t)
          (funcall continuation)))
    (error (neo-newline-probe-fail err))))

(defun neo-newline-probe-y (position)
  "Return the redisplayed buffer POSITION's vertical pixel coordinate."
  (cdr (posn-x-y (posn-at-point position))))

(defun neo-newline-probe-stats-size ()
  "Return the layout telemetry size in Neomacs, or nil for the GNU reference."
  (when (fboundp 'neomacs--frame-snapshot)
    (let* ((path (getenv "NEOMACS_LAYOUT_STATS_FILE"))
           (attributes (and path (file-attributes path))))
      (unless attributes (error "Missing layout telemetry: %S" path))
      (file-attribute-size attributes))))

(defvar neo-newline-probe-cold-cursor nil
  "Physical cursor captured by the first mixed-height full layout.")

(defun neo-newline-probe-finish ()
  "Check physical row or cursor geometry after the queued redisplay."
  (condition-case err
      (progn
        (redisplay t)
        (let* ((probe (getenv "NEOMACS_COMMIT_PROBE"))
               (body-y (neo-newline-probe-y 6))
               (normal (- (neo-newline-probe-y 10) body-y)))
          (cond
           ((equal probe "replacement-newline")
            ;; The replaced position need not have a buffer glyph. Measure
            ;; the short row from the following body row instead.
            (let ((height (- body-y (neo-newline-probe-y 1) normal)))
              (unless (and (> height 0) (< height (/ normal 2.0)))
                (error "Replacement newline is not short: %S/%S" height normal))))
           ((equal probe "tall-overlay-newline")
            (let ((height (- body-y (neo-newline-probe-y 1) (* 2 normal))))
              (unless (> height (* normal 1.5))
                (error "Overlay newline did not grow: %S/%S" height normal))))
           ((member probe '("raised-text" "lowered-text"))
            (let ((height (- (neo-newline-probe-y 5) (neo-newline-probe-y 1)))
                  (expected (* normal 1.5)))
              (unless (<= (abs (- height expected)) 1)
                (error "Shifted text lost an opposite extent: %S/%S" height expected))))
           ((string-prefix-p "mixed-" probe)
            (let ((height (- (neo-newline-probe-y 5) (neo-newline-probe-y 1)))
                  (cursor (window-cursor-info)))
              (unless (and (> height 0) (= (aref cursor 4) height)
                           (= (aref neo-newline-probe-cold-cursor 4) height)
                           (= (aref cursor 5) (aref neo-newline-probe-cold-cursor 5)))
                (error "Mixed newline cursor must fill the row, cold and replayed: %S/%S/%S"
                       neo-newline-probe-cold-cursor cursor height))))
           ((equal probe "short-newline-cursor")
            (let ((row-height (- body-y (neo-newline-probe-y 5)))
                  (cursor-height (aref (window-cursor-info) 4)))
              (unless (and (> row-height 0) (= cursor-height row-height))
                (error "Replayed cursor differs from short spaced row: %S/%S"
                       cursor-height row-height))))
           (t (error "Unknown newline probe: %S" probe))))
        (neo-newline-probe-emit "COMMIT-EDITING-PASS")
        (kill-emacs 0))
    (error (neo-newline-probe-fail err))))

(defun neo-newline-probe-run ()
  "Run the replay geometry check once initial redisplay is ready."
  (condition-case err
      (progn
        (when (string-suffix-p "newline-cursor" (getenv "NEOMACS_COMMIT_PROBE"))
          (when (string-prefix-p "mixed-" (getenv "NEOMACS_COMMIT_PROBE"))
            (setq neo-newline-probe-cold-cursor (window-cursor-info))
            (goto-char 1)
            (redisplay t))
          ;; Retain rows first, then move only point onto the newline.
          (let ((before (neo-newline-probe-stats-size)))
            (goto-char (if neo-newline-probe-cold-cursor 4 5))
            (redisplay t)
            (if before
                (neo-newline-probe-emit
                 (format "CURSOR-REPLAY-STATS %d %d"
                         before (neo-newline-probe-stats-size)))
              (neo-newline-probe-emit "CURSOR-REPLAY-REFERENCE-GEOMETRY-ONLY"))))
        (run-at-time 0 nil #'neo-newline-probe-finish))
    (error (neo-newline-probe-fail err))))

(switch-to-buffer (get-buffer-create "*newline-geometry*"))
(fundamental-mode)
(blink-cursor-mode -1)
(insert "AAA\n\nBBB\nnext\n")
(pcase (getenv "NEOMACS_COMMIT_PROBE")
  ((or "raised-text" "lowered-text")
   (put-text-property 1 4 'display
                      (list 'raise (if (equal (getenv "NEOMACS_COMMIT_PROBE") "raised-text") 0.5 -0.5)))
   (put-text-property 4 5 'line-height t))
  ("replacement-newline"
   (put-text-property 5 6 'display
                      (propertize "\n" 'face '(:height 0.1 :inverse-video t :extend t))))
  ("tall-overlay-newline"
   (overlay-put (make-overlay 5 5) 'before-string
                (propertize "\n" 'face '(:height 2.0 :inverse-video t :extend t))))
  ("short-newline-cursor"
   (put-text-property 5 6 'face '(:height 0.1))
   (setq-local line-spacing 5)))
(when (string-prefix-p "mixed-" (getenv "NEOMACS_COMMIT_PROBE"))
  (put-text-property 1 4 'face '(:height 2.0))
  (put-text-property 4 5 'face
                     (list :height (if (string-match-p "small" (getenv "NEOMACS_COMMIT_PROBE")) 0.1 1.0)))
  (setq-local line-spacing (if (string-match-p "spaced" (getenv "NEOMACS_COMMIT_PROBE")) 5 0)))
(goto-char (if (string-prefix-p "mixed-" (getenv "NEOMACS_COMMIT_PROBE")) 4 1))
(run-at-time 0 nil #'neo-newline-probe-when-ready #'neo-newline-probe-run)
