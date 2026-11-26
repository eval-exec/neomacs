;;; cursor-rtl.el --- Coherent RTL replacement cursor geometry -*- lexical-binding: t; -*-

;; Markers go to stderr first, so a failing file write cannot hide them, and
;; then to NEOMACS_COMMIT_RESULT_FILE, which is the copy the harness reads.
;; Render-thread stderr diagnostics can tear a line written char by char.
;; `create-lockfiles' is nil so two concurrent runs can never meet a lock.
(defun neo-rtl-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-rtl-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-rtl-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-rtl-telemetry-ready-p ()
  "Non-nil once layout telemetry can be measured.
GNU has none.  On Neomacs the file exists, or its directory is absent so it
never can, and the stats read then fails explicitly."
  (or (not (fboundp 'neomacs--frame-snapshot))
      (let ((path (getenv "NEOMACS_LAYOUT_STATS_FILE")))
        (or (null path)
            (file-exists-p path)
            (not (file-directory-p (file-name-directory path)))))))

(defvar neo-rtl-ready nil "Non-nil once the readiness poll has passed.")

;; An explicit diagnostic below the harness's 60 s hang cap: a regression that
;; never creates the telemetry file fails here instead of timing out.
(run-at-time 45 nil
             (lambda ()
               (unless neo-rtl-ready
                 (neo-rtl-fail
                  (list 'error "Missing layout telemetry: never created"
                        (getenv "NEOMACS_LAYOUT_STATS_FILE"))))))

(defun neo-rtl-when-ready (continuation)
  "Redisplay until cursor geometry and telemetry are ready, then CONTINUATION."
  (condition-case err
      (progn
        (redisplay t)
        (if (not (and (window-cursor-info) (neo-rtl-telemetry-ready-p)))
            (run-at-time 0.05 nil #'neo-rtl-when-ready continuation)
          (setq neo-rtl-ready t)
          (funcall continuation)))
    (error (neo-rtl-fail err))))

(defun neo-rtl-stats-size ()
  "Require Neomacs telemetry, returning nil only for the GNU reference."
  (when (fboundp 'neomacs--frame-snapshot)
    (let* ((path (getenv "NEOMACS_LAYOUT_STATS_FILE"))
           (attributes (and path (file-attributes path))))
      (unless attributes (error "Missing layout telemetry: %S" path))
      (file-attribute-size attributes))))

(switch-to-buffer (get-buffer-create "*rtl-cursor*"))
(fundamental-mode)
(blink-cursor-mode -1)
(insert "AAA\nnext\n")
;; Logical index zero is tall but appears after the small glyphs visually.
(put-text-property 1 4 'display
                   (concat (propertize "א" 'face '(:height 2.0))
                           (propertize "בג" 'face '(:height 0.5))))
(goto-char 1)
(run-at-time
 0 nil #'neo-rtl-when-ready
 (lambda ()
   (condition-case err
       (progn
         (let* ((cold (window-cursor-info))
                (height (- (cdr (posn-x-y (posn-at-point 5))) (aref cold 2))))
           (unless (= (aref cold 4) height)
             (error "RTL cursor must use logical first glyph's full height: %S/%S" cold height))
           (goto-char 4)
           (redisplay t)
           (let ((before (neo-rtl-stats-size)))
             (goto-char 1)
             (redisplay t)
             (unless (equal (window-cursor-info) cold)
               (error "RTL cursor changed under replay: %S/%S" cold (window-cursor-info)))
             (if before
                 (neo-rtl-emit (format "CURSOR-REPLAY-STATS %d %d" before
                                       (neo-rtl-stats-size)))
               (neo-rtl-emit "CURSOR-REPLAY-REFERENCE-GEOMETRY-ONLY")))
           (neo-rtl-emit "COMMIT-EDITING-PASS")
           (kill-emacs 0)))
     (error (neo-rtl-fail err)))))
