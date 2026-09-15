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
                       "doom-themes" "doom-themes/themes"))
    (add-to-list 'load-path (expand-file-name directory neomacs-wasm-packages--root)))
  (add-to-list 'custom-theme-load-path
               (expand-file-name "doom-themes/themes" neomacs-wasm-packages--root))
  ;; No package-manager generated files are installed in the user's directory.
  (dolist (feature '(dash s ht pfuture avy ace-window lv hydra posframe cfrs))
    (require feature))
  (setq treemacs-no-png-images t
        treemacs-python-executable nil
        treemacs-collapse-dirs 0
        treemacs-read-string-input 'from-minibuffer
        treemacs-persist-file nil
        treemacs-width 24)
  (require 'treemacs)
  (treemacs-git-mode -1)
  (treemacs-filewatch-mode -1)
  (setq treemacs-collapse-dirs 0)
  (require 'doom-themes)
  ;; Match the browser's opening appearance; personal init runs afterwards.
  (load-theme (if (eq (frame-parameter nil 'background-mode) 'dark)
                  'doom-one 'doom-one-light) t))

(defun neomacs-wasm-packages-initialize ()
  "Install package defaults before personal init, tolerating optional failure."
  (require 'which-key)
  (which-key-mode 1)
  (condition-case error-data
      (neomacs-wasm-packages--activate)
    (error (setq neomacs-wasm-package-error (error-message-string error-data)))))

(provide 'neomacs-wasm-packages)
;;; neomacs-wasm-packages.el ends here
