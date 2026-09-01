;;; below-content-audit.el --- clicks below the last row with content -*- lexical-binding: t -*-

;; Ledger 205.  Ledger 204 s8 residual 1 handed over one sentence and five
;; measured rows:
;;
;;   "Every screen row below the last row with content answers nil where GNU
;;    answers point-max."
;;
;;   two-line|row3        GNU 15 in all 80 columns    NEO nil
;;   no-trailing-nl|row2  GNU 14                      NEO nil
;;   tab-eol|row3         GNU 12                      NEO nil
;;   wide-eol|row3        GNU  9                      NEO nil
;;   truncated|row3       GNU 105                     NEO nil
;;
;; That is every mouse click in the empty area under a short buffer, which in a
;; typical editing session is most of the window.
;;
;; PROTOCOL, AND WHY IT CAN SEE THE DEFECT.  Ledger 195's rule -- run the
;; sensitivity check against the PROTOCOL, not just the tree.  The divergent
;; call here is `posn-at-x-y', which in this port reaches
;; `WindowDisplaySnapshot::point_at_coords' (crates/neovm-core/src/window/mod.rs:2256).
;; That function opens with "find the row whose y band contains Y" and `?'s out
;; to None when none does.  A row below the last line of a short buffer is a row
;; the producer never emitted, so it is absent from `snapshot.rows' whether the
;; snapshot was RETAINED by a redisplay (warm) or RECOMPUTED through ledger
;; 201's synchronous single-window layout seam (cold): the same row producer
;; fills it either way.  The defect must therefore appear under BOTH protocols,
;; and a protocol that only redisplayed more would not hide it.  Both are run
;; and compared like against like -- port-cold vs GNU-cold, port-warm vs
;; GNU-warm -- never mixed:
;;
;;   L205_REDISPLAY=0  COLD  -- no redisplay anywhere.
;;   L205_REDISPLAY=1  WARM  -- (redisplay t) before every probe.
;;
;; QUESTIONS PER PROBE.  `xy' is the divergent call; the rest are the controls
;; that say whether a divergence is this defect or one of its neighbours.
;;
;;   xy       (posn-at-x-y X Y WIN) -> (posn-point, posn-col-row, posn-area)
;;   area     the window part GNU's `window_from_coordinates' assigned, which
;;            is what decides whether the probe is a TEXT-area question at all
;;   wend     (window-end nil t)   -- ledger 201/204's named blast radius
;;   vmot     (vertical-motion 1)  -- the motion engine, same rows
;;   pmax     the buffer's own point-max, so "GNU answers point-max" is
;;            checkable inside a single output file
;;
;; WIDENING.  Ledger 205's brief names five neighbours of "below content", and
;; each is a different reason for a row to be missing:
;;
;;   empty-buffer      no content at all: EVERY body row is below content
;;   no-trailing-nl    the last line has no terminator, so there is no final
;;                     empty display line and point-max is on a drawn row
;;   trailing-nl       there IS a final empty display line, drawn, at point-max
;;   no-mode-line      `mode-line-format' nil: the body reaches the frame's
;;                     last row, so the region below content is one row taller
;;   header-line       a header line shifts the text area down by one row
;;   minibuffer        a one-row window whose buffer is empty
;;   split-narrow      a side window, so x is offset from the frame origin
;;   truncated/wrapped ledger 204's own two, kept so its five rows reproduce
;;
;;   emacs -nw -Q -l scripts/below-content-audit.el  (through scripts/motion-parity-pty.py)
;;
;; Environment: L205_OUT (default ./tmp/l205/audit-out.txt), L205_REDISPLAY.

(defvar l205-out (or (getenv "L205_OUT") "./tmp/l205/audit-out.txt"))
(defvar l205-redisplays (string-to-number (or (getenv "L205_REDISPLAY") "0")))
(defun l205-redisplay ()
  (dotimes (_ l205-redisplays) (redisplay t)))

(defvar l205-xs '(0 1 5 40 79)
  "Screen columns probed on every row.
0 is the left edge, 1 and 5 are inside a short line's drawn glyphs, 40 and 79
are past the end of every line in every case here.  GNU's
`buffer_posn_from_coords' adds \"extra (default width) columns if clicked after
EOL\" (src/dispnew.c:6428-6430), so the COLUMN it reports for these grows with X
while the POSITION does not -- both are recorded.")

(defun l205-probe (fn)
  (condition-case err (format "%S" (funcall fn))
    (error (format "ERR:%S" (car err)))))

(defun l205-xy (win x y)
  "The four answers `posn-at-x-y' gives, or nil if it gives none.

`posn-col-row' is DERIVED from `posn-x-y' by dividing out the frame's character
cell, so it always reports the row that was clicked.  `posn-actual-col-row' is
the raw (COL . ROW) element the C code stored, which is
`buffer_posn_from_coords' handing back `it.hpos' and `it.vpos'
\(src/dispnew.c:6432-6433) -- the row the ITERATOR stopped on, plus the
\"extra (default width) columns if clicked after EOL\" bump at :6428.  Recording
both is what makes \"GNU answers from an iterator that ran out of buffer\"
checkable rather than asserted."
  (let ((p (posn-at-x-y x y win)))
    (and p (list (posn-point p)
                 (posn-col-row p)
                 (posn-actual-col-row p)
                 (posn-area p)))))

;; Each case: NAME TEXT SETUP-FN.  SETUP-FN runs in the probe buffer.
(defvar l205-cases
  (list
   ;; Ledger 204's own five rows, byte for byte, so its residual reproduces.
   (list "two-line" "abcdef\nghijkl\n" nil)
   (list "no-trailing-nl" "abcdef\nghijkl" nil)
   (list "tab-eol" "ab\tcd\t\nefg\n" nil)
   (list "wide-eol" "ab界\ncd界\n" nil)
   (list "truncated" (concat (make-string 100 ?y) "\nzz\n")
         (lambda () (setq-local truncate-lines t)))
   (list "wrapped" (concat (make-string 100 ?y) "\nzz\n")
         (lambda () (setq-local truncate-lines nil)))
   ;; Widening 1: a completely empty buffer.  Every body row but the first is
   ;; below content, and the first row draws no glyph either.
   (list "empty-buffer" "" nil)
   ;; Widening 2: one line, no terminator.  point-max sits on a DRAWN row.
   (list "one-line-no-nl" "abc" nil)
   ;; Widening 3: one line WITH a terminator, so there is a final empty
   ;; display line that IS drawn, and point-max is on it.
   (list "one-line-nl" "abc\n" nil)
   ;; Widening 4: no mode line.  The body is one row taller and its last row
   ;; is the frame's last row, so "below content" reaches the screen bottom.
   (list "no-mode-line" "abc\n" (lambda () (setq-local mode-line-format nil)))
   ;; Widening 5: a header line shifts the text area down.
   (list "header-line" "abc\n"
         (lambda () (setq-local header-line-format "HEADER")))
   ;; Widening 6: both at once.
   (list "header-no-mode" "abc\n"
         (lambda ()
           (setq-local header-line-format "HEADER")
           (setq-local mode-line-format nil)))
   ;; Widening 7: NARROWED.  GNU's walk stops at ZV, which is the accessible
   ;; end and not point-max, so this tells "the end of the buffer" apart from
   ;; "the end of what is accessible" -- and `point-max' in a narrowed buffer
   ;; IS the accessible end, so the CASE line's pmax is the value to expect.
   (list "narrowed" "abc\ndef\nghi\njkl\n"
         (lambda () (narrow-to-region 1 9)))))

(defun l205-sweep (label win)
  "Probe every body row of WIN at every column in `l205-xs'.
Returns a list of output lines."
  (let ((lines '())
        (h (window-body-height win))
        (w (window-body-width win)))
    (push (format "%s|-|geom|%S" label
                  (list w h
                        (window-body-height win t)
                        (window-mode-line-height win)
                        (window-header-line-height win)))
          lines)
    (dotimes (row h)
      (dolist (x l205-xs)
        (when (< x w)
          (push (format "%s|r%d.x%d|xy|%s" label row x
                        (l205-probe (lambda () (l205-xy win x row))))
                lines))))
    ;; One row PAST the body: GNU's `window_from_coordinates' calls this the
    ;; mode line (or, with no mode line, the next window / nothing).  Kept as a
    ;; control: a fix for "below content" must not swallow this row.
    (dolist (x '(0 5))
      (push (format "%s|past.x%d|xy|%s" label x
                    (l205-probe (lambda () (l205-xy win x h))))
            lines))
    (nreverse lines)))

(defun l205-run ()
  (let ((lines '())
        (buffer (generate-new-buffer " *l205-probe*")))
    (dolist (case l205-cases)
      (let* ((name (nth 0 case))
             (text (nth 1 case))
             (setup (nth 2 case))
             (win nil))
        (delete-other-windows)
        (switch-to-buffer buffer)
        (setq win (selected-window))
        (with-current-buffer buffer
          (kill-all-local-variables)
          (setq header-line-format nil)
          (erase-buffer)
          (insert text)
          (set-buffer-modified-p nil)
          (when setup (funcall setup)))
        (goto-char (point-min))
        (l205-redisplay)
        (push (format "CASE %s pmax=%s lines=%s tl=%s" name
                      (with-current-buffer buffer (point-max))
                      (with-current-buffer buffer
                        (count-lines (point-min) (point-max)))
                      (buffer-local-value 'truncate-lines buffer))
              lines)
        (setq lines (append (nreverse (l205-sweep name win)) lines))
        ;; The neighbours ledgers 201 and 204 named as the blast radius.
        (with-current-buffer buffer
          (goto-char (point-min))
          (l205-redisplay)
          (push (format "%s|-|wend|%s" name
                        (l205-probe (lambda () (window-end nil t))))
                lines)
          (push (format "%s|-|vmot|%s" name
                        (l205-probe
                         (lambda ()
                           (save-excursion
                             (goto-char (point-min))
                             (let ((n (vertical-motion 1))) (list n (point)))))))
                lines)
          (push (format "%s|-|pmax-posn|%s" name
                        (l205-probe
                         (lambda ()
                           (save-excursion
                             (goto-char (point-max))
                             (let ((p (posn-at-point)))
                               (and p (list (posn-point p) (posn-col-row p))))))))
                lines))
        (delete-other-windows)))
    ;; Widening 7: a side window, so the text area does not start at frame
    ;; column 0.  The port maps x through `text_area_left_offset'; a below-
    ;; content answer must use the same mapping as an on-content one.
    (progn
      (delete-other-windows)
      (switch-to-buffer buffer)
      (with-current-buffer buffer
        (kill-all-local-variables)
        (setq header-line-format nil)
        (erase-buffer)
        (insert "abc\ndef\n")
        (set-buffer-modified-p nil))
      (let ((side (split-window-right -24)))
        (with-selected-window side
          (switch-to-buffer buffer)
          (goto-char (point-min)))
        (l205-redisplay)
        (push (format "CASE split-narrow pmax=%s lines=%s tl=%s"
                      (with-current-buffer buffer (point-max))
                      (with-current-buffer buffer
                        (count-lines (point-min) (point-max)))
                      (buffer-local-value 'truncate-lines buffer))
              lines)
        (setq lines (append (nreverse (l205-sweep "split-narrow" side)) lines)))
      (delete-other-windows))
    ;; Widening 9: a SCROLLED window.  `window-start' is not point-min, so an
    ;; answer of "the end of the buffer" cannot have been reached by counting
    ;; from the beginning.  The buffer is longer than the window and then
    ;; scrolled so that its end is on screen with rows to spare.
    (progn
      (delete-other-windows)
      (switch-to-buffer buffer)
      (with-current-buffer buffer
        (kill-all-local-variables)
        (setq header-line-format nil)
        (erase-buffer)
        (dotimes (i 40) (insert (format "line %d\n" i)))
        (set-buffer-modified-p nil)
        (goto-char (point-max)))
      (let ((win (selected-window)))
        (set-window-start win (save-excursion
                                (goto-char (point-max))
                                (forward-line -3)
                                (point)))
        (l205-redisplay)
        (push (format "CASE scrolled pmax=%s lines=%s tl=%s wstart=%s"
                      (with-current-buffer buffer (point-max))
                      (with-current-buffer buffer
                        (count-lines (point-min) (point-max)))
                      (buffer-local-value 'truncate-lines buffer)
                      (window-start win))
              lines)
        (setq lines (append (nreverse (l205-sweep "scrolled" win)) lines)))
      (delete-other-windows))
    ;; Widening 8: the minibuffer window.  One row, an empty buffer, and no
    ;; mode line of its own.
    (progn
      (delete-other-windows)
      (let ((mb (minibuffer-window)))
        (l205-redisplay)
        (push (format "CASE minibuffer pmax=%s lines=%s tl=%s"
                      (with-current-buffer (window-buffer mb) (point-max))
                      (with-current-buffer (window-buffer mb)
                        (count-lines (point-min) (point-max)))
                      (buffer-local-value 'truncate-lines (window-buffer mb)))
              lines)
        (setq lines (append (nreverse (l205-sweep "minibuffer" mb)) lines))))
    (make-directory (file-name-directory (expand-file-name l205-out)) t)
    (with-temp-file l205-out
      (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))))

(l205-run)
(kill-emacs)

;;; below-content-audit.el ends here
