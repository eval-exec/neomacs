;;; -*- lexical-binding: t; -*-
(setq create-lockfiles nil auto-save-list-file-name nil)
(setq selection-base (get-buffer-create "selection-base"))
(setq selection-child (make-indirect-buffer selection-base "selection-child"))
(switch-to-buffer selection-child)
(setq selection-before (eq (current-buffer) selection-child))
(setq selection-kill-result (kill-buffer selection-base))
(setq selection-current (current-buffer))
(setq selection-observation (format "SELECTION %S\n"
               (list :before-child selection-before :kill-result selection-kill-result
                     :base-live (buffer-live-p selection-base)
                     :child-live (buffer-live-p selection-child)
                     :current-live (buffer-live-p selection-current)
                     :current-name (buffer-name selection-current)
                     :selected-window-live (buffer-live-p (window-buffer (selected-window)))
                     :current-matches-window (eq selection-current (window-buffer (selected-window))))))
(write-region selection-observation nil (getenv "SHUTDOWN_LOG") nil 'silent)
(kill-emacs 0)
