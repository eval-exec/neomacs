;;; l205-minibuffer-probe.el --- what the mini-window answers, and why -*- lexical-binding: t -*-

;; Ledger 205, residual 5.  scripts/below-content-audit.el measures the minibuffer
;; window answering buffer positions 1, 2, 6, 41 and 66 for columns 0, 1, 5, 40
;; and 79 of an EMPTY minibuffer buffer, where GNU answers 1 in every column.
;; This prints the two facts that identify what those positions are: the
;; mini-window's buffer and its size, and the echo-area message on screen.

(defvar l205-mb-out (or (getenv "L205_OUT") "./tmp/l205/minibuffer-probe.txt"))

(defun l205-mb-run ()
  (redisplay t)
  (let* ((mb (minibuffer-window))
         (buf (window-buffer mb))
         (msg (current-message))
         (lines '()))
    (push (format "mini-window-buffer=%S pmax=%S"
                  (buffer-name buf)
                  (with-current-buffer buf (point-max)))
          lines)
    (push (format "current-message=%S length=%S"
                  msg (and msg (length msg)))
          lines)
    (dolist (x '(0 1 5 40 64 65 79))
      (push (format "x=%d posn=%S" x
                    (let ((p (posn-at-x-y x 0 mb)))
                      (and p (list (posn-point p) (posn-col-row p)
                                   (posn-actual-col-row p) (posn-area p)))))
            lines))
    (make-directory (file-name-directory (expand-file-name l205-mb-out)) t)
    (with-temp-file l205-mb-out
      (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))))

(l205-mb-run)
(kill-emacs)

;;; l205-minibuffer-probe.el ends here
