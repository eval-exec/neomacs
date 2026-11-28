;;; tall-point-row-scroll.el --- #861 tall point-row scroll parity -*- lexical-binding: t; -*-

;; State-gated GNU 31.1 vs Neomacs oracle fixture.
;; Measures `window-start', `posn-at-point', and `window-cursor-info' when point
;; moves onto or edits a partially visible bottom row that is taller than (or
;; equal to) the window's text area across GNU's full xdisp.c decision surface:
;;   - default policy (sc=0, ss=0) cursor-only at lines 2, 3, 6 (+ horiz moves)
;;   - default policy non-cursor-only edit at line 2
;;   - scroll-conservatively = 1, 2 (l6 & l7), 101 (l6, w16 l10 + horiz, edit)
;;   - scroll-step = 2 (l7), 20 (l3 overshoot -> recenter)
;;   - non-BOL point on tall rows (col 3: def-l2, def-l3, w16-sc101-l10)
;;   - non-cursor-only edit with sc=1 (l3) and sc=101 (w16 l10) -> extra=0
;;   - scroll-margin=2 overshoot (m2-w16-sc101-l10) and horiz stability (m1)
;;   - tall row at ZV with no trailing newline (tall-at-zv-l2)
;;   - exact window-body-height row on cursor-only motion (eqh-119-cursor-only)
;;   - non-uniform row height above tall row (nonunif-def-l6)
;;   - mixed-height tall row cursor on short char (mixed-w10-l3-on-Z)
;;   - top-vscrolled tall row cursor publication (vscr20-l1)
;;   - scroll-conservatively + larger scroll-step (sc2-ss5-l7)
;;   - make-cursor-line-fully-visible = nil (mclfv-nil-l2)
;;   - header-line vpos > 0 on body row 0 (hdr-start5-edit)
;;   - paint-proof magenta cursor clip check (Neomacs surface readback PNG)

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
         (ci-x (and (vectorp ci) (aref ci 1)))
         (ci-y (and (vectorp ci) (aref ci 2)))
         (ci-w (and (vectorp ci) (aref ci 3)))
         (ci-h (and (vectorp ci) (aref ci 4)))
         (ci-a (and (vectorp ci) (aref ci 5))))
    (neo-tpr-emit
     (format "TALL-POINT-ROW %s %d %d %d %d %S %S %S %S %S %S"
             label
             (line-number-at-pos (window-start w))
             (line-number-at-pos (point))
             (window-body-height w t)
             lh
             py
             ci-x
             ci-y
             ci-w
             ci-h
             ci-a))))

(defun neo-tpr-setup (label sc ss tall-line tall-scale win-lines
                            &optional margin hdr-line)
  "Create a split window of WIN-LINES with a tall row at TALL-LINE."
  (let ((buf (get-buffer-create (format "*tpr-%s*" label))))
    (delete-other-windows)
    (split-window-below win-lines)
    (switch-to-buffer buf)
    (erase-buffer)
    (setq-local scroll-conservatively sc)
    (setq-local scroll-step ss)
    (setq-local scroll-margin (or margin 0))
    (setq-local truncate-lines t)
    (setq-local make-cursor-line-fully-visible t)
    (setq-local header-line-format hdr-line)
    (dotimes (i 40)
      (let ((ln (1+ i)))
        (if (= ln tall-line)
            (insert (propertize (format "TALL %02d abc\n" ln)
                                'face `(:height ,tall-scale)))
          (insert (format "line %02d\n" ln)))))
    (goto-char (point-min))
    (set-window-start (selected-window) (point-min))
    (redisplay t)))

(defun neo-tpr-move-onto (label sc ss tall-line tall-scale win-lines
                                &optional margin)
  "Settle on the row before TALL-LINE, then move onto TALL-LINE and record."
  (neo-tpr-setup label sc ss tall-line tall-scale win-lines margin)
  (when (> tall-line 1)
    (goto-char (point-min))
    (forward-line (- tall-line 2))
    (redisplay t))
  (goto-char (point-min))
  (forward-line (1- tall-line))
  (redisplay t)
  (neo-tpr-record label))

(defun neo-tpr-move-onto-col (label sc ss tall-line tall-scale win-lines col)
  "Settle on row before TALL-LINE at COL, then move to TALL-LINE at COL."
  (neo-tpr-setup label sc ss tall-line tall-scale win-lines)
  (when (> tall-line 1)
    (goto-char (point-min))
    (forward-line (- tall-line 2))
    (forward-char col)
    (redisplay t))
  (goto-char (point-min))
  (forward-line (1- tall-line))
  (forward-char col)
  (redisplay t)
  (neo-tpr-record label))

(defvar neo-tpr-readback-deadline 0)

(defun neo-tpr-await-readback ()
  "Wait for NEOMACS_DEBUG_SURFACE_READBACK_PNG to be written, then exit."
  (condition-case err
      (let* ((png (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG"))
             (written (and png
                           (file-regular-p png)
                           (> (file-attribute-size (file-attributes png)) 0))))
        (cond
         ((or (null png) written) (kill-emacs 0))
         ((> (float-time) neo-tpr-readback-deadline)
          (neo-tpr-emit (format "COMMIT-EDITING-FAIL readback-timeout %S" png))
          (kill-emacs 1))
         (t (run-at-time 0.02 nil #'neo-tpr-await-readback))))
    (error
     (ignore-errors (neo-tpr-emit (format "COMMIT-EDITING-FAIL %S" err)))
     (kill-emacs 1))))

(defun neo-tpr-run ()
  "Run the #861 observation matrix synchronously from `window-setup-hook'."
  (condition-case err
      (progn
        (redisplay t)
        ;; --- Original 14 cases ---
        (neo-tpr-move-onto "def-l2" 0 0 2 10.0 8)
        ;; Horizontal motion on settled vpos==0 tall row scrolled off BEGV (F5).
        (forward-char 1)
        (redisplay t)
        (neo-tpr-record "def-l2-horiz")

        (neo-tpr-move-onto "def-l3" 0 0 3 10.0 8)
        (neo-tpr-move-onto "def-l6" 0 0 6 10.0 8)
        (forward-char 1)
        (redisplay t)
        (neo-tpr-record "def-l6-horiz")

        (neo-tpr-setup "edit-l2" 0 0 2 10.0 8)
        (goto-char (point-min))
        (forward-line 1)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "edit-l2")

        (neo-tpr-move-onto "sc1-l6" 1 0 6 10.0 8)
        (neo-tpr-move-onto "sc2-l6" 2 0 6 10.0 8)
        (neo-tpr-move-onto "sc2-l7" 2 0 7 10.0 8)
        (neo-tpr-move-onto "ss2-l7" 0 2 7 10.0 8)
        (neo-tpr-move-onto "sc101-l6" 101 0 6 10.0 8)

        (neo-tpr-move-onto "w16-sc101-l10" 101 0 10 18.0 16)
        (forward-char 1)
        (redisplay t)
        (neo-tpr-record "w16-sc101-l10-horiz")
        (neo-tpr-move-onto "w16-def-l10" 0 0 10 18.0 16)

        (neo-tpr-setup "edit-l6-sc101" 101 0 6 10.0 8)
        (goto-char (point-min))
        (forward-line 5)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "edit-l6-sc101")

        ;; --- F1.1: Non-BOL point on tall rows (prefix tall glyphs -> recenter) ---
        (neo-tpr-move-onto-col "col3-def-l2" 0 0 2 10.0 8 3)
        (neo-tpr-move-onto-col "col3-def-l3" 0 0 3 10.0 8 3)
        (neo-tpr-move-onto-col "col3-w16-sc101-l10" 101 0 10 18.0 16 3)

        ;; --- F1.2: Non-cursor-only edit starts at extra=0 (last_line_misfit=false) ---
        (neo-tpr-setup "edit-l3-sc1" 1 0 3 10.0 8)
        (goto-char (point-min))
        (forward-line 2)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "edit-l3-sc1")

        (neo-tpr-setup "w16-edit-l10-sc101" 101 0 10 18.0 16)
        (goto-char (point-min))
        (forward-line 9)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "w16-edit-l10-sc101")

        ;; --- F1.3: Overshoot past PT -> SCROLLING_FAILED -> recenter ---
        (neo-tpr-move-onto "ss20-l3" 0 20 3 10.0 8)
        (neo-tpr-move-onto "m2-w16-sc101-l10" 101 0 10 18.0 16 2)

        ;; --- F1.4: Tall row at ZV with no trailing newline -> no scroll ---
        (let ((buf (get-buffer-create "*tpr-tall-at-zv*")))
          (delete-other-windows)
          (split-window-below 8)
          (switch-to-buffer buf)
          (erase-buffer)
          (setq-local scroll-conservatively 0 scroll-step 0 scroll-margin 0 truncate-lines t)
          (insert "line 01\n")
          (insert (propertize "TALL" 'face '(:height 10.0)))
          (goto-char (point-min))
          (set-window-start (selected-window) (point-min))
          (redisplay t)
          (forward-line 1)
          (redisplay t)
          (neo-tpr-record "tall-at-zv-l2"))

        ;; --- F1.5: Exact body height on cursor-only motion -> no scroll ---
        (let ((buf (get-buffer-create "*tpr-eqh*")))
          (delete-other-windows)
          (split-window-below 8)
          (switch-to-buffer buf)
          (erase-buffer)
          (setq-local scroll-conservatively 0 scroll-step 0 scroll-margin 0 truncate-lines t)
          (redisplay t)
          (let ((bh (window-body-height (selected-window) t)))
            (insert "line 01\n")
            (insert (propertize "X" 'display `(space :width 1 :height (,bh))) "\n")
            (dotimes (i 20) (insert (format "after %02d\n" (1+ i)))))
          (goto-char (point-min))
          (set-window-start (selected-window) (point-min))
          (redisplay t)
          (forward-line 1)
          (redisplay t)
          (neo-tpr-record "eqh-119-cursor-only"))

        ;; --- F2: Non-uniform row height above tall row (:height 2.0 on line 5) ---
        (let ((buf (get-buffer-create "*tpr-nonunif*")))
          (delete-other-windows)
          (split-window-below 8)
          (switch-to-buffer buf)
          (erase-buffer)
          (setq-local scroll-conservatively 0 scroll-step 0 scroll-margin 0 truncate-lines t)
          (dotimes (i 30)
            (let ((ln (1+ i)))
              (cond
               ((= ln 5)
                (insert (propertize "DOUBLE 05\n" 'face '(:height 2.0))))
               ((= ln 6)
                (insert (propertize "TALL 06 abc\n" 'face '(:height 10.0))))
               (t (insert (format "line %02d\n" ln))))))
          (goto-char (point-min))
          (set-window-start (selected-window) (point-min))
          (redisplay t)
          (forward-line 4)
          (redisplay t)
          (forward-line 1)
          (redisplay t)
          (neo-tpr-record "nonunif-def-l6"))

        ;; --- F3: Mixed-height tall row cursor on short char 'Z' ---
        (let ((buf (get-buffer-create "*tpr-mixed-Z*")))
          (delete-other-windows)
          (split-window-below 10)
          (switch-to-buffer buf)
          (erase-buffer)
          (setq-local scroll-conservatively 0 scroll-step 0 scroll-margin 0 truncate-lines t)
          (insert "line 01\nline 02\n")
          (insert "Z" (propertize "TALL 03 abc\n" 'face '(:height 10.0)))
          (dotimes (i 20) (insert (format "after %02d\n" (1+ i))))
          (goto-char (point-min))
          (set-window-start (selected-window) (point-min))
          (redisplay t)
          (goto-char (point-min))
          (forward-line 2)
          (insert "")
          ;; Non-cursor-only recenter keeps start=1; point sits on short 'Z'
          ;; inside 164px row in 153px body -> cursor MUST still publish.
          (set-window-start (selected-window) (point-min) nil)
          (redisplay t)
          (neo-tpr-record "mixed-w10-l3-on-Z"))

        ;; --- F3 + F8: Top-vscrolled tall row cursor publication ---
        (neo-tpr-setup "vscr20-l1" 0 0 1 10.0 8)
        (goto-char (point-min))
        (set-window-vscroll (selected-window) 20 t)
        (redisplay t)
        (neo-tpr-record "vscr20-l1")

        ;; --- F4.1: scroll-conservatively + larger scroll-step ---
        (neo-tpr-move-onto "sc2-ss5-l7" 2 5 7 10.0 8)

        ;; --- F4.2: make-cursor-line-fully-visible = nil ---
        (neo-tpr-setup "mclfv-nil-l2" 0 0 2 10.0 8)
        (setq-local make-cursor-line-fully-visible nil)
        (goto-char (point-min))
        (redisplay t)
        (forward-line 1)
        (redisplay t)
        (neo-tpr-record "mclfv-nil-l2")

        ;; --- F4.3: header-line vpos > 0 on body row 0 -> backward recenter ---
        (neo-tpr-setup "hdr-start5-edit" 0 0 5 10.0 8 0 "HDR")
        (goto-char (point-min))
        (forward-line 4)
        (set-window-start (selected-window) (point))
        (redisplay t)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "hdr-start5-edit")

        ;; --- R2-4(a): header-line + sc > 0 on body row 0 and overshoot ---
        (neo-tpr-setup "hdr-start5-edit-sc101" 101 0 5 10.0 8 0 "HDR")
        (goto-char (point-min))
        (forward-line 4)
        (set-window-start (selected-window) (point))
        (redisplay t)
        (insert "Z")
        (redisplay t)
        (neo-tpr-record "hdr-start5-edit-sc101")

        (neo-tpr-setup "hdr-l4-sc101" 101 0 4 10.0 8 0 "HDR")
        (goto-char (point-min))
        (forward-line 2)
        (redisplay t)
        (goto-char (point-min))
        (forward-line 3)
        (redisplay t)
        (neo-tpr-record "hdr-l4-sc101")

        ;; --- F5: horizontal motion on tall row with scroll-margin > 0 ---
        (neo-tpr-move-onto "m1-w16-sc101-l10" 101 0 10 18.0 16 1)
        (forward-char 1)
        (redisplay t)
        (neo-tpr-record "m1-w16-sc101-l10-horiz")

        (neo-tpr-emit "COMMIT-EDITING-PASS")

        ;; --- F3 paint-proof clip verification (Neomacs surface readback) ---
        ;; Leave default `blink-cursor-mode' enabled so the live paint proof
        ;; exercises the default blinking / retained-static path too.
        (let ((png (and (getenv "NEOMACS_TPR_PAINT_PROOF")
                        (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG"))))
          (if (not png)
              (kill-emacs 0)
            (set-face-attribute 'default nil :foreground "#101010" :background "#ffffff")
            (set-face-attribute 'mode-line nil :foreground "#101010" :background "#d0d0d0")
            (set-face-attribute 'mode-line-inactive nil :foreground "#101010" :background "#e0e0e0")
            (set-face-background 'cursor "#ff00ff")
            (neo-tpr-move-onto "paint-clip" 0 0 6 10.0 8)
            (setq-local cursor-type 'box
                        cursor-in-non-selected-windows t)
            (let ((lower (next-window))
                  (lbuf (get-buffer-create "*tpr-paint-lower*")))
              (with-current-buffer lbuf
                (erase-buffer)
                (insert "lower window body\n")
                (setq-local cursor-type nil
                            cursor-in-non-selected-windows nil))
              (set-window-buffer lower lbuf))
            (redisplay t)
            (let ((edges (window-inside-pixel-edges (selected-window))))
              (neo-tpr-emit
               (format "PAINT-CLIP-BOUNDS %d %d %d %d %d %d"
                       (nth 0 edges)
                       (nth 1 edges)
                       (nth 2 edges)
                       (nth 3 edges)
                       (frame-pixel-width)
                       (frame-pixel-height))))
            (setq neo-tpr-readback-deadline (+ (float-time) 10))
            (run-at-time 0.02 nil #'neo-tpr-await-readback))))
    (error
     (ignore-errors (neo-tpr-emit (format "COMMIT-EDITING-FAIL %S" err)))
     (kill-emacs 1))))

(defvar neo-tpr-geometry nil)
(defvar neo-tpr-stable-polls 0)
(defvar neo-tpr-settle-deadline 0)

(defun neo-tpr-settle ()
  "Wait for initial frame geometry to hold steady before running cases.
GNU GTK3 on Xvfb can still deliver initial ConfigureNotify events during
`window-setup-hook'; waiting until `(frame-pixel-width)',
`(frame-pixel-height)', and `(window-inside-pixel-edges)' settle avoids
splitting a window mid-configure."
  (condition-case err
      (let ((geometry (list (frame-pixel-width)
                            (frame-pixel-height)
                            (window-inside-pixel-edges))))
        (redisplay t)
        (if (equal geometry neo-tpr-geometry)
            (setq neo-tpr-stable-polls (1+ neo-tpr-stable-polls))
          (setq neo-tpr-geometry geometry
                neo-tpr-stable-polls 0))
        (cond
         ((>= neo-tpr-stable-polls 5) (neo-tpr-run))
         ((> (float-time) neo-tpr-settle-deadline)
          (neo-tpr-emit (format "COMMIT-EDITING-FAIL settle-timeout %S" geometry))
          (kill-emacs 1))
         (t (run-at-time 0.02 nil #'neo-tpr-settle))))
    (error
     (ignore-errors (neo-tpr-emit (format "COMMIT-EDITING-FAIL %S" err)))
     (kill-emacs 1))))

(add-hook 'window-setup-hook
          (lambda ()
            (setq neo-tpr-settle-deadline (+ (float-time) 15))
            (run-at-time 0.02 nil #'neo-tpr-settle)))
