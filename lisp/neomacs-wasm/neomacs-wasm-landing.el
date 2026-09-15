;;; neomacs-wasm-landing.el --- The NEO Emacs landing page -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; The landing page is the editor: ordinary buffers, buttons, faces, and
;; windows.  No DOM imitation or resize hook takes over the user's layout.

;;; Code:

(require 'neomacs-wasm-startup)
(require 'button)
(require 'seq)

(defcustom neomacs-wasm-landing-personal-info
  "eval-exec\n\nBuilding NEO Emacs in the open.\n\nThis is a working preview. Feedback and contributions are welcome."
  "Introduction in the landing page's right sidebar; nil omits the sidebar."
  :type '(choice (const :tag "No personal sidebar" nil) string)
  :group 'neomacs-wasm)

(defface neomacs-wasm-landing-title
  '((t (:inherit font-lock-function-name-face :weight bold :height 1.6)))
  "Landing page title." :group 'neomacs-wasm)
(defface neomacs-wasm-landing-heading
  '((t (:inherit font-lock-keyword-face :weight bold)))
  "Landing page section heading." :group 'neomacs-wasm)

(define-derived-mode neomacs-wasm-landing-mode special-mode "NEO"
  "Read-only landing page; use TAB and RET or click an action."
  (setq-local truncate-lines nil
              truncate-partial-width-windows nil
              word-wrap t cursor-type nil))

(defun neomacs-wasm-landing--header (label hint)
  "Give the current pane a role LABEL and a useful HINT."
  (setq-local header-line-format
              (list (propertize (concat "  " label "  ")
                                'face 'neomacs-wasm-landing-heading)
                    (propertize (concat " / " hint) 'face 'shadow))))

(defun neomacs-wasm-landing--heading (text)
  (insert (propertize text 'face 'neomacs-wasm-landing-heading) "\n\n"))

(defun neomacs-wasm-landing--action (label command)
  (insert-text-button label 'follow-link t
                      'action (lambda (_) (call-interactively command)))
  (insert "\n"))

(defun neomacs-wasm-landing-playground ()
  "Select the playground without erasing existing work."
  (interactive)
  (pop-to-buffer (neomacs-wasm-landing--playground-buffer)))

(defun neomacs-wasm-landing--playground-buffer ()
  "Return the editable playground, initializing it only once."
  (or (get-buffer "*NEO Emacs Playground*")
      (with-current-buffer (get-buffer-create "*NEO Emacs Playground*")
        (emacs-lisp-mode)
        (neomacs-wasm-landing--header "PLAYGROUND" "Emacs Lisp | C-x C-e to evaluate")
        (current-buffer))))

(defun neomacs-wasm-landing-init ()
  "Visit personal configuration in the browser's persistent home."
  (interactive)
  (find-file (expand-file-name "init.el" user-emacs-directory)))

(defun neomacs-wasm-landing-theme (theme)
  "Select installed THEME, replacing previously enabled themes."
  (interactive
   (list (intern (completing-read "Theme: " (custom-available-themes) nil t))))
  (let ((previous custom-enabled-themes))
    (load-theme theme t)
    (dolist (old previous) (unless (eq old theme) (disable-theme old)))))

(defun neomacs-wasm-landing--welcome ()
  (with-current-buffer (get-buffer-create "*NEO Emacs*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "\n" (propertize "NEO Emacs" 'face 'neomacs-wasm-landing-title) "\n\n")
      (insert (propertize "YOUR EDITOR. IN YOUR BROWSER." 'face 'shadow) "\n\n")
      (insert (propertize "  WEBASSEMBLY  /  LIVE LISP  /  YOUR RULES  "
                          'face 'neomacs-wasm-landing-heading) "\n\n")
      (insert "Welcome to the browser editor.\n"
              "A living Lisp environment. Explore it,\n"
              "change it, and make it yours.\n\n")
      (neomacs-wasm-landing--heading "START EXPLORING")
      (neomacs-wasm-landing--action "  Open the Lisp playground  →" #'neomacs-wasm-landing-playground)
      (neomacs-wasm-landing--action "  Browse your files         →" #'dired)
      (neomacs-wasm-landing--action "  Choose a theme            →" #'neomacs-wasm-landing-theme)
      (neomacs-wasm-landing--action "  Edit your init.el         →" #'neomacs-wasm-landing-init)
      (insert "\n")
      (neomacs-wasm-landing--heading "A FEW KEYS TO GET STARTED")
      (insert "M-x         Run any editor command\n"
              "C-x b       Switch buffers\n"
              "C-x C-f     Open or create a file\n"
              "C-x C-e     Evaluate Lisp before point\n"
              "C-g         Cancel the current command\n\n"
              "Pause after a key prefix: which-key helps.\n\n")
      (neomacs-wasm-landing--heading "YOURS TO EXPERIMENT WITH")
      (insert "Type an expression in the empty playground.\n"
              "Try (+ 1 2), then C-x C-e.\n\n"
              "Your home is /neomacs-fake. Saved files\n"
              "stay in this browser's storage for this site.\n"
              "Unsaved buffers do not survive a reload.\n")
      (when (bound-and-true-p neomacs-wasm-package-error)
        (insert "\nOptional packages unavailable:\n"
                neomacs-wasm-package-error "\nReload the page to retry.\n")))
    (goto-char (point-min))
    (neomacs-wasm-landing-mode)
    (neomacs-wasm-landing--header "WELCOME" "Explore | Experiment | Make it yours")
    (set-buffer-modified-p nil)
    (current-buffer)))

(defun neomacs-wasm-landing--about ()
  (when neomacs-wasm-landing-personal-info
    (with-current-buffer (get-buffer-create "*NEO Emacs About*")
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert "\n")
        (neomacs-wasm-landing--heading "FROM THE AUTHOR")
        (insert neomacs-wasm-landing-personal-info "\n\n")
        (neomacs-wasm-landing--heading "THE PROJECT")
        (insert "github.com/\neval-exec/neomacs\n\n"
                "The project link in the title bar opens\n"
                "GitHub in a new browser tab.\n\n")
        (neomacs-wasm-landing--heading "WORK IN PROGRESS")
        (insert "Desktop-class ideas,\nbrowser-sized possibilities.\n\n"
                "This preview does not provide native\n"
                "subprocesses or unrestricted networking.\n"))
      (goto-char (point-min))
      (neomacs-wasm-landing-mode)
      (neomacs-wasm-landing--header "ABOUT" "Built in the open")
      (set-buffer-modified-p nil)
      (current-buffer))))

(defun neomacs-wasm-landing--tree ()
  "Open a browser-home tree without making optional failure abort the page."
  (condition-case error-data
      (save-selected-window
        (unless (treemacs-workspace->projects (treemacs-current-workspace))
          (let ((result (treemacs-do-add-project-to-workspace
                         (expand-file-name "~") "Your files")))
            (unless (eq (car result) 'success)
              (error "Cannot create browser workspace: %S" result))))
        (unless (treemacs-get-local-window) (treemacs))
        (with-current-buffer (window-buffer (treemacs-get-local-window))
          (neomacs-wasm-landing--header "FILES" "Browser home")))
    (error
     (setq neomacs-wasm-package-error (error-message-string error-data))
     (neomacs-wasm-landing--welcome))))

(defun neomacs-wasm-landing-open ()
  "Open the landing page, Treemacs, playground, and personal sidebar.
Medium frames omit the personal sidebar; narrow frames show welcome only.
All buffers remain accessible with C-x b.  Reopening preserves playground
edits.  Only this command or initial startup arranges the windows."
  (interactive)
  (let ((welcome (neomacs-wasm-landing--welcome))
        (info (neomacs-wasm-landing--about)))
    (neomacs-wasm-landing--playground-buffer)
    (when (bound-and-true-p tab-bar-mode)
      (tab-bar-rename-tab "NEO / Playground"))
    ;; Select a non-side leaf before deleting the old window layout.
    (select-window
     (or (seq-find (lambda (window) (not (window-parameter window 'window-side)))
                   (window-list))
         (selected-window)))
    ;; Treemacs protects its side window from `delete-other-windows'.
    ;; Explicitly close old side windows so repeated layout has the same width.
    (dolist (window (window-list))
      (when (window-parameter window 'window-side) (delete-window window)))
    (delete-other-windows)
    (switch-to-buffer welcome)
    (let ((width (window-total-width)))
      (when (and (>= width 130) (featurep 'treemacs))
        (neomacs-wasm-landing--tree))
      (when (and info (>= width 170))
        (display-buffer-in-side-window info '((side . right) (slot . 0) (window-width . 26))))
      (when (>= (window-total-width) 85)
        (let ((right (split-window-right)))
          (set-window-buffer right (get-buffer "*NEO Emacs Playground*"))
          (select-window right))))))

(provide 'neomacs-wasm-landing)
;;; neomacs-wasm-landing.el ends here
