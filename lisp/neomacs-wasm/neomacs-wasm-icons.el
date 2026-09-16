;;; neomacs-wasm-icons.el --- Shared browser Nerd Icons -*- lexical-binding: t; -*-

;;; Commentary:
;; The worker registers the font from the authenticated package assets before
;; Lisp startup.  No system-font installation or extra browser download is needed.

;;; Code:

(require 'tab-line)
(require 'nerd-icons)

(defun neomacs-wasm-icons-tab-name (buffer &optional _buffers)
  "Return BUFFER's tab label with its file or major-mode icon."
  (with-current-buffer buffer
    (let ((icon (nerd-icons-icon-for-buffer :height 0.9 :v-adjust 0.0)))
      (concat
       (when (stringp icon)
         ;; The default tab formatter sets the outer face.  A display string
         ;; preserves the icon's own font and color inside that tab face.
         (concat (propertize " " 'display icon) " "))
       (buffer-name buffer)))))

(defun neomacs-wasm-icons-initialize ()
  "Use the shared Nerd Icons font in the tree, tabs, and Dired."
  (if (not (member nerd-icons-font-family (font-family-list)))
      (message "Nerd Icons font unavailable; keeping ordinary labels")
    (require 'treemacs-nerd-icons)
    (treemacs-load-theme "nerd-icons")
    (require 'nerd-icons-dired)
    (add-hook 'dired-mode-hook #'nerd-icons-dired-mode)
    (setq tab-line-tab-name-function #'neomacs-wasm-icons-tab-name)
    (tab-line-force-update t)))

(provide 'neomacs-wasm-icons)
;;; neomacs-wasm-icons.el ends here
