;;; Ledger 217 -- does a message survive a minibuffer session?
;;;
;;; GNU `read_minibuf' calls `clear_message (1, 1)' (src/minibuf.c:894) after
;;; installing the prompt and BEFORE `run_hook (Qminibuffer_setup_hook)'
;;; (src/minibuf.c:900), so both the current message and the last displayed one
;;; are gone by the time any Lisp in the session runs.  Ledger 215 measured the
;;; port keeping the message, but only through an ABNORMAL exit (a throw out of
;;; the setup hook), and said so.  This probe drives NORMAL sessions -- keyboard
;;; input via `unread-command-events', a real RET -- and reads `current-message'
;;; before entry, inside the setup hook, and after the session returns.
(defvar l217-out (or (getenv "L217_OUT") "./tmp/l217/mini-msg.txt"))
(defvar l217-lines nil)
(defun l217-p (fmt &rest args) (push (apply #'format fmt args) l217-lines))
(defun l217-echo-text (n)
  (and (get-buffer n) (with-current-buffer n (buffer-string))))
(defun l217-snap (tag)
  (l217-p "%-26s current-message=%S echo0=%S echo1=%S" tag (current-message)
          (l217-echo-text " *Echo Area 0*") (l217-echo-text " *Echo Area 1*")))
(defun l217-session (tag keys)
  "Run one NORMAL minibuffer session, feeding KEYS, and record the message state."
  (let ((inside 'unset) (result 'unset))
    (setq result
          (condition-case err
              (progn
                (setq unread-command-events (listify-key-sequence (kbd keys)))
                (minibuffer-with-setup-hook
                    (lambda ()
                      ;; Read the message twice: as the hook is entered, and
                      ;; again after a redisplay inside the live session, so a
                      ;; stale one that a redisplay would clear is separated
                      ;; from one that stands for the whole session.
                      (setq inside (list (current-message)
                                         (progn (redisplay t) (current-message)))))
                  (read-from-minibuffer "L217: ")))
            (error (format "ERR:%S" (car err)))
            (quit "QUIT")))
    (l217-p "%-26s in-setup-hook (entry redisplayed) current-message=%S" tag inside)
    (l217-p "%-26s result=%S" tag result)
    (l217-snap (concat tag " after-exit"))))
(defun l217-run ()
  (delete-other-windows)
  (redisplay t)
  (l217-snap "0-startup")
  ;; Session 1: the startup banner is the standing message.
  (l217-session "1-banner" "a b c RET")
  ;; Session 2: an explicit message of our own, displayed first.
  (message "l217 standing message")
  (redisplay t)
  (l217-snap "2-before-entry")
  (l217-session "2-own-message" "x y RET")
  ;; Session 3: message set but NOT redisplayed before entry.
  (message "l217 undisplayed message")
  (l217-snap "3-before-entry")
  (l217-session "3-undisplayed" "q RET")
  ;; Session 4: a message issued from INSIDE the session -- GNU's clear is at
  ;; entry, so this one is the current message when the session ends.
  (message "l217 pre-inside message")
  (redisplay t)
  (setq unread-command-events (listify-key-sequence (kbd "z RET")))
  (l217-p "4-inside result=%S"
          (condition-case err
              (minibuffer-with-setup-hook
                  (lambda () (message "l217 message from inside the session"))
                (read-from-minibuffer "L217: "))
            (error (format "ERR:%S" (car err)))))
  (l217-snap "4-inside after-exit")
  ;; Session 5: aborted with C-g.
  (message "l217 message before abort")
  (redisplay t)
  (l217-session "5-abort" "d e C-g")
  (make-directory (file-name-directory (expand-file-name l217-out)) t)
  (with-temp-file l217-out (insert (mapconcat #'identity (nreverse l217-lines) "\n") "\n")))
(l217-run)
(kill-emacs)
