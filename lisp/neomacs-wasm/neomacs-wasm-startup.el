;;; neomacs-wasm-startup.el --- Browser startup defaults -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; Loaded by the evaluator worker before normal-top-level.  This is product
;; policy, not a replacement init-file loader.  GNU startup owns early-init.el,
;; init.el, error reporting, and startup hooks.  Never write the user's init.

;;; Code:

(add-to-list 'load-path (file-name-directory (or load-file-name buffer-file-name)))

(autoload 'neomacs-wasm-landing-open "neomacs-wasm-landing"
  "Open the welcome page and an editable Emacs Lisp playground." t)

(defgroup neomacs-wasm nil
  "NEO Emacs in the browser."
  :group 'environment)

(defcustom neomacs-wasm-startup-profile 'landing
  "Initial browser editor layout.
Set this in your personal init file.  `editor' preserves normal Emacs startup;
`landing' opens the welcome/playground layout unless your init already chose
a buffer or window layout."
  :type '(choice (const editor) (const landing))
  :group 'neomacs-wasm)

(defun neomacs-wasm-startup-finish ()
  "Apply the chosen profile once, after user initialization and frame setup."
  (remove-hook 'window-setup-hook #'neomacs-wasm-startup-finish)
  (when (and (eq neomacs-wasm-startup-profile 'landing)
             (not initial-buffer-choice)
             (one-window-p t)
             (equal (buffer-name (window-buffer)) "*scratch*"))
    (neomacs-wasm-landing-open)))

(defun neomacs-wasm-startup-initialize ()
  "Install browser defaults before personal initialization.
The worker calls this once per editor session, before `normal-top-level'."
  (setq ls-lisp-use-insert-directory-program nil)
  (require 'ls-lisp)
  (require 'url-neomacs-http)
  (url-neomacs-http-enable)
  (require 'neomacs-wasm-packages)
  (neomacs-wasm-packages-initialize)
  (add-hook 'window-setup-hook #'neomacs-wasm-startup-finish 90))

(provide 'neomacs-wasm-startup)
;;; neomacs-wasm-startup.el ends here
