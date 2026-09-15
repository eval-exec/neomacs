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
(require 'org)
(require 'face-remap)

(defcustom neomacs-wasm-landing-personal-info
  "eval-exec\n\nBuilding NEO Emacs in the open.\n\nThis is a working preview. Feedback and contributions are welcome."
  "Introduction in the landing page's right sidebar; nil omits the sidebar."
  :type '(choice (const :tag "No personal sidebar" nil) string)
  :group 'neomacs-wasm)

(defface neomacs-wasm-landing-body
  '((t (:inherit variable-pitch :family "Ubuntu")))
  "Proportional type for the landing page prose." :group 'neomacs-wasm)
(defface neomacs-wasm-landing-heading
  '((t (:inherit font-lock-keyword-face :weight bold)))
  "Landing sidebar section heading." :group 'neomacs-wasm)
(defface neomacs-wasm-welcome-heading
  '((t (:inherit font-lock-keyword-face :family "Noto Serif" :weight normal)))
  "Contrasting proportional type for landing page headings." :group 'neomacs-wasm)

(define-derived-mode neomacs-wasm-landing-mode special-mode "NEO"
  "Read-only landing page; use TAB and RET or click an action."
  (setq-local truncate-lines nil
              truncate-partial-width-windows nil
              word-wrap t cursor-type nil
              header-line-format nil))

(define-derived-mode neomacs-wasm-welcome-mode org-mode "NEO Org"
  "An Org introduction to the live editor.  TAB folds its headings."
  (setq-local truncate-lines nil
              truncate-partial-width-windows nil
              word-wrap t
              org-hide-emphasis-markers t
              org-hide-leading-stars t
              org-fontify-whole-heading-line t
              header-line-format nil)
  (buffer-face-set 'neomacs-wasm-landing-body)
  (cl-loop for face in '(org-level-1 org-level-2 org-level-3 org-level-4
                        org-level-5 org-level-6 org-level-7 org-level-8)
           for height in '(1.8 1.4 1.15 1.05 1.05 1.05 1.05 1.05)
           do (face-remap-add-relative face (list :height height)
                                      'neomacs-wasm-welcome-heading))
  (dolist (face '(org-code org-verbatim))
    (face-remap-add-relative face 'neomacs-wasm-landing-body))
  (setq buffer-read-only t))

(defconst neomacs-wasm-landing--examples
  ";; NEO Emacs / your live Lisp playground
;;
;; Ready? Press C-x C-e now: hold Ctrl, press x,
;; then (still holding Ctrl) press e. Result: 3.
;; For each next example, put the cursor AFTER
;; its final closing parenthesis, then C-x C-e.

(+ 1 2)

;; 01 / Make the editor say hello
(message \"Hello from Lisp, inside your browser!\")

;; 02 / Change the words. Run it again.
(concat \"An editor is \" \"a place to think.\")

;; 03 / Little programs, immediate answers
(mapcar (lambda (n) (* n n)) '(1 2 3 4 5))
;; => (1 4 9 16 25)

;; 04 / Ask the editor about itself
(list :buffer (buffer-name)
      :mode major-mode
      :characters (buffer-size))

;; 05 / Change this buffer's appearance
(text-scale-set 1)
;; Back to the original size:
(text-scale-set 0)

;; 06 / Turn an idea into an editor command
;; Evaluate the WHOLE defun, then M-x neo-greet.
(defun neo-greet ()
  \"Say hello from the playground.\"
  (interactive)
  (message \"You just taught your editor a new command.\"))

;; 07 / Generate something you can keep
;; Creates another buffer; C-x b brings you back.
(with-current-buffer (get-buffer-create \"*NEO Notes*\")
  (goto-char (point-max))
  (insert \"One small expression. One new possibility.\\n\")
  (display-buffer (current-buffer)))

;; Keep exploring:
;; C-h f  describe a function     C-g  cancel
;; C-/    undo an edit            M-x write-file  save as
;; Saving preserves the text, not live Lisp state.
;; Unsaved buffers disappear when you reload.
"
  "Starter forms inserted once into a newly created playground.")

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
        (setq-local header-line-format nil)
        (insert neomacs-wasm-landing--examples)
        (goto-char (point-min))
        (search-forward "(+ 1 2)")
        (set-buffer-modified-p nil)
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
      (insert "* NEO Emacs\n"
              "/Not a screenshot. An editor you can change./\n\n"
              "Welcome to the browser editor.\n"
              "Rust underneath. Emacs Lisp at your fingertips.\n"
              "Running here, in your browser, through WebAssembly.\n\n"
              "** Your first little spark\n"
              "Select the playground on the right. If it is hidden,\n"
              "use the /Enter the playground/ action below.\n"
              "Then press =C-x C-e= to evaluate =(+ 1 2)=.\n"
              "The answer appears at the bottom of the editor.\n\n"
              "*** Make it your own\n"
              "Change a number. Evaluate again. That is the idea:\n"
              "a short conversation between you and your editor.\n\n")
      (neomacs-wasm-landing--action "  Enter the playground  →" #'neomacs-wasm-landing-playground)
      (insert "\n** This page is part of the editor\n"
              "You are reading an Org-mode buffer, not a web-page overlay.\n"
              "Put the cursor on a heading and press =TAB= to fold it.\n"
              "The playground is an Emacs Lisp buffer. Both are yours\n"
              "to explore with the same windows, commands, and keys.\n\n"
              "** Follow your curiosity\n"
              "Start with arithmetic. Make a message. Generate a list.\n"
              "Then define a command and run it with =M-x neo-greet=.\n"
              "The examples are small on purpose: change one thing,\n"
              "see what happens, and build from there.\n\n")
      (neomacs-wasm-landing--action "  Find a different mood / choose a theme  →" #'neomacs-wasm-landing-theme)
      (neomacs-wasm-landing--action "  Explore your browser files  →" #'dired)
      (neomacs-wasm-landing--action "  Make it personal / edit init.el  →" #'neomacs-wasm-landing-init)
      (insert "\n** A small map of the keyboard\n"
              "- =C-x C-e= :: Evaluate the expression before the cursor.\n"
              "- =C-x o= :: Move to another editor window.\n"
              "- =C-x b= :: Switch to another buffer.\n"
              "- =C-x 2= / =C-x 3= :: Split below / beside.\n"
              "- =M-x= :: Find and run a command (Alt+x).\n"
              "- =C-h f= :: Ask what a function does.\n"
              "- =C-g= :: Cancel. A good key to remember.\n\n"
              "Pause after a prefix: which-key offers the next keys.\n\n"
              "** Keep the good experiments\n"
              "Use =M-x write-file= to save under =/neomacs-fake/=.\n"
              "Saved files live in this site's browser storage.\n"
              "They are not files in your computer's home directory.\n"
              "Unsaved buffers and live Lisp definitions do not survive\n"
              "a reload. Clearing site data can remove saved files, too.\n\n"
              "** Built in the open. Still becoming.\n"
              "NEO Emacs explores a Rust implementation of Emacs\n"
              "with a graphical frontend and an Emacs Lisp heart.\n"
              "This browser edition is a working preview, not a promise\n"
              "that every desktop package already works here.\n\n"
              "Native subprocesses are unavailable; browser networking\n"
              "has browser restrictions. Expect unfinished edges.\n"
              "Found one? A small reproduction is a great contribution.\n"
              "Open the GitHub link in the title bar to join the project.\n\n"
              "/Read a little. Evaluate something. Make it yours./\n")
      (when (bound-and-true-p neomacs-wasm-package-error)
        (insert "\nOptional packages unavailable:\n"
                neomacs-wasm-package-error "\nReload the page to retry.\n")))
    (goto-char (point-min))
    (neomacs-wasm-welcome-mode)
    (org-show-all)
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
          (setq-local header-line-format nil)))
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
