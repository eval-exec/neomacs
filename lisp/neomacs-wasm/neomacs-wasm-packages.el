;;; neomacs-wasm-packages.el --- Browser package defaults -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; Configure available packages before the user's init.  Downloading and
;; verifying third-party assets belongs to the browser startup asset pipeline,
;; not to synchronous Lisp initialization.  which-key ships with Emacs itself.

;;; Code:

(defvar neomacs-wasm-package-error nil
  "Reason optional landing packages are unavailable, or nil.")

(defconst neomacs-wasm-packages--root
  (expand-file-name "../neomacs-wasm-packages/"
                    (file-name-directory (or load-file-name buffer-file-name))))

(defun neomacs-wasm-packages--activate ()
  "Activate the verified, read-only package bundle mounted by the worker."
  (unless (file-directory-p neomacs-wasm-packages--root)
    (error "Package download unavailable; reload the page to retry"))
  (dolist (directory '("dash" "s" "ht" "pfuture" "avy" "ace-window"
                       "hydra" "posframe" "cfrs" "treemacs/src/elisp"
                       "doom-themes" "doom-themes/themes" "compat" "cond-let" "keycast"
                       "nerd-icons" "nerd-icons/data" "treemacs-nerd-icons" "nerd-icons-dired"
                       "org-modern" "doom-modeline" "shrink-path" "f" "consult"))
    (add-to-list 'load-path (expand-file-name directory neomacs-wasm-packages--root)))
  (add-to-list 'custom-theme-load-path
               (expand-file-name "doom-themes/themes" neomacs-wasm-packages--root))
  ;; No package-manager generated files are installed in the user's directory.
  (dolist (feature '(dash s ht pfuture avy ace-window lv hydra posframe cfrs))
    (require feature))
  (setq treemacs-no-png-images nil
        treemacs-python-executable nil
        treemacs-collapse-dirs 0
        treemacs-read-string-input 'from-minibuffer
        treemacs-persist-file nil
        treemacs-width 24
        treemacs-text-scale -1.5)
  (require 'treemacs)
  ;; The curated source bundle has no package-generated autoload file.
  ;; treemacs-mode binds mouse commands without requiring their definitions.
  (require 'treemacs-mouse-interface)
  (require 'neomacs-wasm-icons)
  (neomacs-wasm-icons-initialize)
  (treemacs-git-mode -1)
  (treemacs-filewatch-mode -1)
  (setq treemacs-collapse-dirs 0)
  (require 'doom-themes)
  ;; The landing page has a dark visual identity; personal init runs afterwards.
  (load-theme 'doom-one t)
  ;; Browser sessions have no external language-version subprocesses.
  (setq doom-modeline-env-version nil)
  (require 'doom-modeline)
  (doom-modeline-mode 1)
  ;; Expose Consult commands without overriding Fido or personal key bindings.
  (require 'consult)
  (require 'org-modern)
  ;; These folding markers are covered by the browser's packaged text fonts.
  (setq org-modern-fold-stars '(("▶" . "▼") ("▷" . "▽")))
  (add-hook 'org-mode-hook #'org-modern-mode)
  (add-hook 'org-agenda-finalize-hook #'org-modern-agenda)
  (require 'keycast)
  (keycast-tab-bar-mode 1))

(defun neomacs-wasm-packages-initialize ()
  "Install package defaults before personal init, tolerating optional failure."
  (remove-hook 'before-init-hook #'neomacs-wasm-packages-initialize)
  (require 'which-key)
  (which-key-mode 1)
  (require 'icomplete)
  (fido-vertical-mode 1)
  (require 'tab-bar)
  (require 'tab-line)
  (tab-bar-mode 1)
  (global-tab-line-mode 1)
  (condition-case error-data
      (neomacs-wasm-packages--activate)
    (error (setq neomacs-wasm-package-error (error-message-string error-data)))))

(provide 'neomacs-wasm-packages)
;;; neomacs-wasm-packages.el ends here
