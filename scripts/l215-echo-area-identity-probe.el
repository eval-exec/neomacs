;;; l215-echo-area-identity-probe.el --- the echo area follows its BUFFER -*- lexical-binding: t -*-

;; Ledger 215.  Ledger 209 measured, as a side note to the mini-window posn
;; question, that GNU and this port put the same message in DIFFERENT
;; echo-area buffers, and recorded it as "not a posn defect".  It is a defect
;; of its own, and this probe isolates its mechanism rather than its symptom.
;;
;; GNU holds the two echo-area buffers as Lisp OBJECTS in `echo_buffer[2]'
;; (src/xdisp.c:785) and `ensure_echo_area_buffers' (src/xdisp.c:12862-12884)
;; replaces one only when it has DIED.  So:
;;
;;   * renaming an echo buffer cannot detach the echo area from it -- the next
;;     message lands in the renamed buffer and no fresh " *Echo Area 0*" is
;;     manufactured;
;;   * and a user buffer that afterwards takes the freed name is NOT the echo
;;     area, so messages must leave it alone.
;;
;; A port that looks the buffer up BY NAME on every message gets both wrong,
;; and the second one destroys the user's buffer contents.

(defvar l215-out (or (getenv "L215_OUT") "./tmp/l215/echo-area-identity.txt"))
(defvar l215-lines nil)
(defun l215-safe (thunk)
  (condition-case err (format "%S" (funcall thunk)) (error (format "ERR:%S" (car err)))))
(defun l215-echo-state (tag)
  (push (format "%s current-message=%S" tag (current-message)) l215-lines)
  (dolist (name '(" *Echo Area 0*" " *Echo Area 1*" " *Echo Area RENAMED*"))
    (push (format "%s buffer %s exists=%S text=%S" tag name
                  (and (get-buffer name) t)
                  (and (get-buffer name)
                       (with-current-buffer name
                         (buffer-substring-no-properties (point-min) (min (point-max) 30)))))
          l215-lines)))
(defun l215-run ()
  (delete-other-windows)
  (message "l215 first message")
  (redisplay t)
  (l215-echo-state "1-before-rename")
  ;; Rename the buffer the echo area is using.  GNU holds `echo_buffer[i]' as a
  ;; Lisp OBJECT (src/xdisp.c:12862-12884) and only re-creates it when it dies,
  ;; so a rename cannot detach the echo area from it.
  (push (format "rename %s"
                (l215-safe (lambda ()
                             (with-current-buffer " *Echo Area 0*"
                               (rename-buffer " *Echo Area RENAMED*")))))
        l215-lines)
  (message "l215 second message")
  (redisplay t)
  (l215-echo-state "2-after-rename")
  ;; Now put a USER buffer under the name the port looks up.  In GNU nothing
  ;; touches it; a name-keyed port writes every message into it.
  (push (format "create-user-buffer %s"
                (l215-safe (lambda ()
                             (with-current-buffer (get-buffer-create " *Echo Area 0*")
                               (insert "PRECIOUS USER CONTENT")
                               (point-max)))))
        l215-lines)
  (message "l215 third message")
  (redisplay t)
  (l215-echo-state "3-after-user-buffer")
  (push (format "user-buffer-survived=%S"
                (with-current-buffer " *Echo Area 0*"
                  (string= (buffer-substring-no-properties (point-min) (point-max))
                           "PRECIOUS USER CONTENT")))
        l215-lines)
  (make-directory (file-name-directory (expand-file-name l215-out)) t)
  (with-temp-file l215-out (insert (mapconcat #'identity (nreverse l215-lines) "\n") "\n")))
(l215-run)
(kill-emacs)

;;; l215-echo-area-identity-probe.el ends here
