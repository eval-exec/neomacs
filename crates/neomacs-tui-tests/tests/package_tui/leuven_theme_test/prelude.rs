pub(super) const LEUVEN_TUI_PRELUDE: &str = r####"
(require 'cl-lib)

(defvar neomacs-leuven-tui-lifecycle-report nil)
(defvar neomacs-leuven-tui-phase nil)
(defvar neomacs-leuven-tui-original-controls nil)
(defvar neomacs-leuven-tui-light-scale-history nil)
(defvar neomacs-leuven-tui-dark-scale-history nil)

(defconst neomacs-leuven-tui-buffers
  '("*Leuven Elisp*" "*Leuven Org*" "*Leuven Diff*"))

(defconst neomacs-leuven-tui-control-symbols
  '(leuven-scale-org-document-title
    leuven-scale-outline-headlines
    leuven-scale-org-agenda-structure
    leuven-scale-volatile-highlight
    leuven-dark-scale-org-document-title
    leuven-dark-scale-outline-headlines
    leuven-dark-scale-org-agenda-structure
    leuven-dark-scale-volatile-highlight))

(defun neomacs-leuven-tui-face (face attributes)
  "Return direct and resolved ATTRIBUTES for FACE on the selected frame."
  (list
   :direct
   (mapcar (lambda (attribute)
             (cons attribute (face-attribute face attribute nil nil)))
           attributes)
   :resolved
   (mapcar (lambda (attribute)
             (cons attribute (face-attribute face attribute nil 'default)))
           attributes)))

(defun neomacs-leuven-tui-default-state ()
  "Return public lifecycle and default-face state for the selected frame."
  (list
   :enabled (copy-sequence custom-enabled-themes)
   :mode (frame-parameter nil 'background-mode)
   :direct
   (list (face-attribute 'default :foreground nil nil)
         (face-attribute 'default :background nil nil))
   :resolved
   (list (face-attribute 'default :foreground nil 'default)
         (face-attribute 'default :background nil 'default))))

(defun neomacs-leuven-tui-source-directory ()
  "Return the installed Leuven directory selected over GNU's built-in copy."
  (let ((file
         (locate-file
          "leuven-theme.el"
          (cl-remove-if-not #'stringp custom-theme-load-path))))
    (and file
         (file-name-nondirectory
          (directory-file-name (file-name-directory file))))))

(defun neomacs-leuven-tui-start ()
  "Drive the real public light/dark lifecycle and display its exact report."
  (let* ((baseline (neomacs-leuven-tui-default-state))
         (loaded-no-enable (load-theme 'leuven t t))
         (registered
          (list :result loaded-no-enable
                :known (and (custom-theme-p 'leuven) t)
                :enabled (copy-sequence custom-enabled-themes))))
    (enable-theme 'leuven)
    (let ((light (neomacs-leuven-tui-default-state)))
      (load-theme 'leuven-dark t)
      (let ((dark (neomacs-leuven-tui-default-state)))
        (disable-theme 'leuven-dark)
        (let ((light-restored (neomacs-leuven-tui-default-state)))
          (disable-theme 'leuven)
          (let ((restored (neomacs-leuven-tui-default-state)))
            ;; A repeated public disable is deliberately a no-op.
            (disable-theme 'leuven)
            (with-current-buffer (get-buffer-create "*Leuven Lifecycle*")
              (let ((inhibit-read-only t))
                (erase-buffer)
                (insert
                 (format "CAP %S\n"
                         (list :cells (display-color-cells)
                               :visual-class (display-visual-class)
                               :display-type
                               (frame-parameter nil 'display-type)
                               :graphic (display-graphic-p)
                               :gate
                               (face-spec-set-match-display
                                '((class color) (min-colors 89)) nil)))
                 (format "SOURCE %S\n"
                         (neomacs-leuven-tui-source-directory))
                 (format "REGISTERED %S\n" registered)
                 (format "BASELINE %S\n" baseline)
                 (format "LIGHT %S\n" light)
                 (format "DARK %S\n" dark)
                 (format "LIGHT-RESTORED %S\n" light-restored)
                 (format "BASELINE-RESTORED %S\n" restored)
                 (format "RESTORATION %S\n"
                         (list :light (equal light light-restored)
                               :baseline (equal baseline restored)
                               :second-disable
                               (equal restored
                                      (neomacs-leuven-tui-default-state))))
                 "LEUVEN-TUI-READY\n"))
              (setq neomacs-leuven-tui-lifecycle-report (buffer-string))
              (goto-char (point-min))
              (special-mode)
              (switch-to-buffer (current-buffer))
              (delete-other-windows))))))))

(defun neomacs-leuven-tui-populate-buffers ()
  "Create and fontify the representative real editing buffers."
  (with-current-buffer (get-buffer-create "*Leuven Elisp*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert
       ";; Publish release Ω after review.\n"
       "(defconst release-limit 42)\n"
       "(defun deploy-release (artifact)\n"
       "  \"Ship ARTIFACT safely.\"\n"
       "  (when artifact (message \"ship %s\" artifact)))\n")
      (emacs-lisp-mode)
      (font-lock-ensure)))
  (with-current-buffer (get-buffer-create "*Leuven Org*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert
       "#+title: Release Control Ω\n"
       "* TODO Deploy service\n"
       "** DONE Verify rollback\n"
       "Read the [[https://example.test/runbook][runbook]].\n"
       "#+begin_src emacs-lisp\n"
       "(message \"ship\")\n"
       "#+end_src\n")
      (org-mode)
      (font-lock-ensure)))
  (with-current-buffer (get-buffer-create "*Leuven Diff*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert
       "diff --git a/release.el b/release.el\n"
       "--- a/release.el\n"
       "+++ b/release.el\n"
       "@@ -1,2 +1,2 @@\n"
       " context line\n"
       "-old release\n"
       "+new release Ω\n")
      (diff-mode)
      (font-lock-ensure))))

(defun neomacs-leuven-tui-direct-heights ()
  "Return direct applied heights for all documented scaled surfaces."
  (mapcar (lambda (face) (face-attribute face :height nil nil))
          '(org-document-title org-level-1 org-level-2
            org-agenda-structure org-agenda-date next-error)))

(defun neomacs-leuven-tui-set-controls (prefix values)
  "Set the four public PREFIX scaling controls to VALUES through Custom."
  (cl-mapc
   (lambda (suffix value)
     (customize-set-variable
      (intern (format "%s-scale-%s" prefix suffix)) value))
   '(org-document-title outline-headlines
     org-agenda-structure volatile-highlight)
   values))

(defun neomacs-leuven-tui-exercise-light-controls ()
  "Record numeric/nil/default/reload behavior for all light controls."
  (unless neomacs-leuven-tui-light-scale-history
    (neomacs-leuven-tui-set-controls 'leuven '(1.45 1.25 1.75 1.2))
    (load-theme 'leuven t)
    (let ((numeric (neomacs-leuven-tui-direct-heights)))
      (neomacs-leuven-tui-set-controls 'leuven '(nil nil nil nil))
      (let ((nil-before-reload (neomacs-leuven-tui-direct-heights)))
        (load-theme 'leuven t)
        (let ((nil-after-reload (neomacs-leuven-tui-direct-heights)))
          (neomacs-leuven-tui-set-controls 'leuven '(t t t t))
          (load-theme 'leuven t)
          (let ((defaults (neomacs-leuven-tui-direct-heights)))
            (neomacs-leuven-tui-set-controls
             'leuven '(1.45 1.25 1.75 1.2))
            (load-theme 'leuven t)
            (setq neomacs-leuven-tui-light-scale-history
                  (list (list 'numeric numeric)
                        (list 'nil-before-reload nil-before-reload)
                        (list 'nil-after-reload nil-after-reload)
                        (list 'default defaults)
                        (list 'final-numeric
                              (neomacs-leuven-tui-direct-heights))))))))))

(defun neomacs-leuven-tui-exercise-dark-controls ()
  "Record numeric/nil/default/reload behavior for all dark controls."
  (unless neomacs-leuven-tui-dark-scale-history
    ;; Measure dark without Leuven light supplying direct heights beneath it.
    ;; Rebuild the public dark-over-light stack after the control matrix.
    (when (custom-theme-enabled-p 'leuven)
      (disable-theme 'leuven))
    (neomacs-leuven-tui-set-controls
     'leuven-dark '(1.55 1.35 1.65 1.15))
    (load-theme 'leuven-dark t)
    (let ((numeric (neomacs-leuven-tui-direct-heights)))
      (neomacs-leuven-tui-set-controls 'leuven-dark '(nil nil nil nil))
      (let ((nil-before-reload (neomacs-leuven-tui-direct-heights)))
        (load-theme 'leuven-dark t)
        (let ((nil-after-reload (neomacs-leuven-tui-direct-heights)))
          (neomacs-leuven-tui-set-controls 'leuven-dark '(t t t t))
          (load-theme 'leuven-dark t)
          (let ((defaults (neomacs-leuven-tui-direct-heights)))
            (neomacs-leuven-tui-set-controls
             'leuven-dark '(1.55 1.35 1.65 1.15))
            (load-theme 'leuven-dark t)
            (setq neomacs-leuven-tui-dark-scale-history
                  (list (list 'numeric numeric)
                        (list 'nil-before-reload nil-before-reload)
                        (list 'nil-after-reload nil-after-reload)
                        (list 'default defaults)
                        (list 'final-numeric
                              (neomacs-leuven-tui-direct-heights)))))))))
    (when (custom-theme-enabled-p 'leuven-dark)
      (disable-theme 'leuven-dark))
    (enable-theme 'leuven)
    (enable-theme 'leuven-dark)))

(defun neomacs-leuven-tui-show-buffer (name)
  "Show test buffer NAME with a deterministic phase marker."
  (switch-to-buffer name)
  (setq-local header-line-format
              (format "LEUVEN %s %s"
                      (upcase (symbol-name neomacs-leuven-tui-phase)) name))
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun neomacs-leuven-tui-use-light ()
  "Select light Leuven and show the real Elisp buffer."
  (interactive)
  (when (custom-theme-enabled-p 'leuven-dark)
    (disable-theme 'leuven-dark))
  (unless (custom-theme-enabled-p 'leuven)
    (enable-theme 'leuven))
  ;; Leuven was enabled before these mode faces were defined.  Their late
  ;; `defface' calls must still pick up the already-enabled theme.
  (require 'org)
  (require 'org-agenda)
  (require 'diff-mode)
  (unless neomacs-leuven-tui-original-controls
    (setq neomacs-leuven-tui-original-controls
          (mapcar (lambda (symbol) (cons symbol (symbol-value symbol)))
                  neomacs-leuven-tui-control-symbols)))
  (neomacs-leuven-tui-exercise-light-controls)
  (setq neomacs-leuven-tui-phase 'light)
  (neomacs-leuven-tui-populate-buffers)
  (neomacs-leuven-tui-show-buffer "*Leuven Elisp*"))

(defun neomacs-leuven-tui-use-dark ()
  "Stack dark Leuven over light and show the real Elisp buffer."
  (interactive)
  (unless (custom-theme-enabled-p 'leuven)
    (enable-theme 'leuven))
  (neomacs-leuven-tui-exercise-dark-controls)
  (setq neomacs-leuven-tui-phase 'dark)
  (neomacs-leuven-tui-populate-buffers)
  (neomacs-leuven-tui-show-buffer "*Leuven Elisp*"))

(defun neomacs-leuven-tui-show-elisp ()
  (interactive)
  (neomacs-leuven-tui-show-buffer "*Leuven Elisp*"))

(defun neomacs-leuven-tui-show-org ()
  (interactive)
  (neomacs-leuven-tui-show-buffer "*Leuven Org*"))

(defun neomacs-leuven-tui-show-diff ()
  (interactive)
  (neomacs-leuven-tui-show-buffer "*Leuven Diff*"))

(defun neomacs-leuven-tui-property-run (buffer token)
  "Return TOKEN's exact real face-property run in BUFFER."
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (search-forward token)
      (let* ((position (match-beginning 0))
             (face (get-text-property position 'face))
             (start position)
             (end position))
        (while (and (> start (point-min))
                    (equal face (get-text-property (1- start) 'face)))
          (setq start (1- start)))
        (while (and (< end (point-max))
                    (equal face (get-text-property end 'face)))
          (setq end (1+ end)))
        (list :token token :face face
              :run (buffer-substring-no-properties start end))))))

(defun neomacs-leuven-tui-report-face (face attributes)
  "Insert one compact direct/resolved FACE report."
  (insert
   (format "FACE-D %s %S\n"
           face
           (mapcar (lambda (attribute)
                     (cons attribute
                           (face-attribute face attribute nil nil)))
                   attributes))
   (format "FACE-R %s %S\n"
           face
           (mapcar (lambda (attribute)
                     (cons attribute
                           (face-attribute face attribute nil 'default)))
                   attributes))))

(defun neomacs-leuven-tui-report-run (buffer token)
  "Insert one compact real property-run report."
  (let ((run (neomacs-leuven-tui-property-run buffer token))
        (print-escape-newlines t))
    (insert (format "RUN %s %S %S\n"
                    (substring buffer 8 -1)
                    (plist-get run :face)
                    (plist-get run :run)))))

(defun neomacs-leuven-tui-show-report ()
  "Show exact applied faces and real font-lock runs for the active variant."
  (interactive)
  (with-current-buffer (get-buffer-create "*Leuven Lifecycle*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert (format "PHASE %s %S\n"
                      neomacs-leuven-tui-phase
                      (copy-sequence custom-enabled-themes)))
      (dolist (entry
               (if (eq neomacs-leuven-tui-phase 'light)
                   neomacs-leuven-tui-light-scale-history
                 neomacs-leuven-tui-dark-scale-history))
        (insert (format "SCALE-DIRECT %s %s %S\n"
                        neomacs-leuven-tui-phase (car entry) (cadr entry))))
      (neomacs-leuven-tui-report-face 'default '(:foreground :background))
      (neomacs-leuven-tui-report-face
       'font-lock-comment-face '(:foreground :slant))
      (neomacs-leuven-tui-report-face 'font-lock-keyword-face '(:foreground))
      (neomacs-leuven-tui-report-face
       'font-lock-function-name-face '(:foreground))
      (neomacs-leuven-tui-report-face 'diff-context '(:foreground :background))
      (neomacs-leuven-tui-report-face
       'diff-header '(:foreground :background :weight))
      (neomacs-leuven-tui-report-face
       'org-document-title '(:foreground :weight :height))
      (neomacs-leuven-tui-report-face
       'org-level-1 '(:foreground :background :height))
      (neomacs-leuven-tui-report-face 'org-link '(:foreground :underline))
      (neomacs-leuven-tui-report-face 'org-block '(:foreground :background))
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" ";; Publish release")
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" "defun")
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" "deploy-release")
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" "\"Ship ARTIFACT safely.\"")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "Release Control")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "TODO")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "DONE")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "runbook")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "#+begin_src")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "diff --git")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "@@ -1,2")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" " context line")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "-old release")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "+new release"))
    (goto-char (point-min))
    (special-mode)
    (switch-to-buffer (current-buffer))
    (delete-other-windows)))

(defun neomacs-leuven-tui-finish ()
  "Disable both themes, kill fixtures, and prove baseline restoration."
  (interactive)
  (dolist (theme '(leuven-dark leuven))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme)))
  (dolist (entry neomacs-leuven-tui-original-controls)
    (customize-set-variable (car entry) (cdr entry)))
  (dolist (name neomacs-leuven-tui-buffers)
    (when (get-buffer name) (kill-buffer name)))
  (when (get-buffer "*Leuven Lifecycle*")
    (kill-buffer "*Leuven Lifecycle*"))
  (switch-to-buffer (get-buffer-create "*Leuven Clean*"))
  (let ((inhibit-read-only t))
    (erase-buffer)
    (insert (format "LEUVEN-TUI-CLEAN %S"
                    (neomacs-leuven-tui-default-state))))
  (delete-other-windows))

(add-hook 'emacs-startup-hook #'neomacs-leuven-tui-start)
"####;

pub(super) const REPORT_PREFIXES: &[&str] = &[
    "CAP ",
    "SOURCE ",
    "REGISTERED ",
    "BASELINE ",
    "LIGHT ",
    "DARK ",
    "LIGHT-RESTORED ",
    "BASELINE-RESTORED ",
    "RESTORATION ",
    "PHASE ",
    "SCALE-DIRECT ",
    "FACE-D ",
    "FACE-R ",
    "RUN ",
    "LEUVEN-TUI-READY",
];
