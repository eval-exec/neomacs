;;; Ledger 217 -- what a message standing at minibuffer entry costs.
;;;
;;; `completion--done' (lisp/minibuffer.el:2691-2707) records `(current-message)'
;;; before calling a completion `:exit-function' and shows its own message only
;;; when the two are `equal':
;;;
;;;   (pre-msg (and exit-fun (current-message)))
;;;   ... (funcall exit-fun string finished)
;;;   (when (and message (equal pre-msg (and exit-fun (current-message))))
;;;     (completion--message message))
;;;
;;; GNU guarantees no message is standing inside a minibuffer session --
;;; `read_minibuf' calls `clear_message (1, 1)' at src/minibuf.c:894 -- so
;;; `pre-msg' is nil.  This port left the pre-session message standing.  The
;;; probe drives `completion--done' in BOTH states in the SAME editor, so the
;;; consequence is visible without needing the old binary: with an exit-function
;;; that clears the echo area, a standing message SUPPRESSES the completion
;;; message that GNU shows.
(defvar l217-out (or (getenv "L217_OUT") "./tmp/l217/reach.txt"))
(defvar l217-lines nil)
(defun l217-p (fmt &rest args) (push (apply #'format fmt args) l217-lines))
(defun l217-case (tag standing)
  (if standing (message "%s" standing) (message nil))
  (redisplay t)
  (let ((completion-extra-properties
         (list :exit-function (lambda (_s _st) (message nil)))))
    (completion--done "abc" 'finished "l217 completion message"))
  (l217-p "%-28s standing=%S -> current-message=%S" tag standing (current-message)))
(defun l217-run ()
  (delete-other-windows)
  (redisplay t)
  (require 'minibuffer)
  (l217-case "no-standing-message (GNU)" nil)
  (l217-case "standing-message (port was)" "l217 pre-session message")
  (make-directory (file-name-directory (expand-file-name l217-out)) t)
  (with-temp-file l217-out (insert (mapconcat #'identity (nreverse l217-lines) "\n") "\n")))
(l217-run)
(kill-emacs)
