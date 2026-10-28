;;; line-navigation.el --- Issue #607 timer-driven navigation -*- lexical-binding: t; -*-

(switch-to-buffer (get-buffer-create "*line-navigation*"))
(dotimes (i 260)
  (insert (format "Line %03d: line navigation performance benchmark text.\n" i)))
(goto-char (point-min))
(defvar line-navigation-timer nil)
(setq line-navigation-timer
      (run-with-timer
       3 0.1
       (lambda ()
         (if (< (line-number-at-pos) 261)
             (progn
               (forward-line 1)
               (set-window-point (selected-window) (point))
               (redisplay t))
           (cancel-timer line-navigation-timer)
           ;; Capture point before with-temp-file changes the current buffer.
           (let ((line (line-number-at-pos)))
             (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
               (insert (format "{\"final_line\":%d}\n" line))))
           (neomacs--write-frame-snapshot
            (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT") nil 'text)
           (run-with-timer 0.3 nil (lambda () (kill-emacs 0)))))))
