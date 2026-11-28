;;; tall-point-row-scroll.el --- #861 tall point-row scroll parity -*- lexical-binding: t; -*-

;; State-gated GNU 31.1 vs Neomacs oracle fixture (zero sleeps or timers).
;; Measures `window-start', `posn-at-point', and `window-cursor-info' when point
;; moves onto a partially visible bottom row that is taller than the window's
;; text area across GNU's decision boundaries:
;;   - default policy (sc=0, ss=0) cursor-only at lines 2, 3, 6 (+ horizontal move)
;;   - default policy non-cursor-only edit at line 2
;;   - scroll-conservatively = 1, 2 (l6 & l7), 101 (l6, w16 l10 + horizontal move, edit-l6)
;;   - scroll-step = 2 (l7)

(defun neo-tpr-emit (line)
  "Write LINE to stderr and append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-tpr-record (label)
  "Emit one settled observation for LABEL."
  (let* ((w (selected-window))
         (lh (max 1 (default-line-height)))
         (p (posn-at-point nil w))
         (py (and p (cdr (posn-x-y p))))
         (ci (window-cursor-info w))
         (ci-y (and (vectorp ci) (aref ci 2))))
    (neo-tpr-emit
     (format "TALL-POINT-ROW %s %d %d %d %d %S %S"
             label
             (line-number-at-pos (window-start w))
             (line-number-at-pos (point))
             (window-body-height w t)
             lh
             py
             ci-y))))

(defun neo-tpr-setup (label sc ss tall-line tall-scale win-lines &optional vscr)
  "Create a split window of WIN-LINES with a tall row at TALL-LINE."
  (let ((buf (get-buffer-create (format "*tpr-%s*" label))))
    (delete-other-windows)
    (split-window-below win-lines)
    (switch-to-buffer buf)
    (erase-buffer)
    (setq-local scroll-conservatively sc)
    (setq-local scroll-step ss)
    (setq-local scroll-margin 0)
    (setq-local truncate-lines t)
    (dotimes (i 40)
      (let ((ln (1+ i)))
        (if (= ln tall-line)
            (insert (propertize (format "TALL %02d abc\n" ln)
                                'face `(:height ,tall-scale)))
          (insert (format "line %02d\n" ln)))))
    (goto-char (point-min))
    (set-window-start (selected-window) (point-min))
    (when vscr
      (set-window-vscroll (selected-window) vscr t))
    (redisplay t)))

(defun neo-tpr-move-onto (label sc ss tall-line tall-scale win-lines &optional vscr)
  "Settle on the row before TALL-LINE, then move onto TALL-LINE and record."
  (neo-tpr-setup label sc ss tall-line tall-scale win-lines vscr)
  (when (> tall-line 1)
    (goto-char (point-min))
    (forward-line (- tall-line 2))
    (redisplay t))
  (goto-char (point-min))
  (forward-line (1- tall-line))
  (redisplay t)
  (neo-tpr-record label))

(defun neo-tpr-run ()
  "Run the #861 observation matrix synchronously from `window-setup-hook'."
  (condition-case err
      (progn
        (redisplay t)
        ;; 8-line window (body=119 px = 7*17), tall row (:height 10.0) ~164 px > 119.
        (neo-tpr-move-onto "def-l2" 0 0 2 10.0 8)
        (neo-tpr-move-onto "def-l3" 0 0 3 10.0 8)
        (neo-tpr-move-onto "def-l6" 0 0 6 10.0 8)
        ;; Horizontal motion on the recentered tall row must stay stable.
        (forward-char 1)
        (redisplay t)
        (neo-tpr-record "def-l6-horiz")
        ;; Non-cursor-only edit on tall line 2 skips temp_scroll_step -> recenter keeps line 1.
        (neo-tpr-setup "edit-l2" 0 0 2 10.0 8)
        (goto-char (point-min))
        (forward-line 1)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "edit-l2")
        ;; Policy boundaries on 8-line window.
        (neo-tpr-move-onto "sc1-l6" 1 0 6 10.0 8)
        (neo-tpr-move-onto "sc2-l6" 2 0 6 10.0 8)
        (neo-tpr-move-onto "sc2-l7" 2 0 7 10.0 8)
        (neo-tpr-move-onto "ss2-l7" 0 2 7 10.0 8)
        (neo-tpr-move-onto "sc101-l6" 101 0 6 10.0 8)
        ;; 16-line window (body=255 px = 15*17), tall row (:height 18.0) ~295 px > 255:
        ;; GNU's extra<=1 bound stops sc=101 after 1+2+3=6 lines (start=7),
        ;; horizontal motion on that tall row must stay at start=7,
        ;; while default policy recenters using body/2 (7 lines above -> start=3).
        (neo-tpr-move-onto "w16-sc101-l10" 101 0 10 18.0 16)
        (forward-char 1)
        (redisplay t)
        (neo-tpr-record "w16-sc101-l10-horiz")
        (neo-tpr-move-onto "w16-def-l10" 0 0 10 18.0 16)
        ;; Non-cursor-only edit on tall line 6 with sc=101 still enters try_scrolling -> start=6.
        (neo-tpr-setup "edit-l6-sc101" 101 0 6 10.0 8)
        (goto-char (point-min))
        (forward-line 5)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "edit-l6-sc101")
        (neo-tpr-emit "COMMIT-EDITING-PASS")
        (kill-emacs 0))
    (error
     (ignore-errors (neo-tpr-emit (format "COMMIT-EDITING-FAIL %S" err)))
     (kill-emacs 1))))

(add-hook 'window-setup-hook #'neo-tpr-run)
