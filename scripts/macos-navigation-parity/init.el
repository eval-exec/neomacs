;;; init.el --- Paired issue #607 workload witness -*- lexical-binding: t; -*-

(require 'json)
(setq inhibit-startup-screen t)
(defvar navigation-bench-dir (getenv "NAVIGATION_BENCH_DIR"))
(defvar navigation-bench-mode (getenv "NAVIGATION_BENCH_MODE"))
(defvar navigation-bench-buffer nil)
(defvar navigation-bench-timer nil)
(defvar navigation-bench-moves 0)
(defvar navigation-bench-queued 0)
(defvar navigation-bench-start-wall nil)
(defvar navigation-bench-start-cpu nil)
(defvar navigation-bench-start-gcs nil)
(defvar navigation-bench-finished nil)

(defun navigation-bench-write (name data)
  (let* ((path (expand-file-name name navigation-bench-dir))
         (pending (concat path ".pending")))
    (with-temp-file pending (insert (json-encode data)))
    (rename-file pending path t)))

(defun navigation-bench-finish ()
  (unless navigation-bench-finished
    (setq navigation-bench-finished t)
    (when navigation-bench-timer (cancel-timer navigation-bench-timer))
    (let* ((end-wall (float-time))
           (end-cpu (current-cpu-time))
           (elapsed (- end-wall navigation-bench-start-wall))
           (cpu-seconds (- (/ (float (car end-cpu)) (cdr end-cpu))
                           (/ (float (car navigation-bench-start-cpu))
                              (cdr navigation-bench-start-cpu)))))
      (navigation-bench-write
       "done.json"
       `((mode . ,navigation-bench-mode) (pid . ,(emacs-pid))
         (moves . ,navigation-bench-moves) (queued . ,navigation-bench-queued)
         (final_line . ,(line-number-at-pos)) (point . ,(point))
         (window_start . ,(window-start)) (major_mode . ,(symbol-name major-mode))
         (elapsed_seconds . ,elapsed) (cpu_seconds . ,cpu-seconds)
         (cpu_percent . ,(* 100.0 (/ cpu-seconds elapsed)))
         (gc_count . ,(- gcs-done navigation-bench-start-gcs))
         (start_wall . ,navigation-bench-start-wall) (end_wall . ,end-wall))))
    (when (equal (getenv "NAVIGATION_BENCH_EXIT") "1")
      (run-at-time 0.5 nil (lambda () (kill-emacs 0))))))

(defun navigation-bench-post-command ()
  (when (and navigation-bench-start-wall
             (not navigation-bench-finished)
             (eq this-command 'next-line))
    (setq navigation-bench-moves (1+ navigation-bench-moves))
    (when (= navigation-bench-moves 260) (navigation-bench-finish))))

(defun navigation-bench-tick ()
  (when (and (not navigation-bench-finished)
             (file-exists-p (expand-file-name "go" navigation-bench-dir)))
    (with-current-buffer navigation-bench-buffer
      (unless navigation-bench-start-wall
        (setq navigation-bench-start-wall (float-time)
              navigation-bench-start-cpu (current-cpu-time)
              navigation-bench-start-gcs gcs-done)
        (navigation-bench-write "running.json" `((pid . ,(emacs-pid)))))
      (cond
       ((equal navigation-bench-mode "timer")
        ;; Issue #607's original arm: logical motion and explicit redisplay.
        (if (< (line-number-at-pos) 261)
            (progn
              (forward-line 1)
              (setq navigation-bench-moves (1+ navigation-bench-moves))
              (let ((window (get-buffer-window navigation-bench-buffer t)))
                (when window
                  (set-window-point window (point))
                  (with-selected-window window (redisplay t)))))
          (navigation-bench-finish)))
       ((equal navigation-bench-mode "synthetic-cn")
        ;; A paced C-n through the ordinary command reader. This preserves
        ;; command history and normal redisplay; it is not native autorepeat.
        (when (< navigation-bench-queued 260)
          (setq unread-command-events
                (append unread-command-events (list 14)))
          (setq navigation-bench-queued (1+ navigation-bench-queued))))))))

(find-file (getenv "NAVIGATION_BENCH_FILE"))
(goto-char (point-min))
(set-window-start (selected-window) (point-min))
(setq navigation-bench-buffer (current-buffer))
(add-hook 'post-command-hook #'navigation-bench-post-command)
(navigation-bench-write
 "ready.json"
 `((pid . ,(emacs-pid)) (mode . ,navigation-bench-mode)
   (window_system . ,(format "%s" window-system))
   (major_mode . ,(symbol-name major-mode)) (font . ,(format "%s" (frame-parameter nil 'font)))
   (pixel_width . ,(frame-pixel-width)) (pixel_height . ,(frame-pixel-height))
   (first_line . ,(line-number-at-pos))
   (last_line . ,(line-number-at-pos (point-max)))))
(setq navigation-bench-timer (run-at-time 0.1 0.1 #'navigation-bench-tick))
;; Reap only this independently launched editor if the controller disappears.
(run-at-time 180 nil (lambda () (kill-emacs 2)))
