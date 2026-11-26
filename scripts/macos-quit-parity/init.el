;;; init.el --- Native quit-route/confirmation witness -*- lexical-binding: t -*-
(require 'json)
(setq ns-confirm-quit (equal (getenv "NEOMACS_QUIT_CONFIRM") "t"))
(let ((modifier (getenv "NEOMACS_QUIT_COMMAND_MODIFIER")))
  (when (member modifier '("super" "meta"))
    (setq ns-command-modifier (intern modifier))))
(defun neomacs-quit-witness (event)
  (with-temp-file (getenv "NEOMACS_QUIT_RESULT")
    (insert event))
  (with-temp-file (concat (getenv "NEOMACS_QUIT_RESULT") ".state")
    (insert (json-encode `((route . ,event)
                           (last-nonmenu-event . ,(format "%S" last-nonmenu-event)))))))
(global-set-key [?\s-q] (lambda () (interactive) (neomacs-quit-witness "super-q")))
(global-set-key [?\M-q] (lambda () (interactive) (neomacs-quit-witness "meta-q")))
(when (equal (getenv "NEOMACS_QUIT_UNBIND_SUPER_Q") "t")
  (global-unset-key [?\s-q]))
(global-set-key [ns-power-off] (lambda () (interactive) (neomacs-quit-witness "ns-power-off")))
;; GNU's external terminate: queues a keyboard event without stopping the
;; current NSApp run loop. Keep an explicit diagnostic arm that allows the
;; usual Lisp timer wakeups to drain it; the idle arm remains available.
(when (equal (getenv "NEOMACS_QUIT_HEARTBEAT") "t")
  (run-at-time 0.1 0.1 #'ignore))
(run-at-time
 0.5 nil
 (lambda ()
   (with-temp-file (getenv "NEOMACS_QUIT_READY")
     (insert (json-encode `((pid . ,(emacs-pid))
                            (window-system . ,(format "%s" window-system))
                            (confirm . ,ns-confirm-quit)
                            (last-nonmenu-event . ,(format "%S" last-nonmenu-event))
                            (command-modifier . ,(format "%s" ns-command-modifier))))))))
(run-at-time 30 nil (lambda () (kill-emacs 0)))
(when (equal (getenv "NEOMACS_QUIT_PREFIX") "f6")
  ;; Only the ordinary prefix is unread. The following non-key event still
  ;; comes from the real native terminate/Apple Event path, preserving its
  ;; provenance rather than simulating it with unread-command-events.
  (let ((map (make-sparse-keymap)))
    (define-key map [ns-power-off] (lookup-key global-map [ns-power-off]))
    (global-set-key [f6] map))
  (setq unread-command-events (cons 'f6 unread-command-events)))
