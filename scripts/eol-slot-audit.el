;;; eol-slot-audit.el --- the end-of-line insertion slot, both editors -*- lexical-binding: t -*-

;; Ledger 204.  Ledger 201 s6 residual 1 handed over ONE number and ONE minimal
;; reproduction: a two-line buffer "abcdef\nghijkl\n" in an 80-column terminal,
;; where GNU answers `posn-at-point' at the newline ending line 1 and this port
;; answers nil, and where GNU's row maps several trailing x positions to the
;; same buffer position:
;;
;;   GNU  pos=7 char=10  posn=(7 (6 . 0))    row 0 by x: 1,2,3,4,5,6,7,7,7,7
;;   NEO  pos=7 char=10  posn=nil            row 0 by x: 1,2,3,4,5,6,6,6,6,6
;;
;; This sweep reproduces that and WIDENS it, because "the slot after the final
;; glyph" is a different question on a line that ends in a wrap, on a line with
;; no terminator at all, on an empty line, and on a line whose final glyph is
;; more than one column wide.
;;
;; PROTOCOL, AND WHY IT CAN SEE THE DEFECT.  Ledger 195's rule: run the
;; sensitivity check against the PROTOCOL.  The defect being measured lives in
;; the ROW, so it survives a redisplay -- ledger 201 measured 18 warm nils after
;; its fix, and named all 18 as this one residual.  Both protocols are therefore
;; run and compared like against like (port-cold vs GNU-cold, port-warm vs
;; GNU-warm), never mixed:
;;
;;   L204_REDISPLAY=0  COLD  -- no redisplay.  In this port the posn family now
;;                             recomputes through the synchronous single-window
;;                             layout seam ledger 201 wired up, so the SAME row
;;                             producer answers; the defect must appear here too.
;;   L204_REDISPLAY=1  WARM  -- (redisplay t) before every probe: the answer is
;;                             served from the retained snapshot.  This is where
;;                             ledger 201's residual 18 were counted.
;;
;; QUESTIONS PER CASE.  The `xmap' question is the load-bearing one: it is the
;; "row 0 by x" figure quoted above, and it asks the row directly what buffer
;; position each screen column belongs to.  A row that has no slot for its own
;; line terminator answers its LAST GLYPH's position for every trailing column;
;; a row that has one answers the terminator's position.
;;
;;   posn      (posn-at-point) at the end-of-line position
;;   pvw       (pos-visible-in-window-p P nil t) -- the call GNU BUILDS posn out of
;;   xmap      (posn-point (posn-at-x-y X Y w)) for X across the row
;;   wend      (window-end nil t)  -- ledger 201 named this as the blast radius
;;   vmot      (vertical-motion 1) from the line start, and point after it
;;   eolcol    (current-column) at the end-of-line position
;;
;;   emacs -nw -Q -l scripts/eol-slot-audit.el     (through scripts/motion-parity-pty.py)
;;
;; Environment: L204_OUT (default ./tmp/l204/audit-out.txt), L204_REDISPLAY.

(defvar l204-out (or (getenv "L204_OUT") "./tmp/l204/audit-out.txt"))
(defvar l204-redisplays (string-to-number (or (getenv "L204_REDISPLAY") "0")))
(defun l204-redisplay ()
  (dotimes (_ l204-redisplays) (redisplay t)))

;; Each case: NAME TEXT SETUP-FN PROBES.
;; PROBES is a list of (LABEL . POSITION-FN); POSITION-FN runs in the buffer and
;; returns the buffer position to probe.  Using a function rather than a literal
;; keeps the case table honest about WHICH position is "the end of the line":
;; on a line with no terminator it is point-max, not a newline.
(defvar l204-cases
  (list
   ;; Ledger 201's own minimal reproduction, byte for byte.
   (list "two-line" "abcdef\nghijkl\n" nil
         '(("eol1"  . (lambda () 7))     ; the \n ending line 1
           ("eol2"  . (lambda () 14))    ; the \n ending line 2
           ("bol1"  . (lambda () 1))
           ("pmax"  . (lambda () (point-max)))))
   ;; Widening 1: a last line with NO trailing newline.  There is no terminator
   ;; character at all here, so the "slot" is point-max itself.
   (list "no-trailing-nl" "abcdef\nghijkl" nil
         '(("eol1"     . (lambda () 7))
           ("lastchar" . (lambda () 13))          ; the final `l'
           ("eol2"     . (lambda () (point-max))) ; the end of line 2: no newline
           ("pmax"     . (lambda () (point-max)))))
   ;; Widening 2: an EMPTY line -- a row with no glyphs of its own at all.
   (list "empty-line" "abc\n\ndef\n" nil
         '(("eol1"    . (lambda () 4))
           ("emptyln" . (lambda () 5))   ; the \n that is the whole of line 2
           ("eol3"    . (lambda () 9))))
   ;; Widening 3: the final glyph is DOUBLE WIDTH.  If the slot is derived from
   ;; "one column past the last glyph" rather than from the glyph's own advance,
   ;; a wide final character moves the answer by one column.
   (list "wide-eol" "ab界\ncd界\n" nil
         '(("eol1"  . (lambda () 4))     ; a b 界 \n  -> \n at 4
           ("eol2"  . (lambda () 8))))
   ;; Widening 4: the final glyph is a TAB, whose advance is context dependent.
   (list "tab-eol" "ab\tcd\t\nefg\n" nil
         '(("eol1"  . (lambda () 7))     ; a b TAB c d TAB \n -> \n at 7
           ("eol2"  . (lambda () 11))))
   ;; Widening 5: a line that WRAPS.  The continuation row's end is a wrap, not
   ;; a terminator; only the LAST row of the line owns the newline.
   (list "wrapped" (concat (make-string 100 ?y) "\nzz\n")
         (lambda () (setq-local truncate-lines nil))
         '(("eol1"  . (lambda () 101))   ; the \n after 100 y's, on row 1
           ("mid"   . (lambda () 80))    ; last column of row 0
           ("eol2"  . (lambda () 104))))
   ;; Widening 6: the same line TRUNCATED.  The terminator is off screen.
   (list "truncated" (concat (make-string 100 ?y) "\nzz\n")
         (lambda () (setq-local truncate-lines t))
         '(("eol1"  . (lambda () 101))
           ("eol2"  . (lambda () 104))))))

(defun l204-probe (fn)
  (condition-case err (format "%S" (funcall fn))
    (error (format "ERR:%S" (car err)))))

(defun l204-xmap (win pos)
  "Buffer position under each screen column of the row POS sits on.
This is ledger 201's \"row 0 by x\" figure.  Returns a list as long as the
window body width."
  (let* ((p (save-excursion (goto-char pos) (posn-at-point)))
         (row (and p (cdr (posn-col-row p)))))
    (if (null row)
        'no-row
      (let ((acc '()))
        (dotimes (x (window-body-width win))
          (push (let ((q (posn-at-x-y x row win)))
                  (and q (posn-point q)))
                acc))
        (nreverse acc)))))

(defun l204-xmap-by-row (win row)
  "Buffer position under each screen column of screen ROW.
Unlike `l204-xmap' this does not need `posn-at-point' to answer first, so it
still reports when the divergent call returns nil."
  (let ((acc '()))
    (dotimes (x (window-body-width win))
      (push (let ((q (posn-at-x-y x row win)))
              (and q (posn-point q)))
            acc))
    (nreverse acc)))

(defun l204-run ()
  (let ((lines '())
        (buffer (generate-new-buffer " *l204-probe*")))
    (delete-other-windows)
    (dolist (case l204-cases)
      (let* ((name (nth 0 case))
             (text (nth 1 case))
             (setup (nth 2 case))
             (probes (nth 3 case))
             (win nil))
        (delete-other-windows)
        (switch-to-buffer buffer)
        (setq win (selected-window))
        (with-current-buffer buffer
          (kill-all-local-variables)
          (erase-buffer)
          (insert text)
          (set-buffer-modified-p nil)
          (when setup (funcall setup)))
        (goto-char (point-min))
        (l204-redisplay)
        (push (format "CASE %s width=%s height=%s pmax=%s tl=%s"
                      name (window-body-width win) (window-body-height win)
                      (with-current-buffer buffer (point-max))
                      (buffer-local-value 'truncate-lines buffer))
              lines)
        ;; Row-indexed x maps: independent of whether posn-at-point answers.
        (dotimes (row (min 4 (window-body-height win)))
          (push (format "%s|row%d|xmap|%s" name row
                        (l204-probe (lambda () (l204-xmap-by-row win row))))
                lines))
        (dolist (probe probes)
          (let* ((label (car probe))
                 (posfn (cdr probe))
                 (pos (with-current-buffer buffer (funcall posfn))))
            (with-current-buffer buffer
              (goto-char (min pos (point-max)))
              (l204-redisplay)
              (push (format "%s|%s|char|%s" name label
                            (l204-probe (lambda () (char-after))))
                    lines)
              (push (format "%s|%s|posn|%s" name label
                            (l204-probe
                             (lambda () (let ((p (posn-at-point)))
                                          (and p (list (posn-point p)
                                                       (posn-x-y p)
                                                       (posn-col-row p)))))))
                    lines)
              (push (format "%s|%s|posn-actual|%s" name label
                            (l204-probe
                             (lambda () (let ((p (posn-at-point)))
                                          (and p (posn-actual-col-row p))))))
                    lines)
              (push (format "%s|%s|pvw|%s" name label
                            (l204-probe (lambda () (pos-visible-in-window-p (point) nil t))))
                    lines)
              (push (format "%s|%s|eolcol|%s" name label
                            (l204-probe (lambda () (current-column))))
                    lines)
              (push (format "%s|%s|xmap|%s" name label
                            (l204-probe (lambda () (l204-xmap win pos))))
                    lines))))
        ;; The neighbours ledger 201 named as the blast radius of any fix, so a
        ;; fix that moves them is caught in the same protocol that measures it.
        (with-current-buffer buffer
          (goto-char (point-min))
          (l204-redisplay)
          (push (format "%s|-|wend|%s" name
                        (l204-probe (lambda () (window-end nil t))))
                lines)
          (push (format "%s|-|vmot|%s" name
                        (l204-probe
                         (lambda ()
                           (save-excursion
                             (goto-char (point-min))
                             (let ((n (vertical-motion 1))) (list n (point)))))))
                lines)
          (push (format "%s|-|vmot2|%s" name
                        (l204-probe
                         (lambda ()
                           (save-excursion
                             (goto-char (point-min))
                             (let ((n (vertical-motion 2))) (list n (point)))))))
                lines)
          ;; GOAL-COLUMN motion, which is the SAME question the xmap asks, put
          ;; to the motion engine instead of to the posn family:
          ;; `vertical-motion' with a (COLS . LINES) argument runs GNU's
          ;; move_it_in_display_line (&it, ZV, first_x + to_x, MOVE_TO_X)
          ;; (src/indent.c:2540), whose second exit is the display line's own
          ;; end.  A goal column past everything the row draws must therefore
          ;; land on the row's terminator.  This port reaches it through
          ;; `row_goal_stops' (crates/neovm-core/src/emacs_core/editing/indent/mod.rs:731), which
          ;; reads exactly the row field a terminator slot would move.
          (dolist (goal '(0 3 6 8 20 79))
            (push (format "%s|goal%d|vmgoal|%s" name goal
                          (l204-probe
                           (lambda ()
                             (save-excursion
                               (goto-char (point-min))
                               (let ((n (vertical-motion (cons goal 0))))
                                 (list n (point)))))))
                  lines)
            (push (format "%s|goal%d|vmgoal1|%s" name goal
                          (l204-probe
                           (lambda ()
                             (save-excursion
                               (goto-char (point-min))
                               (let ((n (vertical-motion (cons goal 1))))
                                 (list n (point)))))))
                  lines)))
        (delete-other-windows)))
    (make-directory (file-name-directory (expand-file-name l204-out)) t)
    (with-temp-file l204-out
      (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))))

(l204-run)
(kill-emacs)

;;; eol-slot-audit.el ends here
