;;; issue621-mouse-drag-selection.el --- GNU/Neomacs GUI drag oracle -*- lexical-binding: t; -*-
;;
;; The host injects a real X11 pointer drag once NEOMACS_ISSUE621_READY
;; exists, then this fixture records the post-command state of the drag
;; command in NEOMACS_ISSUE621_RESULT.
;;
;; GNU's drag release is (drag-mouse-1 START END): START is a copy of the
;; down position (src/keyboard.c:6640, 6739-6741).  mouse-set-region then
;; sets mark to START and point to END.  Issue #621: Neomacs emitted
;; (drag-mouse-1 END), so mouse-set-region ran but left an empty region.

(menu-bar-mode -1)
(tool-bar-mode -1)
(when (fboundp 'scroll-bar-mode)
  (scroll-bar-mode -1))
(switch-to-buffer (get-buffer-create "issue621"))
(erase-buffer)
(dotimes (i 12)
  (insert (make-string 80 (+ ?a (% i 26))) "\n"))
(goto-char (point-min))
(set-window-start (selected-window) (point-min))
(setq issue621-state nil)
(defun issue621-finish ()
  (let ((state (copy-sequence issue621-state))
        (buffer (get-buffer "issue621")))
    ;; The command loop may run active-region cleanup after the hook, so
    ;; report the state the user observes when the timer fires.
    (when (and state buffer)
      (with-current-buffer buffer
        (setf (nth 1 state) (region-active-p))))
    (with-temp-file (getenv "NEOMACS_ISSUE621_RESULT")
      (prin1 state (current-buffer))))
  (kill-emacs 0))
(add-hook 'post-command-hook
          (lambda ()
            (when (memq this-command
                        '(mouse-drag-region mouse-set-region mouse-set-point))
              (let ((event last-command-event))
                (setq issue621-state
                      (list this-command
                            (region-active-p)
                            (ignore-errors (mark))
                            (point)
                            (and (consp event) (car event))
                            (and (consp event) (length event))
                            (and (consp event)
                                 (ignore-errors
                                   (posn-point (event-start event))))
                            (and (consp event)
                                 (ignore-errors
                                   (posn-point (event-end event)))))))
              ;; The drag release runs mouse-set-region; report promptly so
              ;; the test does not wait for the watchdog.
              (when (eq this-command 'mouse-set-region)
                (run-with-timer 0.5 nil #'issue621-finish)))))
(with-temp-file (getenv "NEOMACS_ISSUE621_READY")
  (insert "ready"))
;; Fallback: slow window mapping must not record nil before the drag.
;; Without a drag there is nothing to report, so exit non-zero instead of
;; writing a nil state that only surfaces later as a parse error.
(run-with-timer
 30 nil
 (lambda ()
   (if issue621-state
       (issue621-finish)
     (kill-emacs 2))))
