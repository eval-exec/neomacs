;;; neomacs-wasm-icons.el --- Shared browser Nerd Icons -*- lexical-binding: t; -*-

;;; Commentary:
;; The worker registers the font from the authenticated package assets before
;; Lisp startup.  No system-font installation or extra browser download is needed.

;;; Code:

(require 'tab-line)

;; Optional browser packages are mounted at startup, not during native builds.
(defvar nerd-icons-font-family)
(defvar treemacs-nerd-icons-tab)
(declare-function nerd-icons-icon-for-buffer "ext:nerd-icons" (&rest arg-overrides))
(declare-function nerd-icons-completion-mode "ext:nerd-icons-completion" (&optional arg))
(declare-function nerd-icons-dired-mode "ext:nerd-icons-dired" (&optional arg))
(declare-function treemacs-load-theme "ext:treemacs-icons" (name))

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
  "Use the shared Nerd Icons font in the tree, tabs, Dired, and completion."
  (require 'nerd-icons)
  (if (not (member nerd-icons-font-family (font-family-list)))
      (message "Nerd Icons font unavailable; keeping ordinary labels")
    ;; Configure before theme creation: tabs otherwise produce variable gaps.
    (setq treemacs-nerd-icons-tab " ")
    (require 'treemacs-nerd-icons)
    (treemacs-load-theme "nerd-icons")
    (require 'nerd-icons-dired)
    (add-hook 'dired-mode-hook #'nerd-icons-dired-mode)
    ;; Standard completion affixes work with Fido and Consult's mixed sources.
    (require 'nerd-icons-completion)
    (nerd-icons-completion-mode 1)
    (setq tab-line-tab-name-function #'neomacs-wasm-icons-tab-name)
    (tab-line-force-update t)))

(provide 'neomacs-wasm-icons)
;;; neomacs-wasm-icons.el ends here
