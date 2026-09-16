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
(require 'image)
(require 'browse-url)

(defvar-local neomacs-wasm-landing--banner-data nil)
(defvar-local neomacs-wasm-landing--banner-image nil)
(defvar-local neomacs-wasm-landing--banner-overlay nil)
(defvar-local neomacs-wasm-landing--playground-positioned nil)

(defcustom neomacs-wasm-landing-playground-file "~/playground.el"
  "Writable playground file.  Existing contents are never replaced."
  :type 'file
  :group 'neomacs-wasm)

(defun neomacs-wasm-landing--resize-banner (&optional frame)
  "Fit the inline banner to the smallest visible landing pane on FRAME.
Only image geometry changes; never rearrange the user's windows."
  (when-let* ((buffer (get-buffer "*NEO Emacs*"))
              (windows (get-buffer-window-list buffer nil (or frame t))))
    (with-current-buffer buffer
      (when (and neomacs-wasm-landing--banner-data (display-images-p))
        (let ((width (max 1 (min 800 (- (apply #'min
                                      (mapcar (lambda (window) (window-body-width window t))
                                              windows)) 16)))))
          (unless (and (equal width (plist-get (cdr neomacs-wasm-landing--banner-image) :width))
                       (overlayp neomacs-wasm-landing--banner-overlay)
                       (overlay-buffer neomacs-wasm-landing--banner-overlay))
            (condition-case error-data
                (let ((image (create-image neomacs-wasm-landing--banner-data
                                           'svg t :width width :scale 1 :ascent 'center))
                      (inhibit-read-only t))
                  ;; Explicit loading is outside redisplay. Report a real
                  ;; decoder failure instead of leaving an invisible banner.
                  (image-size image t)
                  (when neomacs-wasm-landing--banner-image
                    (image-flush neomacs-wasm-landing--banner-image))
                  (setq neomacs-wasm-landing--banner-image image)
                  (save-excursion
                    (goto-char (point-min))
                    (when (looking-at "\\[\\[file:[^\n]+\\]\\]")
                      ;; Font-lock (including Org Modern) owns display text
                      ;; properties. Keep the image in its own overlay so
                      ;; refontification cannot erase it.
                      (unless (overlayp neomacs-wasm-landing--banner-overlay)
                        (setq neomacs-wasm-landing--banner-overlay
                              (make-overlay (match-beginning 0) (match-end 0))))
                      (move-overlay neomacs-wasm-landing--banner-overlay
                                    (match-beginning 0) (match-end 0))
                      (overlay-put neomacs-wasm-landing--banner-overlay 'display image)
                      (overlay-put neomacs-wasm-landing--banner-overlay 'evaporate t)))
                  (set-buffer-modified-p nil))
              (error (message "Landing banner: %s" (error-message-string error-data))))))))))

(defcustom neomacs-wasm-landing-personal-info
  "Building NEO Emacs in the open.\n\nThis is a working preview. Feedback and contributions are welcome."
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
              header-line-format nil)
  (buffer-face-set 'neomacs-wasm-landing-body))

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

(defun neomacs-wasm-landing--heading (text)
  (insert (propertize text 'face 'neomacs-wasm-landing-heading) "\n\n"))

(defun neomacs-wasm-landing--root ()
  "Return the packaged site's directory."
  (expand-file-name "neomacs-landing/" data-directory))

(defun neomacs-wasm-landing-copy ()
  "Copy the current site document into browser home without overwriting files."
  (interactive)
  (unless (and buffer-file-name
               (file-in-directory-p buffer-file-name (neomacs-wasm-landing--root)))
    (user-error "This is not a packaged site document"))
  (let ((destination (read-file-name "Copy to Your files: "
                                    (expand-file-name "~/")
                                    nil nil (file-name-nondirectory buffer-file-name))))
    (copy-file buffer-file-name destination nil)
    (find-file destination)))

(defun neomacs-wasm-landing--follow (action _argument)
  "Follow a named site ACTION, never arbitrary Lisp."
  (pcase action
    ("playground" (neomacs-wasm-landing-playground))
    ("theme" (call-interactively #'neomacs-wasm-landing-theme))
    ("files" (dired (expand-file-name "~/")))
    ("init" (neomacs-wasm-landing-init))
    ("copy" (call-interactively #'neomacs-wasm-landing-copy))
    (_ (user-error "Unknown NEO Emacs action: %s" action))))

(org-link-set-parameters "neo" :follow #'neomacs-wasm-landing--follow)

(defun neomacs-wasm-landing--visit ()
  "Style packaged Org documents regardless of how they were opened."
  (when (and buffer-file-name
             (file-in-directory-p buffer-file-name (neomacs-wasm-landing--root)))
    (when (derived-mode-p 'org-mode)
      (neomacs-wasm-welcome-mode))
    (setq buffer-read-only t)))

(add-hook 'find-file-hook #'neomacs-wasm-landing--visit)

(defun neomacs-wasm-landing-playground ()
  "Select the playground without erasing existing work."
  (interactive)
  (pop-to-buffer (neomacs-wasm-landing--playground-buffer)))

(defun neomacs-wasm-landing--playground-buffer ()
  "Visit the persistent playground, seeding its file only when absent."
  (let ((file (expand-file-name neomacs-wasm-landing-playground-file)))
    (unless (file-exists-p file)
      (copy-file (expand-file-name "playground.el" (neomacs-wasm-landing--root))
                 file nil))
    (with-current-buffer (find-file-noselect file)
      (rename-buffer "*Playgorund*" t)
      (setq-local header-line-format nil)
      (display-line-numbers-mode 1)
      (unless (bound-and-true-p neomacs-wasm-landing--playground-positioned)
        (setq-local neomacs-wasm-landing--playground-positioned t)
        (goto-char (point-min))
        (search-forward "(+ 1 2)" nil t))
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
  "Visit the packaged home page without generating or replacing its text."
  (with-current-buffer
      (find-file-noselect (expand-file-name "index.org" (neomacs-wasm-landing--root)))
    (rename-buffer "*NEO Emacs*" t)
    (unless neomacs-wasm-landing--banner-data
      (let ((banner (expand-file-name "../images/neomacs-banner.svg"
                                      (neomacs-wasm-landing--root))))
        (when (and (display-images-p) (file-readable-p banner))
          (setq neomacs-wasm-landing--banner-data
                (with-temp-buffer
                  (set-buffer-multibyte nil)
                  (insert-file-contents-literally banner)
                  (buffer-string))))))
    (org-show-all)
    (current-buffer)))

(defun neomacs-wasm-landing--about ()
  (when neomacs-wasm-landing-personal-info
    (with-current-buffer (get-buffer-create "*About*")
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert "\n")
        (neomacs-wasm-landing--heading "FROM THE AUTHOR")
        (when (display-images-p)
          (condition-case error-data
              (let ((image (create-image
                            (expand-file-name "neomacs-landing/author.jpg" data-directory)
                            'jpeg nil :width 112 :scale 1 :ascent 'center)))
                ;; Decode before redisplay, just as for the welcome banner.
                (image-size image t)
                (insert-image image "[Eval Exec's GitHub avatar]")
                (insert "\n\n"))
            (error (message "Author picture: %s" (error-message-string error-data)))))
        (insert-text-button "Eval Exec"
                            'follow-link t
                            'help-echo "Open GitHub profile in a browser tab"
                            'action (lambda (_button)
                                      (browse-url "https://github.com/eval-exec")))
        (insert "\n@eval-exec\n\n")
        (insert neomacs-wasm-landing-personal-info "\n\n")
        (neomacs-wasm-landing--heading "THE PROJECT")
        (insert "https://github.com/eval-exec/neomacs\n\n"
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
        (dolist (entry (list (cons (neomacs-wasm-landing--root) "NEO Emacs")
                            (cons (expand-file-name "~/") "Your files")))
          (unless (seq-some
                   (lambda (project)
                     (equal (directory-file-name (treemacs-project->path project))
                            (directory-file-name (car entry))))
                   (treemacs-workspace->projects (treemacs-current-workspace)))
            (let ((result (treemacs-do-add-project-to-workspace (car entry) (cdr entry))))
              (unless (eq (car result) 'success)
                (error "Cannot create browser workspace: %S" result)))))
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
          (set-window-buffer right (get-buffer "*Playgorund*"))
          (select-window right)))))
  (add-hook 'window-size-change-functions #'neomacs-wasm-landing--resize-banner)
  (neomacs-wasm-landing--resize-banner)
  (when (bound-and-true-p neomacs-wasm-package-error)
    (message "Optional landing packages unavailable: %s. Reload to retry."
             neomacs-wasm-package-error)))

(provide 'neomacs-wasm-landing)
;;; neomacs-wasm-landing.el ends here
