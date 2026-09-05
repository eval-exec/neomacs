;;; primary-selection.el --- PRIMARY ownership across deactivate-mark -*- lexical-binding: t -*-

;; GNU `deactivate-mark' (emacs-31.0.90 lisp/simple.el:7056-7066) republishes
;; the region to PRIMARY only when this Emacs owns PRIMARY or nobody does.
;; On a display whose PRIMARY is process-local (the NS private pasteboard,
;; nsselect.m:494-511; the w32 Lisp property, w32-win.el:449-451) that owner
;; test must answer t after our own `gui-set-selection', so a later region
;; replaces the earlier value instead of leaving it stale.

(defun neomacs-primary-selection-probe ()
  (let (owner-before owned-before after-deactivate owner-after owner-p exists-p
        empty-owner empty-exists-p after-disown disown-tested error-text)
    (condition-case err
        (progn
          (gui-set-selection 'PRIMARY "old")
          (setq owner-before (neomacs-primary-selection-owner)
                owned-before (and (gui-backend-selection-owner-p 'PRIMARY) t))
          (with-temp-buffer
            (insert "new")
            (push-mark (point-min) t t)
            (goto-char (point-max))
            (deactivate-mark))
          (setq after-deactivate (gui-get-selection 'PRIMARY)
                owner-after (neomacs-primary-selection-owner)
                owner-p (and (gui-backend-selection-owner-p 'PRIMARY) t)
                exists-p (and (gui-backend-selection-exists-p 'PRIMARY) t))
          ;; GNU NS and w32 both consider an empty value to be an existing,
          ;; owned selection.
          (gui-set-selection 'PRIMARY "")
          (setq empty-owner (neomacs-primary-selection-owner)
                empty-exists-p
                (and (gui-backend-selection-exists-p 'PRIMARY) t))
          ;; Current Linux backends cannot observe the owner; native Wayland
          ;; also cannot explicitly disown a selection.  Keep those limitations
          ;; separate from the process-local PRIMARY contract exercised here.
          (unless (eq empty-owner 'unknown)
            (setq disown-tested t)
            (gui-set-selection 'PRIMARY nil)
            (setq after-disown
                  (list (gui-get-selection 'PRIMARY)
                        (neomacs-primary-selection-owner)
                        (and (gui-backend-selection-owner-p 'PRIMARY) t)))))
      (error (setq error-text (error-message-string err))))
    (let ((path (getenv "NEOMACS_GUI_STATE_JSON")))
      (when path
        (make-directory (file-name-directory path) t)
        (with-temp-file path
          (insert
           (json-serialize
            (list :window_system (symbol-name window-system)
                  :select_active_regions (if select-active-regions t :false)
                  :owner_before (symbol-name owner-before)
                  :owned_before (if owned-before t :false)
                  :after_deactivate (or after-deactivate :null)
                  :owner_after (symbol-name owner-after)
                  :owner_p (if owner-p t :false)
                  :exists_p (if exists-p t :false)
                  :empty_owner (symbol-name empty-owner)
                  :empty_exists_p (if empty-exists-p t :false)
                  :disown_tested (if disown-tested t :false)
                  :after_disown_value (or (car after-disown) :null)
                  :after_disown_owner
                  (if (cadr after-disown)
                      (symbol-name (cadr after-disown))
                    :null)
                  :after_disown_owner_p (if (caddr after-disown) t :false)
                  :error (or error-text :null)))
           "\n"))))
    (kill-emacs 0)))

(run-at-time 2 nil #'neomacs-primary-selection-probe)

;;; primary-selection.el ends here
