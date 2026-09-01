;;; l215-minibuffer-source-probe.el --- what a mini-window answers about ITS buffer -*- lexical-binding: t -*-

;; Ledger 215, root-causing ledger 205 residual 5 / ledger 209 item 5 / ledger
;; 212 item 8.  An inactive mini-window is laid out from an ECHO AREA buffer
;; while `window-buffer' still answers " *Minibuf-0*" -- GNU does the same, with
;; `with_echo_area_buffer' installing the echo buffer in the window for the
;; duration of display (src/xdisp.c:12961) and restoring it on unwind
;; (src/xdisp.c:13038).  What GNU does NOT do is answer a buffer position out of
;; the rows that walk produced: `buffer_posn_from_coords' opens with
;; `Fset_buffer (w->contents)' (src/dispnew.c:6276) and re-walks the WINDOW's own
;; buffer, and `Fvertical_motion' walks the current buffer with no matrix at all.
;;
;; This probe asks every question in that class at once, so the CLASS is
;; measured rather than the one `posn-at-x-y' column ledger 205 published.

(defvar l215-out (or (getenv "L215_OUT") "./tmp/l215/minibuffer-source-probe.txt"))

(defun l215-safe (thunk)
  (condition-case err (format "%S" (funcall thunk))
    (error (format "ERR:%S" (car err)))))

(defun l215-run ()
  (redisplay t)
  (let* ((mb (minibuffer-window))
         (buf (window-buffer mb))
         (lines '()))
    (push (format "window-buffer=%S pmax=%S"
                  (buffer-name buf)
                  (with-current-buffer buf (point-max)))
          lines)
    (push (format "current-message=%S" (current-message)) lines)
    (dolist (name '(" *Echo Area 0*" " *Echo Area 1*"))
      (push (format "echo %s exists=%S pmax=%S text=%S" name
                    (and (get-buffer name) t)
                    (and (get-buffer name)
                         (with-current-buffer name (point-max)))
                    (and (get-buffer name)
                         (with-current-buffer name
                           (buffer-substring-no-properties
                            (point-min) (min (point-max) 24)))))
            lines))
    ;; 1. posn-at-x-y on the mini-window: ledger 205's six probes.
    (dolist (x '(0 1 5 40 64 65 79))
      (push (format "posn-at-x-y x=%d %s" x
                    (l215-safe
                     (lambda ()
                       (let ((p (posn-at-x-y x 0 mb)))
                         (and p (list (posn-point p) (posn-col-row p)
                                      (posn-actual-col-row p) (posn-area p)))))))
            lines))
    ;; 2. posn-at-point inside the mini-window.
    (push (format "posn-at-point %s"
                  (l215-safe
                   (lambda ()
                     (with-selected-window mb
                       (let ((p (posn-at-point (point-min) mb)))
                         (and p (list (posn-point p) (posn-col-row p)
                                      (posn-actual-col-row p))))))))
          lines)
    ;; 3. window-end / pos-visible-in-window-p on the mini-window.
    (push (format "window-end=%s" (l215-safe (lambda () (window-end mb)))) lines)
    (push (format "window-end-update=%s" (l215-safe (lambda () (window-end mb t)))) lines)
    (push (format "pos-visible=%s"
                  (l215-safe (lambda () (pos-visible-in-window-p (point-min) mb t))))
          lines)
    ;; 4. The motion engines, run with the mini-window selected and its own
    ;;    buffer current.  GNU walks the buffer; a matrix has no say.
    (dolist (probe '(("vm0" . 0) ("vm1" . 1) ("vm-1" . -1)))
      (push (format "vertical-motion %s %s" (car probe)
                    (l215-safe
                     (lambda ()
                       (with-selected-window mb
                         (with-current-buffer (window-buffer mb)
                           (goto-char (point-min))
                           (list (vertical-motion (cdr probe)) (point)))))))
            lines))
    (push (format "count-screen-lines %s"
                  (l215-safe
                   (lambda ()
                     (with-selected-window mb
                       (with-current-buffer (window-buffer mb)
                         (count-screen-lines (point-min) (point-max)))))))
          lines)
    (push (format "compute-motion %s"
                  (l215-safe
                   (lambda ()
                     (with-selected-window mb
                       (with-current-buffer (window-buffer mb)
                         (compute-motion (point-min) '(0 . 0) (point-max) '(80 . 1)
                                         80 nil mb))))))
          lines)
    ;; 5. window-line-height reads the same rows.
    (push (format "window-line-height-0 %s"
                  (l215-safe (lambda () (window-line-height 0 mb))))
          lines)
    (make-directory (file-name-directory (expand-file-name l215-out)) t)
    (with-temp-file l215-out
      (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))))

(l215-run)
(kill-emacs)

;;; l215-minibuffer-source-probe.el ends here
