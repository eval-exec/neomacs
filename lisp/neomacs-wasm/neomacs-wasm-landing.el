;;; neomacs-wasm-landing.el --- Browser welcome and playground -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; An optional, one-time startup layout using ordinary Emacs windows and
;; buffers.  No resize hook owns the user's layout after startup.  Third-party
;; sidebars can be integrated when their browser asset bundle is available.

;;; Code:

(require 'neomacs-wasm-startup)

(defcustom neomacs-wasm-landing-personal-info nil
  "Personal introduction for the landing page's right sidebar.
Nil omits the sidebar.  Narrow frames omit it too; its buffer remains
available through `switch-to-buffer' after opening a landing page."
  :type '(choice (const :tag "No personal sidebar" nil) string)
  :group 'neomacs-wasm)

(defun neomacs-wasm-landing--text-buffer (name text)
  "Return a read-only buffer NAME containing TEXT."
  (with-current-buffer (get-buffer-create name)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert text)
      (goto-char (point-min)))
    (special-mode)
    (setq-local truncate-lines nil
                word-wrap t)
    (set-buffer-modified-p nil)
    (current-buffer)))

(defun neomacs-wasm-landing-open ()
  "Open the welcome page and an editable Emacs Lisp playground.
Wide windows show both side by side.  Narrow windows show the welcome page;
the playground is available using `switch-to-buffer'.  Reopening never erases
playground edits.  Explicit invocation replaces the current window layout."
  (interactive)
  (let* ((welcome (neomacs-wasm-landing--text-buffer
                   "*NEO Emacs*"
                   (concat "NEO Emacs — Working in progress\n\n"
                           "https://github.com/eval-exec/neomacs\n\n"
                           "Welcome to the browser editor.\n\n"
                           "M-x                 Run a command\n"
                           "C-x b               Switch buffers\n"
                           "C-x C-f             Open a file\n"
                           "M-x load-theme      Choose an installed theme\n\n"
                           "Try Emacs Lisp in *NEO Emacs Playground*.\n"
                           "C-j evaluates the expression before point.\n\n"
                           "Personal configuration: ~/.emacs.d/init.el\n"
                           "Files in your browser home persist on this origin.\n")))
         (playground (or (get-buffer "*NEO Emacs Playground*")
                         (with-current-buffer (get-buffer-create "*NEO Emacs Playground*")
                           (emacs-lisp-mode)
                           (current-buffer))))
         (info (when neomacs-wasm-landing-personal-info
                 (neomacs-wasm-landing--text-buffer
                  "*NEO Emacs About*" neomacs-wasm-landing-personal-info))))
    (delete-other-windows)
    (switch-to-buffer welcome)
    (when (and info (>= (window-total-width) 160))
      (display-buffer-in-side-window
       info '((side . right) (slot . 0) (window-width . 28))))
    (when (>= (window-total-width) 100)
      (let ((right (split-window-right)))
        (set-window-buffer right playground)
        (select-window right)))))

(provide 'neomacs-wasm-landing)
;;; neomacs-wasm-landing.el ends here
