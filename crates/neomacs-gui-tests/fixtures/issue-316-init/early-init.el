;;; early-init.el --- issue #316 startup lifecycle regression -*- lexical-binding: t -*-

(defvar neomacs-issue-316-early-init-count 0)
(setq neomacs-issue-316-early-init-count
      (1+ neomacs-issue-316-early-init-count))

;; Doom installs the equivalent deferred TTY initialization only when GUI
;; early init incorrectly observes a nil initial-window-system.  Capturing the
;; selected frame here reproduces the original dead-bootstrap-frame failure.
(unless initial-window-system
  (add-hook 'window-setup-hook
            (apply-partially #'tty-run-terminal-initialization
                             (selected-frame) nil t)))

;;; early-init.el ends here
