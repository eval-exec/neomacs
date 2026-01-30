;;; neomacs-win.el --- parse relevant switches and set up for Neomacs  -*- lexical-binding: t -*-

;; Copyright (C) 2024-2026 Free Software Foundation, Inc.

;; Author: FSF
;; Keywords: terminals

;; This file is part of GNU Emacs.

;; GNU Emacs is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.

;; GNU Emacs is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.

;; You should have received a copy of the GNU General Public License
;; along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.

;;; Commentary:

;; Neomacs is a GPU-accelerated display backend using GTK4 and Rust.

;;; Code:

(eval-when-compile (require 'cl-lib))
(unless (featurep 'neomacs)
  (error "%s: Loading neomacs-win.el but not compiled with NEOMACS"
         invocation-name))

;; Documentation-purposes only: actually loaded in loadup.el.
(require 'term/common-win)
(require 'frame)
(require 'mouse)
(require 'scroll-bar)
(require 'faces)
(require 'menu-bar)
(require 'fontset)

(defvar x-invocation-args)
(defvar x-command-line-resources)

(defun neomacs-suspend-error ()
  "Don't allow suspending if any of the frames are Neomacs frames."
  (if (memq 'neomacs (mapcar 'window-system (frame-list)))
      (error "Cannot suspend Emacs while a Neomacs GUI frame exists")))

(defvar neomacs-initialized nil
  "Non-nil if Neomacs windowing has been initialized.")

(declare-function x-handle-args "common-win" (args))
(declare-function x-open-connection "neomacsfns.c"
                  (display &optional xrm-string must-succeed))

;; Define x-display-name if not set
(defvar x-display-name nil
  "The display name specifying the display to connect to.")

;; Do the actual window system setup here.
(cl-defmethod window-system-initialization (&context (window-system neomacs)
                                            &optional display)
  "Initialize the Neomacs window system.
WINDOW-SYSTEM is, aptly, `neomacs'.
DISPLAY is the name of the display Emacs should connect to."
  (cl-assert (not neomacs-initialized))

  ;; Handle command line args
  (setq command-line-args (x-handle-args command-line-args))

  ;; Make sure we have a valid resource name.
  (when (boundp 'x-resource-name)
    (unless (stringp x-resource-name)
      (let (i)
	(setq x-resource-name (copy-sequence invocation-name))
	;; Change any . or * characters in x-resource-name to hyphens.
	(while (setq i (string-match "[.*]" x-resource-name))
	  (aset x-resource-name i ?-)))))

  ;; Setup the default fontset.
  (create-default-fontset)
  ;; Create the standard fontset.
  (condition-case err
      (create-fontset-from-fontset-spec standard-fontset-spec t)
    (error (display-warning
            'initialization
            (format "Creation of the standard fontset failed: %s" err)
            :error)))

  ;; Open the display connection
  (x-open-connection (or display
                         x-display-name)
		     x-command-line-resources
		     ;; Exit Emacs with fatal error if this fails.
		     t)

  ;; Create the default faces
  (let ((color-map '((foreground-color . "black")
                     (background-color . "white")
                     (cursor-color . "black"))))
    (dolist (param color-map)
      (add-to-list 'default-frame-alist param)))

  (add-hook 'suspend-hook #'neomacs-suspend-error)

  (setq neomacs-initialized t))

;; Handle args function (required by common-win)
(cl-defmethod handle-args-function (args &context (window-system neomacs))
  (x-handle-args args))

;; Frame creation for Neomacs
(cl-defmethod frame-creation-function (params &context (window-system neomacs))
  (x-create-frame params))

;; Provide the feature
(provide 'neomacs-win)
(provide 'term/neomacs-win)

;;; neomacs-win.el ends here
