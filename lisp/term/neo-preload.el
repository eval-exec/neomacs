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
;;
;; It is an X-window-system name: loadup.el loads term/x-win.el only under
;; `(featurep 'x)' (`lisp/loadup.el:304-309') and the NS dump loads
;; term/ns-win.el instead (`:347-356'), so GNU's NS build leaves the alias
;; unbound and `(boundp 'x-gtk-use-system-tooltips)' is nil there.
;; `system-type' is the honest dump-time proxy: Neomacs answers `(featurep 'x)'
;; and `(featurep 'ns)' nil on every host (c_features.rs), so feature-gating
;; would delete the alias from the X11 dump as well.
(unless (eq system-type 'darwin)
  (defvaralias 'x-gtk-use-system-tooltips 'use-system-tooltips))

;; GNU binds `x-file-dialog' in its toolkit builds and, where the C layer has
;; no dialog, from Lisp: the X DEFUN is registered only under USE_MOTIF ||
;; USE_GTK (xfns.c:10632-10634; the pinned reference is a GTK build),
;; w32fns.c defines it for W32 (8252), and the NS and Haiku builds bind it
;; from Lisp (`term/ns-win.el:464' over ns-read-file-name,
;; `term/haiku-win.el:329' over haiku-read-file-name; the latter ends
;; `(error "x-file-dialog on a tty frame")').  Every such build therefore
;; answers `fboundp' t in the dump, batch sessions included.
;; `minibuffer.el:4051' calls it only when `next-read-file-uses-dialog-p'
;; (guarded at :3886) says a dialog is wanted, and GNU's C predicate answers
;; t on a toolkit GUI frame (fileio.c:6556-6572, via use-dialog-box and
;; use-file-dialog).  Neomacs's backend has no native file-panel primitive, so
;; this is GNU's own Lisp arm for that case: the same name, signature and
;; return value, with minibuffer completion performing the prompt.  It lives
;; here, the dump-safe preload (loadup.el:267), not in the GUI-runtime-only
;; term/neo-win.el, so batch and TTY sessions expose the same surface GNU's
;; dump does.  Because the caller may itself have been reached from
;; `read-file-name', the inner call binds `use-file-dialog' to nil: once
;; `next-read-file-uses-dialog-p' follows GNU on GUI frames, the chooser must
;; not re-enter `x-file-dialog' through `read-file-name-default'.
(defun x-file-dialog (prompt dir &optional default-filename
                             mustmatch only-dir-p)
  "Read a file name; the Neomacs backend has no native file panel.
PROMPT, DIR, DEFAULT-FILENAME, MUSTMATCH and ONLY-DIR-P have the same
meaning as in GNU's `x-file-dialog'; minibuffer completion stands in for
the toolkit dialog, with `use-file-dialog' bound to nil around it so the
call cannot re-enter this function.  On a non-graphical frame (batch or
-nw) this signals the same error GNU's backend for this surface signals."
  (unless (display-graphic-p)
    ;; GNU's C bodies check the window system before any argument work, but
    ;; the message is backend-specific: the X DEFUN checks the selected frame
    ;; (src/frame.c:134-141, xfns.c:9688) and says "Window system frame should
    ;; be used"; the NS arm ends in ns-read-file-name's
    ;; check_window_system(NULL) (nsfns.m:1784) and says "Window system is not
    ;; in use or not initialized".  Neo's surface gives the matching answer.
    (error "%s"
           (if (eq system-type 'darwin)
               "Window system is not in use or not initialized"
             "Window system frame should be used")))
  (let ((use-file-dialog nil))
    (read-file-name prompt dir default-filename mustmatch nil
                    (and only-dir-p #'file-directory-p))))

;;; neo-preload.el ends here
