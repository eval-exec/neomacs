;;; shift-selection.el --- Issue #558 -*- lexical-binding: t -*-

(run-at-time
 1 nil
 (lambda ()
   (condition-case err
       (progn
         (switch-to-buffer (get-buffer-create "shift-selection"))
         (insert "abcdefgh\n")
         (goto-char 1)
         (setq shift-select-mode t)
         (transient-mark-mode 1)
         (set-face-background 'region "red")
         (setq-local mode-line-format nil)
         ;; Capture without an intervening unshifted motion command.
         (add-hook
          'post-command-hook
          (lambda ()
            (when (memq this-command '(right-char forward-char))
              (condition-case err
                  (progn
                    (unless (and mark-active (= (mark) 1)
                                 (equal (buffer-substring-no-properties
                                         (region-beginning) (region-end))
                                        (substring "abcd" 0 (1- (point)))))
                      (error "Shifted region: point=%S mark=%S active=%S"
                             (point) (mark t) mark-active))
                    (when (= (point) 5)
                      (redisplay t)
                      (neomacs--write-frame-snapshot
                       (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json)
                      (run-at-time
                       1 nil
                       (lambda ()
                         (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
                                    (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".png") t)
                         (kill-emacs 0)))))
                (error (message "Shift selection: %S" err) (kill-emacs 1)))))
          nil t)
         (with-temp-file (getenv "NEOMACS_SELECTION_READY") (insert "ready")))
     (error (message "Shift selection setup: %S" err) (kill-emacs 1)))))
(run-at-time 12 nil (lambda () (message "Shift selection watchdog expired") (kill-emacs 2)))
