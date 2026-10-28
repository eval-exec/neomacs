;;; init.el --- issue #316 startup lifecycle regression -*- lexical-binding: t -*-

(switch-to-buffer (get-buffer-create "*neomacs-startup-lifecycle*"))
(erase-buffer)
(insert (format "early-init count: %d\ninitial-window-system: %S\n"
                neomacs-issue-316-early-init-count
                initial-window-system))

(let ((path (getenv "NEOMACS_GUI_STATE_JSON")))
  (when path
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert
       (format
        "{\"early_init_count\":%d,\"initial_window_system\":\"%s\"}\n"
        neomacs-issue-316-early-init-count
        (symbol-name initial-window-system))))))

;; The live Lisp frame precedes asynchronous GPU initialization. Finish when
;; its first complete PNG is published, rather than after a startup delay.
(defvar neomacs-issue-316-capture-timer nil)
(setq neomacs-issue-316-capture-timer
      (run-with-timer
       0 0.05
       (lambda ()
         (let ((png (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")))
           (when (or (not (fboundp 'neomacs--write-frame-snapshot))
                     (and png (file-exists-p png)
                          (> (file-attribute-size (file-attributes png)) 0)
                          (not (time-less-p
                                (file-attribute-modification-time (file-attributes png))
                                before-init-time))))
             (cancel-timer neomacs-issue-316-capture-timer)
             (kill-emacs 0))))))

;;; init.el ends here
