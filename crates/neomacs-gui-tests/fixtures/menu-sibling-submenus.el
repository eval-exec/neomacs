;;; menu-sibling-submenus.el --- Two submenus in one popup -*- lexical-binding: t -*-

;; Two sibling rows each open their own native submenu. Switching the pointer
;; from one row to the other replaces the open submenu, which is the sequence
;; in #567.

(require 'json)
(require 'cl-lib)
(let ((root (make-sparse-keymap "Siblings"))
      (first (make-sparse-keymap "First"))
      (second (make-sparse-keymap "Second")))
  (define-key first [noop] '(menu-item "First action" ignore))
  (define-key second [noop] '(menu-item "Second action" ignore))
  ;; Define in reverse display order: First Submenu, then Second Submenu.
  (define-key root [second] (list 'menu-item "Second Submenu" second))
  (define-key root [first] (list 'menu-item "First Submenu" first))
  (define-key global-map [menu-bar file] (list 'menu-item "File" root)))

(defvar neomacs-menu-directory (getenv "NEOMACS_GUI_MENU_CONTROL"))
(defvar neomacs-menu-sample 0)
(menu-bar-mode 1)
(modify-frame-parameters nil '((title . "NEOMACS-MENU-REPRO")))

(defun neomacs-menu-observe ()
  (let ((state `((pid . ,(emacs-pid))
                 (sample . ,(cl-incf neomacs-menu-sample))
                 (char-width . ,(frame-char-width))
                 (char-height . ,(frame-char-height)))))
    (with-temp-file (expand-file-name "state.pending" neomacs-menu-directory)
      (insert (json-encode state)))
    (rename-file (expand-file-name "state.pending" neomacs-menu-directory)
                 (expand-file-name "state.json" neomacs-menu-directory) t))
  (when (file-exists-p (expand-file-name "stop" neomacs-menu-directory))
    (kill-emacs 0)))

(run-at-time 0.2 0.05 #'neomacs-menu-observe)
