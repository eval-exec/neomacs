;;; l211-truncated-row-probe.el --- a clipped TRUNCATE row, under both engines -*- lexical-binding: t -*-

;; Ledger 211.  Ledger 210 section 7 reduced the row-edge divergence to one
;; label: `truncated_logical_line_step' calls a row that was CLIPPED at the
;; window's right edge `ScreenLineEnd::BufferEnd' -- "the scan ran out of
;; accessible buffer" -- so the row is not counted and its boundary is not a
;; goal stop.  This probe widens 210's three lines to every motion that reads
;; the label, and it is deliberately runnable under BOTH protocols:
;;
;;   pty  (display iterator)   bash scripts/l205-audit-run.sh EDITOR \
;;          scripts/l211-truncated-row-probe.el L211_OUT OUT L195_REDISPLAY 1 80 24
;;   batch (compute_motion)    EDITOR --batch -Q -l scripts/l211-truncated-row-probe.el
;;
;; because the answer is NOT the same in the two, and that is the whole point:
;; `compute_motion's truncating branch skips to the newline WITHOUT
;; incrementing `vpos' (src/indent.c:1494-1502) where its continuing branch
;; does (src/indent.c:1523), while the display iterator's MOVE_LINE_TRUNCATED
;; arm reseats to the next visible line start and falls through to `++it->vpos'
;; (src/xdisp.c:11118-11143, 11200).  A port with ONE row model cannot answer
;; both.

(defvar l211-out (getenv "L211_OUT"))
;; WARM (the default) asks after a `redisplay', so a retained snapshot may
;; answer; COLD (L211_REDISPLAY=0) forces the scanner.  Ledger 195's harness
;; makes the same distinction and for the same reason: a defect in the scanner
;; is invisible under WARM and a defect in the snapshot is invisible under
;; COLD.
(defvar l211-warm (not (equal (getenv "L211_REDISPLAY") "0")))
(defvar l211-lines nil)
(defun l211-say (fmt &rest args) (push (apply #'format fmt args) l211-lines))

(defun l211-probe (buffer)
  (switch-to-buffer buffer)
  (let ((w (window-body-width)))
    (l211-say "body-width=%s protocol=%s" w (if l211-warm "WARM" "COLD"))
    ;; PART A -- one logical line of `x', NO trailing newline.  The row is
    ;; clipped exactly when the text passes the truncation marker's column.
    (dolist (n (list (- w 2) (- w 1) w (+ w 1) (+ w 80)))
      (with-current-buffer buffer
        (erase-buffer)
        (insert (make-string n ?x))
        (setq-local truncate-lines t)
        (setq-local word-wrap nil)
        (set-buffer-modified-p nil))
      (goto-char (point-min))
      (when l211-warm (redisplay t))
      (let* ((vm-end (progn (goto-char (point-min))
                            (list (vertical-motion (buffer-size)) (point))))
             (vm1 (progn (goto-char (point-min))
                         (list (vertical-motion 1) (point))))
             (csl (count-screen-lines (point-min) (point-max)))
             (eovl (progn (goto-char (point-min)) (end-of-visual-line) (point)))
             (bovl-zv (progn (goto-char (point-max)) (vertical-motion 0) (point)))
             (vm-1-zv (progn (goto-char (point-max))
                             (list (vertical-motion -1) (point)))))
        (l211-say "A len=%-4s vm-end=%S vm1=%S csl=%s eovl=%s bovl@zv=%s vm-1@zv=%S"
                  n vm-end vm1 csl eovl bovl-zv vm-1-zv)))
    ;; PART B -- the same clipped row WITH a following logical line, so the
    ;; clipped row is not also the last one.  A truncated row that ends at a
    ;; newline counts under both engines; this pins that it still does.
    (dolist (n (list (- w 1) w (+ w 80)))
      (with-current-buffer buffer
        (erase-buffer)
        (insert (make-string n ?x) "\n" (make-string n ?y) "\n")
        (setq-local truncate-lines t)
        (setq-local word-wrap nil)
        (set-buffer-modified-p nil))
      (goto-char (point-min))
      (when l211-warm (redisplay t))
      (let* ((vm-end (progn (goto-char (point-min))
                            (list (vertical-motion (buffer-size)) (point))))
             (vm1 (progn (goto-char (point-min))
                         (list (vertical-motion 1) (point))))
             (vm2 (progn (goto-char (point-min))
                         (list (vertical-motion 2) (point))))
             (csl (count-screen-lines (point-min) (point-max)))
             (vm-1-zv (progn (goto-char (point-max))
                             (list (vertical-motion -1) (point)))))
        (l211-say "B len=%-4s vm-end=%S vm1=%S vm2=%S csl=%s vm-1@zv=%S"
                  n vm-end vm1 vm2 csl vm-1-zv)))
    ;; PART C -- the WRAPPING control.  Nothing here may move: the continuing
    ;; branch already increments `vpos' in both engines, and a row filled by
    ;; the buffer's LAST character still counts as none.
    (dolist (n (list (- w 1) w (+ w 1)))
      (with-current-buffer buffer
        (erase-buffer)
        (insert (make-string n ?x))
        (setq-local truncate-lines nil)
        (setq-local word-wrap nil)
        (set-buffer-modified-p nil))
      (goto-char (point-min))
      (when l211-warm (redisplay t))
      (let* ((vm-end (progn (goto-char (point-min))
                            (list (vertical-motion (buffer-size)) (point))))
             (vm1 (progn (goto-char (point-min))
                         (list (vertical-motion 1) (point))))
             (csl (count-screen-lines (point-min) (point-max)))
             (eovl (progn (goto-char (point-min)) (end-of-visual-line) (point))))
        (l211-say "C len=%-4s vm-end=%S vm1=%S csl=%s eovl=%s" n vm-end vm1 csl eovl)))))

(let ((buffer (generate-new-buffer " *l211-truncated-row*")))
  (delete-other-windows)
  (l211-probe buffer)
  (delete-other-windows))
(let ((text (concat (mapconcat #'identity (nreverse l211-lines) "\n") "\n")))
  (if l211-out
      (with-temp-file l211-out (insert text))
    (princ text)))
(when l211-out (kill-emacs))

;;; l211-truncated-row-probe.el ends here
