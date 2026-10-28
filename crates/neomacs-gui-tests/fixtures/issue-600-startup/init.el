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
;; Logical startup can finish before the native renderer publishes pixels.
;; Its PNG is atomically renamed into place only after encoding completes.
(defvar issue-600-capture-timer nil)
(setq issue-600-capture-timer
      (run-with-timer
       0 0.05
       (lambda ()
         (let ((png (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")))
           (when (and (> issue-600-window-setup-count 0)
                      (or (not (fboundp 'neomacs--write-frame-snapshot))
                          (and png (file-exists-p png)
                               (> (file-attribute-size (file-attributes png)) 0)
                               (not (time-less-p
                                     (file-attribute-modification-time (file-attributes png))
                                     before-init-time)))))
             (cancel-timer issue-600-capture-timer)
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
               (error (message "Issue 600 startup: %S" err) (kill-emacs 1))))))))
