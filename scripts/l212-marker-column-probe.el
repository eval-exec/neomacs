;;; l212-marker-column-probe.el --- what position a marker column owns -*- lexical-binding: t -*-

;; Ledger 212.  Three ledger entries have hit ONE question from three sides:
;;
;;   * 204 residual 2 / 205 s8.6 / 209 item 4 -- `wrapped|r0.x79' and
;;     `truncated|r0.x79': the column under the RIGHT-edge continuation or
;;     truncation marker answers 80 in GNU and 79 here.
;;   * 210 s8.2 -- a horizontally scrolled truncating row whose retained
;;     snapshot answers `vertical-motion 0' as line-start + hscroll + 1, the
;;     extra 1 being the LEFT truncation marker.
;;
;; THE GNU MODEL, read before this probe was written (Emacs 31.0.90):
;;
;;   A marker glyph carries NO buffer position.  `insert_left_trunc_glyphs'
;;   sets `CHARPOS (truncate_it.position) = BYTEPOS (...) = -1' and
;;   `truncate_it.object = Qnil' (src/xdisp.c:23858-23860) and then OVERWRITES
;;   the row's leading glyphs; `produce_special_glyphs', which makes the
;;   right-edge truncation and the continuation glyph, does
;;   `temp_it.object = Qnil' and zeroes `temp_it.current' (src/xdisp.c:32989-32991)
;;   and likewise overwrites the last glyph produced (src/xdisp.c:26611-26632).
;;
;;   And NO position-answering path in GNU reads a glyph's charpos.  They all
;;   re-walk with an iterator: `Fvertical_motion' through `start_display' and
;;   `move_it_by_lines' (src/indent.c:2317, :2466-2472),
;;   `buffer_posn_from_coords' through `move_it_to' and
;;   `move_it_in_display_line' (src/dispnew.c:6273-6281, :6327), the latter
;;   after `to_x += it.first_visible_x' (src/dispnew.c:6300-6302).  So the
;;   position a marker column reports is the position the WALK is at when it
;;   reaches that column -- the marker overlays a column, it never consumes a
;;   position -- and a row's start is `row->start = it->start' recorded BEFORE
;;   the hscroll skip (src/xdisp.c:25857, skip at :25878-25890), which is the
;;   LINE start however far the row is scrolled.
;;
;; Run in BOTH editors, SAME geometry, one protocol per run:
;;
;;   bash scripts/l205-audit-run.sh emacs scripts/l212-marker-column-probe.el \
;;       L212_OUT ./tmp/l212/marker-gnu-cold.txt L212_REDISPLAY 0 80 24
;;   bash scripts/l205-audit-run.sh ./target/release/neomacs scripts/l212-marker-column-probe.el \
;;       L212_OUT ./tmp/l212/marker-neo-cold.txt L212_REDISPLAY 0 80 24
;;   diff ./tmp/l212/marker-gnu-cold.txt ./tmp/l212/marker-neo-cold.txt
;;
;; Environment: L212_OUT, L212_REDISPLAY (0 cold / 1 warm).

(defvar l212-out (or (getenv "L212_OUT") "./tmp/l212/marker-out.txt"))
(defvar l212-redisplays (string-to-number (or (getenv "L212_REDISPLAY") "0")))
(defun l212-redisplay () (dotimes (_ l212-redisplays) (redisplay t)))

(defvar l212-lines nil)
(defun l212-say (fmt &rest args) (push (apply #'format fmt args) l212-lines))

(defun l212-xy (win x y)
  "`posn-at-x-y' reduced to the three cells this entry is about."
  (let ((p (condition-case err (posn-at-x-y x y win)
             (error (format "ERR:%S" (car err))))))
    (cond ((stringp p) p)
          ((null p) "nil")
          (t (format "%S" (list (posn-point p)
                                (posn-actual-col-row p)
                                (posn-area p)))))))

(defun l212-part-a (buffer)
  "The LEFT truncation marker: a horizontally scrolled truncating row.
Line 2 of the buffer starts at 202.  At hscroll H the first character the
row can draw is 202+H, and GNU overwrites its glyph with `$' -- but the
column still answers 202+H, and `vertical-motion 0' still answers 202."
  (switch-to-buffer buffer)
  (with-current-buffer buffer
    (erase-buffer)
    (insert (concat (make-string 200 ?a) "\n" (make-string 200 ?b) "\n"))
    (setq-local truncate-lines t)
    (setq-local word-wrap nil)
    (set-buffer-modified-p nil))
  (l212-say "PARTA body-width=%s body-height=%s" (window-body-width) (window-body-height))
  (dolist (hs '(0 5 20 100))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l212-redisplay)
    (set-window-hscroll (selected-window) hs)
    (goto-char 250)
    (let ((vm0 (progn (vertical-motion 0) (point))))
      (goto-char 250)
      (let ((vm-1 (format "%S" (list (vertical-motion -1) (point)))))
        (l212-say "PARTA hscroll=%-4s vm0=%-5s vm-1=%s" hs vm0 vm-1))))
  ;; Row 1 of the window is the second line of the buffer.
  (dolist (hs '(0 5 20 100))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l212-redisplay)
    ;; What the editor's own redisplay left the hscroll at.  `auto-hscroll-mode'
    ;; may move it to keep point visible; recording it here is what separates
    ;; "the marker ate a position" from "the retained snapshot describes a
    ;; different hscroll than the one the query is asked under".
    (let ((seen (window-hscroll (selected-window))))
      (set-window-hscroll (selected-window) hs)
      (l212-say "PARTA hscroll=%-4s after-redisplay=%s" hs seen))
    (dolist (x '(0 1 2 3 40 79))
      (l212-say "PARTA hscroll=%-4s r1.x%-3s xy=%s" hs x
                (l212-xy (selected-window) x 1)))))

(defun l212-part-b (buffer kind)
  "The RIGHT-edge marker.  KIND is `truncate' or `wrap'.
Row 0 draws buffer positions 1..79 in columns 0..78 and puts its marker in
column 79.  GNU answers 80 there: the position its walk had reached."
  (switch-to-buffer buffer)
  (with-current-buffer buffer
    (erase-buffer)
    (insert (concat (make-string 200 ?y) "\nzz\n"))
    (setq-local truncate-lines (eq kind 'truncate))
    (setq-local word-wrap nil)
    (set-buffer-modified-p nil))
  (goto-char (point-min))
  (set-window-hscroll (selected-window) 0)
  (l212-redisplay)
  (dolist (x '(0 1 77 78 79))
    (l212-say "PARTB %-8s r0.x%-3s xy=%s" kind x (l212-xy (selected-window) x 0)))
  (dolist (x '(0 1 79))
    (l212-say "PARTB %-8s r1.x%-3s xy=%s" kind x (l212-xy (selected-window) x 1))))

(defun l212-part-c (buffer)
  "Both markers on ONE row: a truncating row that is hscrolled AND overflows."
  (switch-to-buffer buffer)
  (with-current-buffer buffer
    (erase-buffer)
    (insert (concat (make-string 200 ?a) "\n" (make-string 200 ?b) "\n"))
    (setq-local truncate-lines t)
    (setq-local word-wrap nil)
    (set-buffer-modified-p nil))
  (dolist (hs '(5 20))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l212-redisplay)
    (set-window-hscroll (selected-window) hs)
    (goto-char 250)
    (l212-say "PARTC hscroll=%-4s eovl=%s"
              hs (progn (end-of-visual-line) (point)))))

(defun l212-pap (pos)
  "`posn-at-point' reduced to the cells this entry is about.
The eighth cell -- the object (WIDTH . HEIGHT) -- is deliberately NOT recorded:
GNU answers `(1 . 0)' there and this port `(1 . 1)' on every probe alike, which
is ledger 205 residual 1 (GNU's stale `it.pixel_width'), declined by 205 and 209
and out of scope here.  Recording it would make every line of this file diverge
for a reason that has nothing to do with markers."
  (let ((p (condition-case err (posn-at-point pos)
             (error (format "ERR:%S" (car err))))))
    (cond ((stringp p) p)
          ((null p) "nil")
          (t (format "%S" (list (posn-point p) (posn-actual-col-row p)))))))

(defun l212-part-d (buffer)
  "The OTHER direction: `posn-at-point' for a position a marker overlays.
GNU answers this from `pos_visible_in_window_p', which runs an iterator
(src/xdisp.c:1690-1900) and never consults a matrix glyph, so a position
hidden under a marker still reports the column the walk puts it at.  Recording
it says whether a row-based port may make its marker column findable BY
POSITION as well as by coordinate."
  (switch-to-buffer buffer)
  (with-current-buffer buffer
    (erase-buffer)
    (insert (concat (make-string 200 ?a) "\n" (make-string 200 ?b) "\n"))
    (setq-local truncate-lines t)
    (setq-local word-wrap nil)
    (set-buffer-modified-p nil))
  (dolist (hs '(0 5 20))
    (goto-char 250)
    (set-window-hscroll (selected-window) hs)
    (l212-redisplay)
    (set-window-hscroll (selected-window) hs)
    ;; 202+hs is the character the LEFT marker overlays; 281+hs is the one the
    ;; RIGHT-edge marker overlays; the neighbours either side are controls, and
    ;; 282+hs is PAST the truncation (a different question: what a truncated
    ;; row does with the rest of its line).
    (dolist (pos (list (+ 201 hs) (+ 202 hs) (+ 203 hs)
                       (+ 280 hs) (+ 281 hs) (+ 282 hs)))
      (l212-say "PARTD trunc  hscroll=%-4s pos=%-5s pap=%-18s pvis=%s" hs pos
                (l212-pap pos)
                (format "%S" (condition-case err
                                 (pos-visible-in-window-p pos nil t)
                               (error (format "ERR:%S" (car err))))))))
  ;; The CONTINUATION case: position 80 is drawn at row 1 column 0 AND stands
  ;; under the `\\' in row 0 column 79.  Which one `posn-at-point' answers is
  ;; what says whether a marker slot may win a POSITION lookup.
  (with-current-buffer buffer (setq-local truncate-lines nil))
  (goto-char (point-min))
  (set-window-hscroll (selected-window) 0)
  (l212-redisplay)
  (dolist (pos '(78 79 80 81))
    (l212-say "PARTD wrap   pos=%-5s pap=%-18s pvis=%s" pos (l212-pap pos)
              (format "%S" (condition-case err
                               (pos-visible-in-window-p pos nil t)
                             (error (format "ERR:%S" (car err))))))))

(let ((buffer (generate-new-buffer " *l212-marker*")))
  (delete-other-windows)
  (l212-part-a buffer)
  (l212-part-b buffer 'truncate)
  (l212-part-b buffer 'wrap)
  (l212-part-c buffer)
  (l212-part-d buffer)
  (delete-other-windows))
(with-temp-file l212-out
  (insert (mapconcat #'identity (nreverse l212-lines) "\n") "\n"))
(kill-emacs)

;;; l212-marker-column-probe.el ends here
