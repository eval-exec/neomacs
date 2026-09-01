;;; Ledger 217 -- what the SECOND echo-area buffer holds, step by step.
;;;
;;; GNU keeps two physical buffers (`echo_buffer[2]', src/xdisp.c:789) and a
;;; two-slot SELECTION over them (`echo_area_buffer[2]', src/xdisp.c:785):
;;; slot 0 is the current message, slot 1 the last displayed one.  `set_message'
;;; enters `with_echo_area_buffer' with WHICH < 0 (src/xdisp.c:13570), which
;;; picks the physical buffer slot 1 is NOT holding (src/xdisp.c:12933-12941) so
;;; the new message cannot overwrite the last displayed one; `echo_area_display'
;;; then closes with `echo_area_buffer[1] = echo_area_buffer[0]'
;;; (src/xdisp.c:13795).  The two buffers therefore alternate, and this probe
;;; reads that alternation out from Lisp.
(defvar l217-out (or (getenv "L217_OUT") "./tmp/l217/echo-two.txt"))
(defvar l217-lines nil)
(defun l217-p (fmt &rest args) (push (apply #'format fmt args) l217-lines))
(defun l217-safe (thunk)
  (condition-case err (format "%S" (funcall thunk)) (error (format "ERR:%S" (car err)))))
(defun l217-snap (tag)
  (l217-p "%-22s current-message=%S" tag (current-message))
  (dolist (n '(" *Echo Area 0*" " *Echo Area 1*"))
    (l217-p "%-22s %-16s exists=%S text=%S" tag n
            (and (get-buffer n) t)
            (and (get-buffer n) (with-current-buffer n (buffer-string)))))
  (l217-p "%-22s mini-window-buffer=%S" tag
          (buffer-name (window-buffer (minibuffer-window)))))
(defun l217-run ()
  (delete-other-windows)
  (redisplay t)
  (l217-snap "0-startup")
  (message "l217 message A")
  (l217-snap "1-A-no-redisplay")
  (redisplay t)
  (l217-snap "2-A-redisplayed")
  (message "l217 message B")
  (l217-snap "3-B-no-redisplay")
  (redisplay t)
  (l217-snap "4-B-redisplayed")
  (message "l217 message C")
  (redisplay t)
  (l217-snap "5-C-redisplayed")
  (message "l217 message D")
  (redisplay t)
  (l217-snap "6-D-redisplayed")
  (message nil)
  (redisplay t)
  (l217-snap "7-cleared")
  (message "l217 message E")
  (redisplay t)
  (l217-snap "8-E-after-clear")
  ;; The echo buffers are ordinary buffers: what does buffer-list say, and are
  ;; they live, and what are their local settings?
  (l217-p "buffer-list-echo=%S"
          (delq nil (mapcar (lambda (b)
                              (and (string-match-p "Echo Area" (buffer-name b))
                                   (buffer-name b)))
                            (buffer-list))))
  (dolist (n '(" *Echo Area 0*" " *Echo Area 1*"))
    (when (get-buffer n)
      (with-current-buffer n
        (l217-p "%-16s truncate-lines=%S undo=%S multibyte=%S point-max=%S" n
                truncate-lines (eq buffer-undo-list t)
                enable-multibyte-characters (point-max)))))
  (make-directory (file-name-directory (expand-file-name l217-out)) t)
  (with-temp-file l217-out (insert (mapconcat #'identity (nreverse l217-lines) "\n") "\n")))
(l217-run)
(kill-emacs)
