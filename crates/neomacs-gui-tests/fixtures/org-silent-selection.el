;;; org-silent-selection.el --- Issue #379 -*- lexical-binding: t -*-

(require 'org)
(load (expand-file-name "org-typora-markers.el"
                        (file-name-directory load-file-name)) nil t)

(run-at-time
 1 nil
 (lambda ()
   (condition-case err
       (progn
         (switch-to-buffer (get-buffer-create "region.org"))
         (org-mode)
         (insert "Plain first\n*bold* and /italic/\nPlain last\n")
         (setq-local org-hide-emphasis-markers t)
         (font-lock-ensure)
         (goto-char (point-min))
         (set-face-background 'region "red")
         ;; Rust supplies real native keyboard input after readiness.
         (local-set-key
          (kbd "C-c t")
          (lambda ()
            (interactive)
            (condition-case err
                (progn
                  (unless (region-active-p)
                    (error "Region deactivated by silent emphasis-marker updates"))
                  (redisplay t)
                  (neomacs--write-frame-snapshot
                   (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
                  (run-at-time
                   1 nil
                   (lambda ()
                     (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
                                (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".png") t)
                     (kill-emacs 0))))
              (error (message "Org selection capture: %S" err) (kill-emacs 1)))))
         (with-temp-file (getenv "NEOMACS_SELECTION_READY") (insert "ready")))
     (error (message "Org silent selection regression: %S" err)
            (kill-emacs 1)))))
