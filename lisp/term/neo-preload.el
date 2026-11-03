;;; neo-preload.el --- dump-safe Neomacs backend defaults  -*- lexical-binding: t -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.

;; This file is part of GNU Emacs.

;;; Commentary:

;; Keep backend defaults that must also exist in batch and TTY sessions here.
;; The full `term/neo-win' layer remains a GUI-runtime concern and is therefore
;; intentionally not loaded into the portable dump.

;;; Code:

;; GNU's X-capable build preloads this binding from term/x-win.el.  Neomacs
;; recognizes the same power-management keysym, so its dumped global map must
;; expose the same command even before the GUI terminal layer is loaded.
(global-set-key [XF86WakeUp] #'ignore)

;; GNU's NS port binds Cmd-Q to `save-buffers-kill-emacs' in the dump
;; (`term/ns-win.el:141').  Neomacs's AppKit Quit item is retargeted to feed
;; that key event into the input transport, so the binding must exist in the
;; dump exactly where GNU installs it.  Other window systems leave `s-q'
;; unbound, as GNU does.
(when (eq system-type 'darwin)
  (global-set-key [?\s-q] #'save-buffers-kill-emacs))

;; `x-gtk-use-system-tooltips' has no C declaration in any GNU build: it is an
;; alias that term/x-win.el installs onto the `use-system-tooltips' DEFVAR_BOOL
;; in src/frame.c (term/x-win.el:1572, and term/pgtk-win.el:372 for pgtk), and
;; loadup.el preloads that file into the dump.  Aliasing rather than declaring
;; is what gives the name `use-system-tooltips' default of t, its docstring, and
;; the Boolean coercion the DEFVAR_BOOL slot performs -- a separate variable
;; here would have all three subtly wrong and would let the two names disagree.
(defvaralias 'x-gtk-use-system-tooltips 'use-system-tooltips)

;;; neo-preload.el ends here
