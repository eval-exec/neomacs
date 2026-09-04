;;; primary-selection.el --- PRIMARY ownership across deactivate-mark -*- lexical-binding: t -*-

;; GNU `deactivate-mark' (emacs-31.0.90 lisp/simple.el:7056-7066) republishes
;; the region to PRIMARY only when this Emacs owns PRIMARY or nobody does.
;; On a display whose PRIMARY is process-local (the NS private pasteboard,
;; nsselect.m:494-511; the w32 Lisp property, w32-win.el:449-451) that owner
;; test must answer t after our own `gui-set-selection', so a later region
;; replaces the earlier value instead of leaving it stale.

(defun neomacs-primary-selection-probe ()
  (let (owned-before after-deactivate owner-p exists-p after-disown error-text)
    (condition-case err
        (progn
          (gui-set-selection 'PRIMARY "old")
          (setq owned-before (and (gui-backend-selection-owner-p 'PRIMARY) t))
          (with-temp-buffer
            (insert "new")
            (push-mark (point-min) t t)
            (goto-char (point-max))
            (deactivate-mark))
          (setq after-deactivate (gui-get-selection 'PRIMARY)
                owner-p (and (gui-backend-selection-owner-p 'PRIMARY) t)
                exists-p (and (gui-backend-selection-exists-p 'PRIMARY) t))
          (gui-set-selection 'PRIMARY nil)
          (setq after-disown
                (list (gui-get-selection 'PRIMARY)
                      (and (gui-backend-selection-owner-p 'PRIMARY) t))))
      (error (setq error-text (error-message-string err))))
    (let ((path (getenv "NEOMACS_GUI_STATE_JSON")))
      (when path
        (make-directory (file-name-directory path) t)
        (with-temp-file path
          (insert
           (json-serialize
            (list :window_system (symbol-name window-system)
                  :select_active_regions (if select-active-regions t :false)
                  :owned_before (if owned-before t :false)
                  :after_deactivate (or after-deactivate :null)
                  :owner_p (if owner-p t :false)
                  :exists_p (if exists-p t :false)
                  :after_disown_value (or (car after-disown) :null)
                  :after_disown_owner_p (if (cadr after-disown) t :false)
                  :error (or error-text :null)))
           "\n"))))
    (kill-emacs 0)))

(run-at-time 2 nil #'neomacs-primary-selection-probe)

;;; primary-selection.el ends here
