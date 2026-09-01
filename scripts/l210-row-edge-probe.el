;;; l210-row-edge-probe.el --- where a row ends at the window's right edge -*- lexical-binding: t -*-

;; Ledger 210.  The reproduction for the three defects entry 210 found and did
;; NOT fix, committed so the next agent starts from RED rather than rebuilding
;; it.  Run it in BOTH editors through the pty driver, at the SAME geometry:
;;
;;   bash scripts/l205-audit-run.sh emacs scripts/l210-row-edge-probe.el \
;;       L210_OUT ./tmp/l210/edge-gnu.txt L195_REDISPLAY 1 80 24
;;   bash scripts/l205-audit-run.sh ./target/release/neomacs scripts/l210-row-edge-probe.el \
;;       L210_OUT ./tmp/l210/edge-neo.txt L195_REDISPLAY 1 80 24
;;   diff ./tmp/l210/edge-gnu.txt ./tmp/l210/edge-neo.txt
;;
;; It must be run under a pty.  `vertical-motion' is TWO engines that share no
;; code (ledger 195; GNU's own `if (noninteractive)' at src/indent.c:2280), and
;; under --batch both editors already agree here to the character -- the whole
;; point is that this port has ONE row model where GNU has two.
;;
;; GROUND TRUTH, GNU Emacs 31.0.90, `-nw -Q' in an 80x24 pty, measured
;; 2026-08-28.  Every line this port gets wrong is marked.
;;
;; PART 1 -- one line of `x', NO trailing newline, `truncate-lines' t:
;;
;;   len  (vertical-motion (buffer-size))  count-screen-lines  end-of-visual-line
;;    78            (0 79)                        1                   79
;;    79            (0 80)                        1                   80
;;    80            (1 81)   <- port says (0 81)  2  <- port 1         80  <- port 79
;;    81            (1 82)   <- port says (0 82)  1  <- port 0         80  <- port 79
;;   160            (1 161)  <- port says (0 161) 1  <- port 0         80  <- port 79
;;
;;   `count-screen-lines' answering 0 for a buffer with text in it is the
;;   sharpest way to put it.  Both symptoms come from ONE place:
;;   `truncated_logical_line_step' (crates/neovm-core/src/emacs_core/editing/indent/mod.rs:600)
;;   labels the row it leaves `ScreenLineEnd::BufferEnd' -- "the scan ran out of
;;   accessible buffer" -- when the row had ALREADY reached the window's right
;;   edge; both of its call sites (indent.rs:508, :539) are reached only from
;;   there.  `ScreenLineEnd::Edge' is the label that both counts the row and
;;   makes its boundary a goal-column stop (indent.rs:1028).
;;
;;   It is NOT a one-line fix: under GNU's OTHER engine the port is already
;;   right.  `compute_motion's truncating branch skips to the newline and does
;;   NOT increment `vpos' (src/indent.c:1494-1502), where the continuing branch
;;   does (src/indent.c:1523) -- so GNU's own --batch answers are
;;   ((78 0 79 1 1) (79 0 80 1 1) (80 0 81 1 1) (81 0 82 0 1) (160 0 161 0 1)),
;;   which this port reproduces byte for byte.  The row end has to become a
;;   function of `MotionEngine'.
;;
;; PART 2 -- a horizontally scrolled truncating row, COLD and WARM:
;;
;;   `vertical-motion 0' answers the row's START.  GNU answers the LINE start at
;;   every hscroll, because `nlines <= 0' goes through `move_it_by_lines'
;;   (src/indent.c:2466-2472) and hscroll does not split a line into rows.  This
;;   port's SCANNER agrees; its retained redisplay SNAPSHOT does not -- warm it
;;   answers line-start + hscroll + 1, the extra 1 being the left truncation
;;   marker consuming a position it only overlays.
;;
;;     COLD  GNU 202 / (-1 1) at hscroll 0, 5, 20, 100   port identical
;;     WARM  GNU 202 / (-1 1) at hscroll 0, 5, 20, 100
;;           port 202, 208, 223, 202  and  (-1 1), (-1 7), (-1 22), (-1 1)
;;
;;   `end-of-visual-line' is `(vertical-motion (cons (window-width) 0))'
;;   (lisp/simple.el:8546-8558) and GNU adds the hscroll to the goal:
;;   `move_it_in_display_line (&it, ZV, first_x + to_x, MOVE_TO_X)'
;;   (src/indent.c:2531), documented at src/indent.c:2226-2228.  GNU answers
;;   281 / 286 / 301 / 381 in BOTH protocols.  This port answers
;;
;;     COLD  281 / 281 / 281 / 281   -- the scanner does not add the hscroll
;;                                      to the goal at all
;;     WARM  280 / 285 / 300 / 281   -- the snapshot adds it and then stops one
;;                                      column short of the window edge
;;
;;   so the goal column and the row start are two different defects that happen
;;   to meet on the same probe.

(defvar l210-out (or (getenv "L210_OUT") "./tmp/l210/row-edge-out.txt"))
(defvar l210-lines nil)
(defun l210-say (fmt &rest args) (push (apply #'format fmt args) l210-lines))

(defun l210-part1 (buffer)
  "One truncating line that reaches the window's right edge, with no newline."
  (switch-to-buffer buffer)
  (let ((w (window-body-width)))
    (l210-say "PART1 body-width=%s" w)
    (dolist (n (list (- w 2) (- w 1) w (+ w 1) (+ w 80)))
      (with-current-buffer buffer
        (erase-buffer)
        (insert (make-string n ?x))
        (setq-local truncate-lines t)
        (setq-local word-wrap nil)
        (set-buffer-modified-p nil))
      (goto-char (point-min))
      (redisplay t)
      (let* ((vm (progn (goto-char (point-min)) (list (vertical-motion (buffer-size)) (point))))
             (csl (count-screen-lines (point-min) (point-max)))
             (eovl (progn (goto-char (point-min)) (end-of-visual-line) (point))))
        (l210-say "PART1 len=%-4s vm-to-end=%S csl=%s eovl=%s" n vm csl eovl)))))

(defun l210-part2 (buffer)
  "A horizontally scrolled truncating row, asked COLD and then WARM."
  (switch-to-buffer buffer)
  (with-current-buffer buffer
    (erase-buffer)
    (insert (concat (make-string 200 ?a) "\n" (make-string 200 ?b) "\n"))
    (setq-local truncate-lines t)
    (setq-local word-wrap nil)
    (set-buffer-modified-p nil))
  (dolist (warm '(nil t))
    (dolist (hs '(0 5 20 100))
      (goto-char 250)                   ; line 2 starts at 202
      (set-window-hscroll (selected-window) hs)
      (when warm (redisplay t))
      (set-window-hscroll (selected-window) hs)
      (goto-char 250)
      (let ((vm0 (progn (vertical-motion 0) (point))))
        (goto-char 250)
        (let ((vm-1 (list (vertical-motion -1) (point))))
          (goto-char 250)
          (let ((eovl (progn (end-of-visual-line) (point))))
            (l210-say "PART2 %s hscroll=%-4s vm0=%-5s vm-1=%-10s eovl=%s"
                      (if warm "WARM" "COLD") hs vm0 (format "%S" vm-1) eovl)))))))

(let ((buffer (generate-new-buffer " *l210-row-edge*")))
  (delete-other-windows)
  (l210-part1 buffer)
  (l210-part2 buffer)
  (delete-other-windows))
(with-temp-file l210-out
  (insert (mapconcat #'identity (nreverse l210-lines) "\n") "\n"))
(kill-emacs)

;;; l210-row-edge-probe.el ends here
