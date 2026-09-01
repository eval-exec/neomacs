;;; l216-hscroll-origin-probe.el --- one horizontal origin, seven symptoms -*- lexical-binding: t -*-

;; Ledger 216.  Ledgers 210, 211 and 212 each hit a piece of hscroll handling
;; while working on something else and each deferred it.  This probe puts all
;; SEVEN of the handed-over claims in ONE file so that the question "how many
;; distinct causes are there?" can be answered by measurement rather than by
;; reading, and so that a fix for one can be shown NOT to move the others.
;;
;; THE GNU MODEL, read before this probe was written (Emacs 31.0.90):
;;
;;   GNU has exactly ONE horizontal origin and it is established once, in
;;   `init_iterator':
;;
;;       it->first_visible_x = window_hscroll_limited (w, it->f)
;;                             * FRAME_COLUMN_WIDTH (it->f);      xdisp.c:3500
;;       it->last_visible_x  = it->first_visible_x + body_width;  xdisp.c:3507
;;       ... less the truncation/continuation glyph                xdisp.c:3510-3518
;;
;;   and the comment above it names the coordinate system outright: "The
;;   display area consists of the visible window area plus a horizontally
;;   scrolled part to the left of the window.  All x-values are relative to
;;   the start of this total display area." (xdisp.c:3473-3476).  So every
;;   iterator x in GNU is LINE-relative: `it->current_x' is 0 at the line
;;   start however far the window is scrolled, and the window's left edge sits
;;   at `first_visible_x' inside that space.
;;
;;   Every consumer therefore converts, once, in a named direction:
;;
;;     * the goal-column walk ADDS it:
;;         move_it_in_display_line (&it, ZV, first_x + to_x, MOVE_TO_X)
;;                                                            indent.c:2540
;;       with `first_x = it.first_visible_x' (indent.c:2321) -- documented in
;;       the `vertical-motion' docstring: "If the line is scrolled
;;       horizontally, COLS is interpreted visually, i.e., as addition to the
;;       columns of text beyond the left edge of the window" (indent.c:2226).
;;     * the coordinate query ADDS it:
;;         to_x += it.first_visible_x                          dispnew.c:6305
;;       "We need to add it.first_visible_x because iterator positions include
;;       the hscroll" (dispnew.c:6303).
;;     * `pos_visible_p' SUBTRACTS it, once, at the very end:
;;         if (w->hscroll > 0)
;;           *x -= window_hscroll_limited (w, ...) * ...COLUMN_WIDTH (w);
;;                                                            xdisp.c:2120-2125
;;       which is why a position hidden to the LEFT of the window has a
;;       NEGATIVE x rather than no answer, and `Fposn_at_point' then treats
;;       x = -1 as a frame posn and only x < -1 as nil (keyboard.c:13084-13086).
;;
;; Run in BOTH editors, SAME geometry, one protocol per run:
;;
;;   bash scripts/l205-audit-run.sh emacs scripts/l216-hscroll-origin-probe.el \
;;       L216_OUT ./tmp/l216/probe-gnu-cold.txt L216_REDISPLAY 0 80 24
;;   bash scripts/l205-audit-run.sh ./target/release/neomacs scripts/l216-hscroll-origin-probe.el \
;;       L216_OUT ./tmp/l216/probe-neo-cold.txt L216_REDISPLAY 0 80 24
;;   diff ./tmp/l216/probe-gnu-cold.txt ./tmp/l216/probe-neo-cold.txt
;;
;; Environment: L216_OUT, L216_REDISPLAY (0 cold / 1 warm).

(defvar l216-out (or (getenv "L216_OUT") "./tmp/l216/probe-out.txt"))
(defvar l216-redisplays (string-to-number (or (getenv "L216_REDISPLAY") "0")))
(defun l216-redisplay () (dotimes (_ l216-redisplays) (redisplay t)))

(defvar l216-lines nil)
(defun l216-say (fmt &rest args) (push (apply #'format fmt args) l216-lines))

(defun l216-pap (pos)
  "`posn-at-point' reduced to POINT and (COL . ROW).
The object cell is deliberately omitted: ledger 205 residual 1 (GNU's stale
`it.pixel_width', `(1 . 0)' against this port's `(1 . 1)') diverges on every
probe alike and would make every line of this file red for a reason that has
nothing to do with the hscroll."
  (let ((p (condition-case err (posn-at-point pos)
             (error (format "ERR:%S" (car err))))))
    (cond ((stringp p) p)
          ((null p) "nil")
          (t (format "%S" (list (posn-point p) (posn-actual-col-row p)))))))

(defun l216-pvis (pos)
  (format "%S" (condition-case err (pos-visible-in-window-p pos nil t)
                 (error (format "ERR:%S" (car err))))))

(defun l216-two-lines (buffer trunc)
  "200 `a's, newline, 200 `b's, newline.  Line 2 starts at 202."
  (switch-to-buffer buffer)
  (with-current-buffer buffer
    (erase-buffer)
    (insert (concat (make-string 200 ?a) "\n" (make-string 200 ?b) "\n"))
    (setq-local truncate-lines trunc)
    (setq-local word-wrap nil)
    (set-buffer-modified-p nil)))

;; ---------------------------------------------------------------- PART A
;; The GOAL COLUMN under hscroll (ledger 210 residual 3, ledger 211 item 3,
;; ledger 212 item 2).  GNU reaches `first_x + to_x'; a port that passes
;; `to_x' alone answers the goal in the wrong coordinate space.
;;
;; `end-of-visual-line' is `(vertical-motion (cons (window-width) 0))'
;; (lisp/simple.el), so it asks for a goal PAST the row's right edge at every
;; hscroll and its answer is the row's own edge stop: line-start + hscroll + 79
;; in an 80-column window with `truncate-lines' t.  The explicit goals below
;; separate "the goal term is missing" from "the edge term is missing":
;; a goal of 10 is INSIDE the window at every hscroll here.
(defun l216-part-a (buffer)
  (l216-two-lines buffer t)
  (l216-say "PARTA body-width=%s body-height=%s" (window-body-width) (window-body-height))
  (dolist (hs '(0 5 20 100))
    (dolist (goal '(0 10 40 79 80 200))
      (goto-char 250)
      (set-window-hscroll (selected-window) hs)
      (l216-redisplay)
      (set-window-hscroll (selected-window) hs)
      (goto-char 250)
      (let ((moved (vertical-motion (cons goal 0))))
        (l216-say "PARTA trunc hscroll=%-4s goal=%-4s -> %S" hs goal
                  (list moved (point)))))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l216-redisplay)
    (set-window-hscroll (selected-window) hs)
    (goto-char 250)
    (l216-say "PARTA trunc hscroll=%-4s eovl=%s" hs
              (progn (end-of-visual-line) (point))))
  ;; The same question with `truncate-lines' nil.  A wrapped row has no
  ;; hscroll at all in GNU (`hscroll' is only honoured for truncated lines in
  ;; practice, but `set-window-hscroll' still sets it), so this is the control
  ;; that says whether a fix keyed on the hscroll can reach a wrapping row.
  (l216-two-lines buffer nil)
  (dolist (hs '(0 5))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l216-redisplay)
    (set-window-hscroll (selected-window) hs)
    (goto-char 250)
    (l216-say "PARTA wrap  hscroll=%-4s eovl=%s" hs
              (progn (end-of-visual-line) (point)))))

;; ---------------------------------------------------------------- PART B
;; AUTO-HSCROLL (ledger 212 item 3, new there and unattributed).  Both
;; editors are asked under the same `window-hscroll'; what this records is
;; what each editor's own redisplay LEFT it at.  A divergence here is not a
;; motion defect at all -- it changes the window every other probe is asked
;; about -- so it has to be measured before anything else is believed.
(defun l216-part-b (buffer)
  (l216-two-lines buffer t)
  (l216-say "PARTB auto-hscroll-mode=%S hscroll-margin=%S hscroll-step=%S"
            auto-hscroll-mode hscroll-margin hscroll-step)
  (dolist (hs '(0 5 20 100))
    (dolist (pt '(250 210 400))
      (goto-char pt)
      (set-window-hscroll (selected-window) hs)
      (l216-redisplay)
      (l216-say "PARTB set=%-4s point=%-4s (col %-3s) -> window-hscroll=%s"
                hs pt (- pt 202) (window-hscroll (selected-window)))))
  ;; And with auto-hscroll off: if the divergence survives THAT, it is not the
  ;; auto-hscroll policy but something else moving the hscroll.
  (setq-local auto-hscroll-mode nil)
  (dolist (hs '(0 100))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l216-redisplay)
    (l216-say "PARTB auto-nil set=%-4s point=250 -> window-hscroll=%s"
              hs (window-hscroll (selected-window))))
  (kill-local-variable 'auto-hscroll-mode))

;; ---------------------------------------------------------------- PART C
;; Positions OUTSIDE the window's horizontal span, both directions at once
;; (ledger 212 items 4 and 5).  In GNU these are one mechanism: `pos_visible_p'
;; walks in line-relative coordinates and subtracts the origin once, so a
;; position left of the window gets a negative x and a position past the
;; truncation gets the x the walk STOPPED at.
(defun l216-part-c (buffer)
  (l216-two-lines buffer t)
  (dolist (hs '(0 5 20))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l216-redisplay)
    (set-window-hscroll (selected-window) hs)
    ;; 201+hs is one column LEFT of the window (GNU: x = -1, a frame posn);
    ;; 200+hs is two columns left (GNU: x < -1, nil).
    ;; 202+hs is the first visible column, 280+hs the last drawn one,
    ;; 281+hs the one the right-edge marker overlays, and 282+hs / 300+hs are
    ;; PAST the truncation on the same row.
    (dolist (pos (list (+ 199 hs) (+ 200 hs) (+ 201 hs) (+ 202 hs)
                       (+ 280 hs) (+ 281 hs) (+ 282 hs) (+ 300 hs) 401))
      (l216-say "PARTC hscroll=%-4s pos=%-5s pap=%-20s pvis=%s"
                hs pos (l216-pap pos) (l216-pvis pos)))))

;; ---------------------------------------------------------------- PART D
;; The retained snapshot and a CLIPPED remainder (ledger 211 item 1) and the
;; snapshot's goal column at the window edge (ledger 211 item 2).  One line of
;; `x' with NO trailing newline, `truncate-lines' t, asked from `point-max'.
(defun l216-part-d (buffer)
  (dolist (len '(40 79 80 81 120 161))
    (dolist (trunc '(t nil))
      (switch-to-buffer buffer)
      (with-current-buffer buffer
        (erase-buffer)
        (insert (make-string len ?x))
        (setq-local truncate-lines trunc)
        (setq-local word-wrap nil)
        (set-buffer-modified-p nil))
      (set-window-hscroll (selected-window) 0)
      (goto-char (point-max))
      (l216-redisplay)
      (set-window-hscroll (selected-window) 0)
      (goto-char (point-max))
      (let ((vm0 (progn (vertical-motion 0) (point))))
        (goto-char (point-max))
        (let ((vm-1 (format "%S" (list (vertical-motion -1) (point)))))
          (goto-char (point-max))
          (let ((eovl (progn (end-of-visual-line) (point)))
                (csl (count-screen-lines (point-min) (point-max))))
            (l216-say "PARTD len=%-4s trunc=%-4s vm0=%-5s vm-1=%-9s eovl=%-5s csl=%s"
                      len trunc vm0 vm-1 eovl csl)))))))

;; ---------------------------------------------------------------- PART E
;; The COORDINATE query under hscroll -- the third consumer of the same
;; origin (dispnew.c:6305).  Included so that a change to the goal walk can be
;; shown NOT to move it.
(defun l216-xy (win x y)
  (let ((p (condition-case err (posn-at-x-y x y win)
             (error (format "ERR:%S" (car err))))))
    (cond ((stringp p) p)
          ((null p) "nil")
          (t (format "%S" (list (posn-point p) (posn-actual-col-row p)
                                (posn-area p)))))))

(defun l216-part-e (buffer)
  (l216-two-lines buffer t)
  (dolist (hs '(0 5 20 100))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l216-redisplay)
    (set-window-hscroll (selected-window) hs)
    (dolist (x '(0 1 40 78 79))
      (l216-say "PARTE hscroll=%-4s r1.x%-3s xy=%s" hs x
                (l216-xy (selected-window) x 1)))))

(let ((buffer (generate-new-buffer " *l216-hscroll*")))
  (delete-other-windows)
  (l216-part-a buffer)
  (l216-part-b buffer)
  (l216-part-c buffer)
  (l216-part-d buffer)
  (l216-part-e buffer)
  (delete-other-windows))

(with-temp-file l216-out
  (insert (mapconcat #'identity (nreverse l216-lines) "\n") "\n"))
(kill-emacs 0)

;;; l216-hscroll-origin-probe.el ends here
