;;; neomacs-wasm-packages.el --- Browser package defaults -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; Configure available packages before the user's init.  Downloading and
;; verifying third-party assets belongs to the browser startup asset pipeline,
;; not to synchronous Lisp initialization.  which-key ships with Emacs itself.

;;; Code:

(defun neomacs-wasm-packages-initialize ()
  "Enable the bundled key-binding helper; personal init can override it."
  (require 'which-key)
  (which-key-mode 1))

(provide 'neomacs-wasm-packages)
;;; neomacs-wasm-packages.el ends here
