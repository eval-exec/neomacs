;;; l212-wrap-goal-probe.el --- does the goal-column walk stop ON a marker column? -*- lexical-binding: t -*-

;; Ledger 212 section 5, committed so the open question in section 6 item 1
;; starts from a measurement rather than from my sentence.
;;
;;   bash scripts/l205-audit-run.sh emacs scripts/l212-wrap-goal-probe.el \
;;       L212_OUT ./tmp/l212/wrap-goal-gnu.txt L212_REDISPLAY 1 80 24
;;
;; GNU Emacs 31.0.90, 80x24 pty, measured 2026-08-28.  Changing NOTHING but
;; `word-wrap', on the same unbroken 300-character line:
;;
;;   word-wrap nil   eovl 80   (vertical-motion (cons 80 0)) -> (0 80)   x79 posn 80
;;   word-wrap t     eovl 79   (vertical-motion (cons 80 0)) -> (0 79)   x79 posn 80
;;
;; The COORDINATE answer is the same in both and the GOAL answer is not, which
;; is why ledger 212 admits marker slots to `point_at_coords' and keeps them out
;; of `row_goal_stops'.
;;
;; THE CONTRADICTION THE NEXT AGENT SHOULD START FROM.  GNU's WORD_WRAP exit
;; from `move_it_in_display_line_to' is guarded by
;; `it->line_wrap != WORD_WRAP || wrap_it.sp < 0' (src/xdisp.c:10385, :10398,
;; and the restore at :10816-10837).  A 300-character line with no space in it
;; has NO wrap opportunity, so `wrap_it.sp < 0' should send both configurations
;; down the same branch -- and the two answers above say they do not.  Whatever
;; the mechanism is, it is not the one that reading predicts.
(defvar out (or (getenv "L212_OUT") "./tmp/l212/wrap-goal.txt"))
(defvar lines nil)
(defun say (fmt &rest args) (push (apply #'format fmt args) lines))
(defun xy (win x y)
  (let ((p (posn-at-x-y x y win)))
    (if p (format "%S" (list (posn-point p) (posn-actual-col-row p))) "nil")))
(let ((buffer (generate-new-buffer " *l212-wrap-goal*")))
  (delete-other-windows)
  (switch-to-buffer buffer)
  (dolist (case '(("char-wrap"  nil nil)     ; long unbroken run, word-wrap nil
                  ("word-wrap"  t   nil)
                  ("visual-line" t  t)))
    (let ((name (nth 0 case)) (ww (nth 1 case)) (vlm (nth 2 case)))
      (dolist (text (list (make-string 300 ?x)
                          (mapconcat #'identity (make-list 40 "wordy") " ")))
        (with-current-buffer buffer
          (erase-buffer) (insert text) (insert "\n")
          (setq-local truncate-lines nil)
          (setq-local word-wrap ww)
          (when vlm (visual-line-mode 1))
          (set-buffer-modified-p nil))
        (goto-char (point-min))
        (redisplay t)
        (let* ((w (window-width))
               (eovl (progn (goto-char (point-min)) (end-of-visual-line) (point)))
               (vmc (progn (goto-char (point-min))
                           (list (vertical-motion (cons w 0)) (point))))
               (vm1 (progn (goto-char (point-min)) (vertical-motion 1) (point))))
          (say "%-11s len=%-4s w=%-3s eovl=%-5s vmc-w=%-10s vm1=%-5s x%s=%-14s x%s=%s"
               name (length text) w eovl (format "%S" vmc) vm1
               (- w 2) (xy (selected-window) (- w 2) 0)
               (- w 1) (xy (selected-window) (- w 1) 0)))
        (when vlm (with-current-buffer buffer (visual-line-mode -1))))))
  (delete-other-windows))
(with-temp-file out (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))
(kill-emacs)
