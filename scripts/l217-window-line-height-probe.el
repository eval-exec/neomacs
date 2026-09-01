;;; Ledger 217 -- `window-line-height' on the mini-window, and whether it moves
;;; with the echo area's buffer state.
;;;
;;; GNU `Fwindow_line_height' (src/window.c:2069-2115) refuses to answer at all
;;; -- returns nil -- unless the window's current matrix is up to date:
;;;   !w->window_end_valid || windows_or_buffers_changed || b->clip_changed
;;;   || b->prevent_redisplay_optimizations_p || window_outdated (w)
;;; (src/window.c:2077-2089).  There is no minibuffer special case in it.
;;; Ledger 215 recorded four divergent mini-window probes and none on normal
;;; windows.  This probe takes the whole matrix -- both window kinds, every LINE
;;; form, in each echo-area state -- so a change can be attributed.
(defvar l217-out (or (getenv "L217_OUT") "./tmp/l217/wlh.txt"))
(defvar l217-lines nil)
(defun l217-p (fmt &rest args) (push (apply #'format fmt args) l217-lines))
(defun l217-safe (thunk)
  (condition-case err (funcall thunk) (error (format "ERR:%S" (car err)))))
(defun l217-wlh (tag)
  (let ((mw (minibuffer-window))
        (nw (selected-window)))
    (dolist (spec (list (cons "mini" mw) (cons "normal" nw)))
      (dolist (line '(nil 0 1 header-line mode-line))
        (l217-p "%-22s %-6s line=%-12S %S" tag (car spec) line
                (l217-safe (lambda () (window-line-height line (cdr spec)))))))
    (l217-p "%-22s mini window-buffer=%S point-max=%S current-message=%S" tag
            (buffer-name (window-buffer mw))
            (with-current-buffer (window-buffer mw) (point-max))
            (current-message))
    (l217-p "%-22s mini window-start=%S window-end=%S" tag
            (l217-safe (lambda () (window-start mw)))
            (l217-safe (lambda () (window-end mw))))))
(defun l217-run ()
  (delete-other-windows)
  (redisplay t)
  (l217-wlh "0-startup")
  (message "l217 wlh displayed message")
  (redisplay t)
  (l217-wlh "1-message-displayed")
  (message nil)
  (redisplay t)
  (l217-wlh "2-message-cleared")
  (message "l217 wlh fresh message")
  (l217-wlh "3-fresh-no-redisplay")
  (redisplay t)
  (l217-wlh "4-fresh-redisplayed")
  ;; A buffer with several lines in the normal window, to show the normal-window
  ;; answers are real answers and not a constant.
  (switch-to-buffer (get-buffer-create "l217-body"))
  (erase-buffer)
  (dotimes (i 12) (insert (format "line %d\n" i)))
  (goto-char (point-min))
  (redisplay t)
  (l217-wlh "5-body-buffer")
  (make-directory (file-name-directory (expand-file-name l217-out)) t)
  (with-temp-file l217-out (insert (mapconcat #'identity (nreverse l217-lines) "\n") "\n")))
(l217-run)
(kill-emacs)
