;;; l209-echo-area-probe.el --- which buffer the mini-window's rows came from -*- lexical-binding: t -*-

;; Ledger 209, sizing ledger 205's residual 5.  `posn-at-x-y' on the mini-window
;; answers COLUMN+1 in this port and 1 in GNU, for a window whose own buffer is
;; empty.  This prints the buffer the echo area displays and its size, so
;; "those are the echo-area buffer's own indices" is measured rather than
;; inferred.
;;
;; GNU's `buffer_posn_from_coords' opens with `Fset_buffer (w->contents)'
;; (src/dispnew.c:6275) and walks the WINDOW's buffer, which is why GNU's answer
;; is that buffer's ZV no matter what its matrix holds.

(defvar l209-out (or (getenv "L209_OUT") "./tmp/l209/echo-area-probe.txt"))

(defun l209-run ()
  (redisplay t)
  (let ((mb (minibuffer-window))
        (lines '()))
    (push (format "window-buffer=%S pmax=%S"
                  (buffer-name (window-buffer mb))
                  (with-current-buffer (window-buffer mb) (point-max)))
          lines)
    (dolist (name '(" *Echo Area 0*" " *Echo Area 1*"))
      (push (format "%s exists=%S pmax=%S text=%S" name
                    (and (get-buffer name) t)
                    (and (get-buffer name)
                         (with-current-buffer name (point-max)))
                    (and (get-buffer name)
                         (with-current-buffer name
                           (buffer-substring-no-properties
                            (point-min) (min (point-max) 20)))))
            lines))
    (push (format "posn-at-x-y-5=%S"
                  (let ((p (posn-at-x-y 5 0 mb))) (and p (posn-point p))))
          lines)
    (make-directory (file-name-directory (expand-file-name l209-out)) t)
    (with-temp-file l209-out
      (insert (mapconcat #'identity (nreverse lines) "\n") "\n"))))

(l209-run)
(kill-emacs)

;;; l209-echo-area-probe.el ends here
