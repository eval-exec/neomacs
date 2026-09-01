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

(run-at-time 2 nil (lambda () (kill-emacs 0)))

;;; init.el ends here
