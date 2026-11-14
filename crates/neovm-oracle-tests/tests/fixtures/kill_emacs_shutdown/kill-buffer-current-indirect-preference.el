;;; Draft parity fixture: land only after actual revised-core verify passes.
(defun selection-record (text)
  (write-region text nil (getenv "SHUTDOWN_LOG") t 'silent))
;;; -*- lexical-binding: t; -*-
(setq create-lockfiles nil auto-save-list-file-name nil)
(setq selection-base (get-buffer-create "selection-base"))
(setq selection-child (make-indirect-buffer selection-base "selection-child"))
(setq selection-preferred (get-buffer-create "selection-preferred"))
(setq selection-shown (get-buffer-create "selection-shown"))
(switch-to-buffer selection-shown)
(set-frame-parameter nil 'buffer-list (list selection-preferred selection-shown selection-child))
(set-buffer selection-child)
(setq selection-window-before (window-buffer (selected-window)))
(setq selection-other-before (other-buffer selection-child))
(selection-record (format "BEFORE %S\n"
               (list :current-child (eq (current-buffer) selection-child)
                     :window (buffer-name selection-window-before)
                     :other (buffer-name selection-other-before)
                     :different (not (eq selection-window-before selection-other-before)))))
(setq selection-result (kill-buffer selection-base))
(selection-record (format "AFTER %S\n"
               (list :result selection-result
                     :current-live (buffer-live-p (current-buffer))
                     :current (buffer-name (current-buffer))
                     :window (buffer-name (window-buffer (selected-window)))
                     :current-eq-before-other (eq (current-buffer) selection-other-before)
                     :current-eq-window (eq (current-buffer) (window-buffer (selected-window))))))
(setq selection-other-after-dead (other-buffer selection-child))
(selection-record (format "DEAD-OTHER %S\n"
               (list :child-live (buffer-live-p selection-child)
                     :other (buffer-name selection-other-after-dead)
                     :same-as-before (eq selection-other-after-dead selection-other-before))))
(kill-emacs 0)
