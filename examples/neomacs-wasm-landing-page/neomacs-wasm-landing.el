;;; neomacs-wasm-landing.el --- The NEO Emacs landing page -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.
;; SPDX-License-Identifier: GPL-3.0-or-later

;;; Commentary:

;; The landing page is the editor: ordinary buffers, buttons, faces, and
;; windows.  Resize events restore its panes while the landing layout is
;; intact; user changes leave window management under the user's control.

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

(defconst neomacs-wasm-landing--repo-url "https://github.com/eval-exec/neomacs"
  "The repository this experimental preview is developed in.")

(defvar neomacs-wasm-landing--icon-image nil
  "Cached window-icon image for the tab bar, or nil when unavailable.")

(defun neomacs-wasm-landing--tab-bar-icon ()
  "Return a display-spec string with the window icon, or an empty string."
  (if (not (display-images-p))
      ""
    (unless neomacs-wasm-landing--icon-image
      (condition-case nil
          (let ((image (create-image
                        (expand-file-name "images/neomacs-window-icon.svg"
                                          data-directory)
                        'svg nil :height 20 :ascent 'center)))
            ;; Decode outside redisplay so a failure is visible here, not
            ;; as a silently blank spot in the tab bar.
            (image-size image t)
            (setq neomacs-wasm-landing--icon-image image))
        (error (setq neomacs-wasm-landing--icon-image 'unavailable))))
    (if (eq neomacs-wasm-landing--icon-image 'unavailable)
        ""
      (propertize " " 'display neomacs-wasm-landing--icon-image))))

(defun neomacs-wasm-landing--tab-bar-icon-item ()
  "Produce the window icon alone at the far left of the tab bar."
  `((neomacs-wasm-icon
     menu-item
     ,(concat (neomacs-wasm-landing--tab-bar-icon) " ")
     neomacs-wasm-landing-browse-repository
     :help "Open https://github.com/eval-exec/neomacs in a browser tab")))

(defun neomacs-wasm-landing--tab-bar-branding ()
  "Produce the product name at the left of the tab bar."
  `((neomacs-wasm-branding
     menu-item
     ,(concat
       (propertize " NEO Emacs (WebAssembly build) "
                   'face '(:inherit (bold neomacs-wasm-landing-body)
                           :weight bold :foreground "white"))
       (propertize "  │  " 'face 'shadow))
     neomacs-wasm-landing-browse-repository
     :help "Open https://github.com/eval-exec/neomacs in a browser tab")))

(defun neomacs-wasm-landing--tab-bar-warning ()
  "Produce the conspicuous work-in-progress warning in the tab bar.
A yellow badge with a warning sign; clickable, and the tooltip carries
the full URL so the bar does not have to."
  `((neomacs-wasm-warning
     menu-item
     ,(concat
       (propertize
        " ▲ EXPERIMENTAL · INCOMPLETE · WORK IN PROGRESS "
        'face '(:inherit (font-lock-warning-face
                          neomacs-wasm-landing-body)
                         :weight bold :background "yellow"
                         :foreground "black")
        'mouse-face 'highlight)
       (propertize "  │  " 'face 'shadow))
     neomacs-wasm-landing-browse-repository
     :help "Open https://github.com/eval-exec/neomacs in a browser tab")))

(defun neomacs-wasm-landing-browse-repository ()
  "Open the neomacs repository in a browser tab."
  (interactive)
  (neomacs-wasm-browse-url neomacs-wasm-landing--repo-url))

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

(defconst neomacs-wasm-landing--site-root
  (expand-file-name "site/" (file-name-directory (or load-file-name buffer-file-name)))
  "This example's read-only site documents.")

(defun neomacs-wasm-landing--root ()
  "Return the packaged site's directory."
  neomacs-wasm-landing--site-root)

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
      (let ((banner (expand-file-name "assets/neomacs-banner.svg"
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
                            (expand-file-name "assets/author.jpg"
                                              (neomacs-wasm-landing--root))
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

(defvar neomacs-wasm-landing--resizing nil)

(defun neomacs-wasm-landing--remember-layout (frame)
  "Record FRAME's width and landing windows after arranging its panes."
  (set-frame-parameter
   frame 'neomacs-wasm-landing-layout
   (cons (window-total-width (frame-root-window frame))
         (mapcar (lambda (window) (cons window (window-buffer window)))
                 (window-list frame 'no-minibuf)))))

(defun neomacs-wasm-landing--resize-layout (frame)
  "Restore missing landing panes when FRAME grows wide enough.
Stop managing the layout if the user splits a window, switches its buffer,
or closes a pane without a frame-width change.  Existing panes and buffer
contents are preserved."
  (when-let* (((not neomacs-wasm-landing--resizing))
              (layout (frame-parameter frame 'neomacs-wasm-landing-layout)))
    (with-selected-frame frame
      (let* ((width (window-total-width (frame-root-window frame)))
             (windows (window-list frame 'no-minibuf))
             (unchanged-width (= width (car layout)))
             (known-windows
              (seq-every-p (lambda (window)
                            (eq (window-buffer window)
                                (cdr (assq window (cdr layout)))))
                          windows)))
        (cond
         ((or (not known-windows)
              (and unchanged-width (/= (length windows) (length (cdr layout)))))
          (set-frame-parameter frame 'neomacs-wasm-landing-layout nil))
         ((not unchanged-width)
          (let ((neomacs-wasm-landing--resizing t))
            (save-selected-window
              (when (or (get-buffer-window "*NEO Emacs*" frame)
                        (get-buffer-window "*Playgorund*" frame))
                (when (and (>= width 130) (featurep 'treemacs)
                           (not (seq-some
                                 (lambda (window)
                                   (with-current-buffer (window-buffer window)
                                     (derived-mode-p 'treemacs-mode)))
                                 windows)))
                  (neomacs-wasm-landing--tree))
                (when-let* (((>= width 170))
                            (neomacs-wasm-landing-personal-info)
                            (info (get-buffer "*About*"))
                            ((not (get-buffer-window info frame))))
                  (display-buffer-in-side-window
                   info '((side . right) (slot . 0) (window-width . 33))))
                (let ((welcome (get-buffer-window "*NEO Emacs*" frame))
                      (playground (get-buffer-window "*Playgorund*" frame)))
                  (cond
                   ((and welcome (not playground) (get-buffer "*Playgorund*")
                         (>= (window-total-width welcome) 85))
                    (set-window-buffer (split-window welcome nil 'right)
                                       (get-buffer "*Playgorund*")))
                   ((and playground (not welcome) (get-buffer "*NEO Emacs*")
                         (>= (window-total-width playground) 85))
                    (set-window-buffer (split-window playground nil 'left)
                                       (get-buffer "*NEO Emacs*")))))))
            (neomacs-wasm-landing--remember-layout frame))))))))

(defun neomacs-wasm-landing-open ()
  "Open the landing page, Treemacs, playground, and personal sidebar.
Medium frames omit the personal sidebar; narrow frames show welcome only.
All buffers remain accessible with C-x b.  Reopening preserves playground
edits.  Frame resize events restore missing panes until the user changes
the landing window layout."
  (interactive)
  (let ((welcome (neomacs-wasm-landing--welcome))
        (info (neomacs-wasm-landing--about)))
    (neomacs-wasm-landing--playground-buffer)
    (when (bound-and-true-p tab-bar-mode)
      (tab-bar-rename-tab "Landing")
      ;; Far left: window icon and product name, then the conspicuous
      ;; work-in-progress warning linking to the repository.
      (add-to-list 'tab-bar-format 'neomacs-wasm-landing--tab-bar-warning)
      (add-to-list 'tab-bar-format 'neomacs-wasm-landing--tab-bar-branding)
      (add-to-list 'tab-bar-format 'neomacs-wasm-landing--tab-bar-icon-item))
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
        (display-buffer-in-side-window info '((side . right) (slot . 0) (window-width . 33))))
      (when (>= (window-total-width) 85)
        (let ((right (split-window-right)))
          (set-window-buffer right (get-buffer "*Playgorund*"))
          (select-window right)))))
  (neomacs-wasm-landing--remember-layout (selected-frame))
  (add-hook 'window-size-change-functions #'neomacs-wasm-landing--resize-banner)
  (add-hook 'window-size-change-functions #'neomacs-wasm-landing--resize-layout)
  (neomacs-wasm-landing--resize-banner)
  (when (bound-and-true-p neomacs-wasm-package-error)
    (message "Optional landing packages unavailable: %s. Reload to retry."
             neomacs-wasm-package-error)))

(provide 'neomacs-wasm-landing)
;;; neomacs-wasm-landing.el ends here
