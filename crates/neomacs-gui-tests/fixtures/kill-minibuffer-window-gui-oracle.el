;;; kill-minibuffer-window-gui-oracle.el --- GNU/Neomacs GUI kill-buffer oracle -*- lexical-binding: t -*-
;;
;; One `(NAME . STATE)` line per case, written to the path named by
;; NEOMACS_KILL_MINIBUFFER_ORACLE_RESULT. The test runs this fixture under both
;; editors on one X11 display and compares the files line by line.
;;
;; The contract pinned here, from GNU 31.1: `Fkill_buffer` checks, after the
;; query functions and `kill-buffer-hook` have run, whether the buffer is the
;; contents of `minibuf_window`; if so it returns nil without killing anything
;; (src/buffer.c:1998-2000, "Don't kill the minibuffer now current").
;; Killing it would leave an ordinary buffer displayed in the minibuffer
;; window. `ordinary` is the narrowness control: a buffer not displayed there
;; still dies and reports t.
;;
;; Errors propagate: a fixture that cannot reach the state under test must
;; fail the oracle instead of recording two identical error values that would
;; compare equal.

(defvar neo-kill-minibuffer-oracle-results nil)

(defun neo-kill-minibuffer-oracle--record (name thunk)
  (push (cons name (funcall thunk))
        neo-kill-minibuffer-oracle-results))

(neo-kill-minibuffer-oracle--record
 'refuse
 (lambda ()
   (let* ((mini (minibuffer-window))
          (original (window-buffer mini))
          (victim (generate-new-buffer "neb-gui-kill-mini-victim")))
     (unwind-protect
         (progn
           (set-window-buffer mini victim)
           (list (kill-buffer victim)
                 (buffer-live-p victim)
                 (buffer-name (window-buffer mini))))
       (when (buffer-live-p victim)
         (set-window-buffer mini original)
         (kill-buffer victim))))))

(neo-kill-minibuffer-oracle--record
 'ordinary
 (lambda ()
   (let ((buffer (generate-new-buffer "neb-gui-kill-mini-ordinary")))
     (list (kill-buffer buffer) (buffer-live-p buffer)))))

(let ((path (getenv "NEOMACS_KILL_MINIBUFFER_ORACLE_RESULT")))
  (with-temp-file path
    (dolist (result (reverse neo-kill-minibuffer-oracle-results))
      (prin1 result (current-buffer))
      (terpri (current-buffer)))))

(kill-emacs 0)
