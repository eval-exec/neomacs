;;; cursor-rtl.el --- Coherent RTL replacement cursor geometry -*- lexical-binding: t; -*-

(defun neo-rtl-fail (err)
  "Report ERR and terminate the isolated GUI process."
  (princ (format "COMMIT-EDITING-FAIL %S\n" err) 'external-debugging-output)
  (kill-emacs 1))

(defun neo-rtl-stats-size ()
  "Require Neomacs telemetry, returning nil only for the GNU reference."
  (when (fboundp 'neomacs--frame-snapshot)
    (let* ((path (getenv "NEOMACS_LAYOUT_STATS_FILE"))
           (attributes (and path (file-attributes path))))
      (unless attributes (error "Missing layout telemetry: %S" path))
      (file-attribute-size attributes))))

(run-at-time 10 nil (lambda () (neo-rtl-fail "watchdog expired")))
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
 0.5 nil
 (lambda ()
   (condition-case err
       (progn
         (redisplay t)
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
                 (princ (format "CURSOR-REPLAY-STATS %d %d\n" before
                                (neo-rtl-stats-size))
                        'external-debugging-output)
               (princ "CURSOR-REPLAY-REFERENCE-GEOMETRY-ONLY\n" 'external-debugging-output)))
           (princ "COMMIT-EDITING-PASS\n" 'external-debugging-output)
           (kill-emacs 0)))
     (error (neo-rtl-fail err)))))
