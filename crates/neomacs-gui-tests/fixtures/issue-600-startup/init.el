;;; Exercise the public startup hook after early frame settings. -*- lexical-binding: t; -*-
(setq inhibit-startup-screen (not (getenv "NEOMACS_GUI_SHOW_STARTUP_SCREEN")))
(defvar issue-600-window-setup-count 0)
(defvar issue-600-command-line-ran nil)
(defvar issue-600-window-setup-state nil)
(add-hook 'window-setup-hook
          (lambda ()
            (setq issue-600-window-setup-count (1+ issue-600-window-setup-count)
                  issue-600-window-setup-state
                  (list (display-graphic-p)
                        (frame-parameter nil 'menu-bar-lines)
                        (frame-parameter nil 'tool-bar-lines)
                        (frame-parameter nil 'vertical-scroll-bars)
                        issue-600-command-line-ran))))
(run-with-timer
 2 nil
 (lambda ()
   (condition-case err
       (progn
         (redisplay t)
         (when (fboundp 'neomacs--write-frame-snapshot)
           (neomacs--write-frame-snapshot
            (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json))
         (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
           (insert (format "{\"early_init_count\":%d,\"setup_count\":%d,\"setup\":%S}\n"
                           issue-600-early-init-count
                           issue-600-window-setup-count
                           (format "%S" issue-600-window-setup-state))))
         (kill-emacs 0))
     (error (message "Issue 600 startup: %S" err) (kill-emacs 1)))))
