pub(super) const HELM_CSS_SCSS_DEFAULT_TUI_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

(defconst neomacs-hcss-fixture
  "/* A disabled prototype is intentionally excluded. */
/* .disabled {
  color: gray;
} */

.dashboard,
.dashboard--compact {
  color: red;

  .card {
    padding: 1rem;

    &__title,
    &__subtitle {
      color: blue;
    }
  }
}

.footer {
  color: black;
}
")

(defvar neomacs-hcss-default-root nil)
(defvar neomacs-hcss-default-root-owned nil)

(defun neomacs-hcss-default-setup ()
  "Create the exact real SCSS editing fixture below the editor sandbox."
  (require 'css-mode)
  (require 'helm-css-scss)
  (let ((home (getenv "HOME")))
    (unless (and (stringp home) (> (length home) 0)
                 (file-name-absolute-p home))
      (error "NEOMACS-HCSS: HOME must be a nonempty absolute sandbox path"))
    (setq neomacs-hcss-default-root
          (expand-file-name "helm-css-scss-default/"
                            (file-name-as-directory home)))
    (when (file-exists-p neomacs-hcss-default-root)
      (error "NEOMACS-HCSS: default owned root already exists: %s"
             neomacs-hcss-default-root)))
  (let ((file (expand-file-name "tui-fixture.scss"
                                neomacs-hcss-default-root)))
    (make-directory neomacs-hcss-default-root)
    (setq neomacs-hcss-default-root-owned t)
    (with-temp-file file
      (insert neomacs-hcss-fixture))
    (find-file file)
    (scss-mode)
    (setq-local helm-css-scss-include-commented-selector nil)
    (goto-char (point-min))
    (search-forward "padding")
    (beginning-of-line)
    (set-buffer-modified-p nil)
    (message "HCSS-DEFAULT-READY")))

(defun neomacs-hcss-session-advice-state ()
  "Return only the package's temporary single-session advice state."
  (list
   (and (ad-advice-enabled
         (ad-find-advice 'helm-next-line 'around
                         'helm-css-scss--next-line)) t)
   (and (ad-advice-enabled
         (ad-find-advice 'helm-previous-line 'around
                         'helm-css-scss--previous-line)) t)))

(defun neomacs-hcss-default-run ()
  "Invoke the unadapted public command and report its exact failure cleanup."
  (interactive)
  (let* ((source (current-buffer))
         (file buffer-file-name)
         (root neomacs-hcss-default-root)
         (fold (make-overlay (point-at-bol) (min (point-max) (1+ (point-at-eol)))))
         outcome post-public report-buffer cleanup-error)
    (overlay-put fold 'invisible 'neomacs-hcss-test-fold)
    (setq outcome
          (condition-case condition
              (list :value (helm-css-scss))
            (error
             (list
              :error (car condition)
              :arity (and (functionp (cadr condition))
                          (func-arity (cadr condition)))
              :received (car (last condition))))))
    (setq post-public
          (list
           :outcome outcome
           :buffer (buffer-name source)
           :point (with-current-buffer source (point))
           :line (with-current-buffer source (line-number-at-pos))
           :cache-count
           (with-current-buffer source
             (and (boundp 'helm-css-scss-cache)
                  (length helm-css-scss-cache)))
           :last-point
           (and (consp helm-css-scss-last-point)
                (cons (car helm-css-scss-last-point)
                      (buffer-name (get-buffer (cdr helm-css-scss-last-point)))))
           :last-query
           (with-current-buffer source
             (and (boundp 'helm-css-scss-last-query)
                  helm-css-scss-last-query))
           :fold-invisible (overlay-get fold 'invisible)
           :recorded-invisible helm-css-scss-invisible-targets
           :package-overlay-buffer
           (and (overlayp helm-css-scss-overlay)
                (let ((buffer (overlay-buffer helm-css-scss-overlay)))
                  (and buffer (buffer-name buffer))))
           :session-advices (neomacs-hcss-session-advice-state)
           :session-hook
           (and (memq #'helm-css-scss--keep-nearest-position
                      helm-after-update-hook) t)
           :helm-alive (and helm-alive-p t)
           :helm-buffers
           (seq-filter #'get-buffer
                       (list helm-css-scss-buffer
                             helm-css-scss-multi-buffer
                             "*helm action*"))
           :modified (with-current-buffer source (buffer-modified-p))))
    (setq report-buffer (get-buffer-create "*Helm CSS SCSS Default Failure*"))
    (switch-to-buffer report-buffer)
    (delete-other-windows)
    ;; Everything below is test-owned teardown.  The post-public snapshot above
    ;; remains the package cleanup oracle and therefore cannot be hidden here.
    (condition-case condition
        (when (overlayp helm-css-scss-overlay)
          (delete-overlay helm-css-scss-overlay))
      (error (setq cleanup-error condition)))
    (condition-case condition
        (helm-css-scss--restore-unveiled-overlay)
      (error (unless cleanup-error (setq cleanup-error condition))))
    (condition-case condition
        (delete-overlay fold)
      (error (unless cleanup-error (setq cleanup-error condition))))
    (condition-case condition
        (when (buffer-live-p source)
          (with-current-buffer source (set-buffer-modified-p nil))
          (kill-buffer source))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (dolist (name (list helm-css-scss-buffer
                        helm-css-scss-multi-buffer
                        "*helm action*"))
      (condition-case condition
          (when (get-buffer name) (kill-buffer name))
        (error (unless cleanup-error (setq cleanup-error condition)))))
    (condition-case condition
        (when neomacs-hcss-default-root-owned
          (when (file-exists-p root) (delete-directory root t))
          (unless (file-exists-p root)
            (setq neomacs-hcss-default-root-owned nil)))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (let ((cleanup
           (list
            :source-live (and (buffer-live-p source) t)
            :fold-buffer (and (overlayp fold) (overlay-buffer fold))
            :package-overlay-buffer
            (and (overlayp helm-css-scss-overlay)
                 (overlay-buffer helm-css-scss-overlay))
            :session-advices (neomacs-hcss-session-advice-state)
            :session-hook
            (and (memq #'helm-css-scss--keep-nearest-position
                       helm-after-update-hook) t)
            :helm-alive (and helm-alive-p t)
            :helm-buffers
            (seq-filter #'get-buffer
                        (list helm-css-scss-buffer
                              helm-css-scss-multi-buffer
                              "*helm action*"))
            :root-exists (file-exists-p root)
            :cleanup-error cleanup-error)))
      (with-temp-file (expand-file-name "hcss-default-report.sexp" (getenv "HOME"))
        (prin1 (list :post-public post-public :cleanup cleanup) (current-buffer)))
      (with-current-buffer report-buffer
        (let ((inhibit-read-only t))
          (erase-buffer)
          (insert "HCSS DEFAULT FAILURE\n")
          (prin1 post-public (current-buffer))
          (insert "\nHCSS CLEANUP\n")
          (prin1 cleanup (current-buffer))
          (insert "\nHCSS-DEFAULT-CLEAN\n")
          (goto-char (point-min))
          (special-mode))))))

(add-hook 'emacs-startup-hook #'neomacs-hcss-default-setup 100)
"####;

pub(super) const HELM_CSS_SCSS_SINGLE_TUI_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

(defconst neomacs-hcss-fixture
  "/* A disabled prototype is intentionally excluded. */
/* .disabled {
  color: gray;
} */

.dashboard,
.dashboard--compact {
  color: red;

  .card {
    padding: 1rem;

    &__title,
    &__subtitle {
      color: blue;
    }
  }
}

.footer {
  color: black;
}
")

(defvar neomacs-hcss-single-root nil)
(defvar neomacs-hcss-single-root-owned nil)
(defvar neomacs-hcss-single-source nil)
(defvar neomacs-hcss-single-fold nil)
(defvar neomacs-hcss-single-ledger nil)
(defvar neomacs-hcss-single-event 0)
(defvar neomacs-hcss-single-original-display nil)
(defvar neomacs-hcss-single-map nil)

(defun neomacs-hcss-single-write (name value)
  (with-temp-file (expand-file-name name (getenv "HOME"))
    (let ((print-length nil) (print-level nil) (print-circle nil))
      (prin1 value (current-buffer)))))

(defun neomacs-hcss-single-face-runs (string)
  (let ((position 0) runs)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (or (next-single-property-change position 'face string)
                       (length string))))
        (when face
          (push (list position next face
                      (substring-no-properties string position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-hcss-single-advice-state ()
  (list
   (and (ad-advice-enabled
         (ad-find-advice 'helm-next-line 'around 'helm-css-scss--next-line)) t)
   (and (ad-advice-enabled
         (ad-find-advice 'helm-previous-line 'around 'helm-css-scss--previous-line)) t)))

(defun neomacs-hcss-single-compatible-display (buffer &optional _resume)
  "Adapt current Helm's two-argument display call at the public option seam."
  (funcall neomacs-hcss-single-original-display buffer))

(defun neomacs-hcss-single-selected-line ()
  (when helm-alive-p
    (with-helm-window
      (buffer-substring (line-beginning-position) (line-end-position)))))

(defun neomacs-hcss-single-active-state (stage)
  (let* ((source helm-css-scss-target-buffer)
         (helm-text (and (get-buffer helm-buffer)
                         (with-current-buffer helm-buffer
                           (buffer-substring (point-min) (point-max)))))
         (selected (neomacs-hcss-single-selected-line)))
    (list
     :stage stage
     :alive (and helm-alive-p t)
     :prompt (and (minibufferp) (minibuffer-prompt))
     :pattern helm-pattern
     :source-name (assoc-default 'name (helm-get-current-source))
     :helm-buffer-text (and helm-text (substring-no-properties helm-text))
     :helm-face-runs (and helm-text (neomacs-hcss-single-face-runs helm-text))
     :selected (and selected (substring-no-properties selected))
     :selected-face-runs (and selected (neomacs-hcss-single-face-runs selected))
     :selected-real (copy-tree (helm-get-selection))
     :source
     (and (buffer-live-p source)
          (with-current-buffer source
            (list (buffer-name) (point) (line-number-at-pos) (current-column)
                  (char-before) (char-after)
                  (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position)))))
     :overlay
     (and (overlayp helm-css-scss-overlay)
          (overlay-buffer helm-css-scss-overlay)
          (let ((overlay-buffer (overlay-buffer helm-css-scss-overlay)))
            (with-current-buffer overlay-buffer
              (list (buffer-name overlay-buffer)
                    (overlay-start helm-css-scss-overlay)
                    (overlay-end helm-css-scss-overlay)
                    (buffer-substring-no-properties
                     (overlay-start helm-css-scss-overlay)
                     (overlay-end helm-css-scss-overlay))
                    (overlay-get helm-css-scss-overlay 'face)))))
     :last-line
     (and (consp helm-css-scss-last-line-info)
          (list (buffer-name (car helm-css-scss-last-line-info))
                (cdr helm-css-scss-last-line-info)))
     :fold-invisible (and (overlayp neomacs-hcss-single-fold)
                          (overlay-get neomacs-hcss-single-fold 'invisible))
     :unveiled (mapcar (lambda (entry)
                         (list (overlay-start (car entry))
                               (overlay-end (car entry))
                               (cdr entry)))
                       helm-css-scss-invisible-targets)
     :advices (neomacs-hcss-single-advice-state)
     :update-hook (and (memq #'helm-css-scss--keep-nearest-position
                             helm-after-update-hook) t)
     :windows (mapcar (lambda (window)
                        (buffer-name (window-buffer window)))
                      (seq-remove #'window-minibuffer-p (window-list)))
     :cache-count
     (and (buffer-live-p source)
          (with-current-buffer source
            (and (boundp 'helm-css-scss-cache)
                 (length helm-css-scss-cache)))))))

(defun neomacs-hcss-single-observe ()
  "Record the active real Helm session without changing it."
  (interactive)
  (setq neomacs-hcss-single-event (1+ neomacs-hcss-single-event))
  (setq neomacs-hcss-single-ledger
        (append neomacs-hcss-single-ledger
                (list
                 (condition-case condition
                     (neomacs-hcss-single-active-state
                      (intern (format "active-%d" neomacs-hcss-single-event)))
                   (error
                    (list :stage
                          (intern (format "active-%d" neomacs-hcss-single-event))
                          :observer-error condition))))))
  (neomacs-hcss-single-write "hcss-single-ledger.sexp"
                             neomacs-hcss-single-ledger)
  (message "HCSS-SINGLE-OBSERVED-%d" neomacs-hcss-single-event))

(defun neomacs-hcss-single-post-state (stage)
  (let ((source neomacs-hcss-single-source))
    (list
     :stage stage
     :current (buffer-name)
     :windows (mapcar (lambda (window)
                        (if (eq (window-buffer window) source)
                            (buffer-name source)
                          :other))
                      (seq-remove #'window-minibuffer-p (window-list)))
     :source
     (and (buffer-live-p source)
          (with-current-buffer source
            (list (buffer-name) (point) (line-number-at-pos) (current-column)
                  (char-before) (char-after)
                  (and (buffer-modified-p) t)
                  (and (boundp 'helm-css-scss-cache)
                       (length helm-css-scss-cache))
                  (and (boundp 'helm-css-scss-last-query)
                       helm-css-scss-last-query))))
     :disk-bytes
     (and (buffer-live-p source)
          (with-current-buffer source
            (let ((file buffer-file-name))
              (and file
                   (file-readable-p file)
                   (with-temp-buffer
                     (insert-file-contents-literally file)
                     (buffer-string))))))
     :last-point
     (and (consp helm-css-scss-last-point)
          (list (car helm-css-scss-last-point) (cdr helm-css-scss-last-point)))
     :fold-invisible (and (overlayp neomacs-hcss-single-fold)
                          (overlay-get neomacs-hcss-single-fold 'invisible))
     :unveiled
     (mapcar (lambda (entry)
               (list (overlay-start (car entry))
                     (overlay-end (car entry))
                     (cdr entry)))
             helm-css-scss-invisible-targets)
     :package-overlay-buffer
     (and (overlayp helm-css-scss-overlay)
          (overlay-buffer helm-css-scss-overlay)
          (buffer-name (overlay-buffer helm-css-scss-overlay)))
     :advices (neomacs-hcss-single-advice-state)
     :update-hook (and (memq #'helm-css-scss--keep-nearest-position
                             helm-after-update-hook) t)
     :helm-alive (and helm-alive-p t)
     :helm-buffers
     (seq-filter #'get-buffer
                 (list helm-css-scss-buffer helm-css-scss-multi-buffer
                       "*helm action*")))))

(defun neomacs-hcss-single-record-post (stage)
  (setq neomacs-hcss-single-ledger
        (append neomacs-hcss-single-ledger
                (list
                 (condition-case condition
                     (neomacs-hcss-single-post-state stage)
                   (error (list :stage stage :post-error condition))))))
  (neomacs-hcss-single-write "hcss-single-ledger.sexp"
                             neomacs-hcss-single-ledger)
  (message "HCSS-SINGLE-POST-%s" stage))

(defun neomacs-hcss-single-post-cancel ()
  (interactive)
  (neomacs-hcss-single-record-post 'cancel))

(defun neomacs-hcss-single-post-open ()
  (interactive)
  (neomacs-hcss-single-record-post 'open-action))

(defun neomacs-hcss-single-post-close ()
  (interactive)
  (neomacs-hcss-single-record-post 'close-action))

(defun neomacs-hcss-single-back-one ()
  (interactive)
  (helm-css-scss-back-to-last-point)
  (neomacs-hcss-single-record-post 'back-one))

(defun neomacs-hcss-single-back-two ()
  (interactive)
  (helm-css-scss-back-to-last-point)
  (neomacs-hcss-single-record-post 'back-two))

(defun neomacs-hcss-single-post-unsaved ()
  (interactive)
  (neomacs-hcss-single-record-post 'unsaved-cache))

(defun neomacs-hcss-single-post-save ()
  (interactive)
  (neomacs-hcss-single-record-post 'saved-cache-cleared))

(defun neomacs-hcss-single-post-rebuilt ()
  (interactive)
  (neomacs-hcss-single-record-post 'saved-cache-rebuilt-action))

(defun neomacs-hcss-single-post-isearch-literal ()
  (interactive)
  (neomacs-hcss-single-record-post 'isearch-literal-cancel))

(defun neomacs-hcss-single-post-isearch-regexp ()
  (interactive)
  (neomacs-hcss-single-record-post 'isearch-regexp-cancel))

(defun neomacs-hcss-single-configure-public-display ()
  "Select the documented public display option for direct public commands."
  (interactive)
  (setq helm-css-scss-split-window-function
        #'neomacs-hcss-single-compatible-display)
  (message "HCSS-SINGLE-PUBLIC-DISPLAY-CONFIGURED"))

(defun neomacs-hcss-single-restore-test-fold ()
  "Restore only the test-owned fold after its public success snapshot."
  (interactive)
  (when (overlayp neomacs-hcss-single-fold)
    (overlay-put neomacs-hcss-single-fold 'invisible
                 'neomacs-hcss-single-fold))
  (setq helm-css-scss-invisible-targets nil)
  (message "HCSS-SINGLE-TEST-FOLD-RESTORED"))

(defun neomacs-hcss-single-command (&optional query)
  (let ((helm-css-scss-map neomacs-hcss-single-map)
        (helm-css-scss-split-window-function
         #'neomacs-hcss-single-compatible-display))
    (helm-css-scss query)))

(defun neomacs-hcss-single-start ()
  (interactive)
  (neomacs-hcss-single-command nil))

(defun neomacs-hcss-single-start-new-release ()
  (interactive)
  (neomacs-hcss-single-command "new-release"))

(defun neomacs-hcss-single-setup ()
  (require 'css-mode)
  (require 'helm-css-scss)
  (let ((home (getenv "HOME")))
    (unless (and (stringp home) (> (length home) 0)
                 (file-name-absolute-p home))
      (error "NEOMACS-HCSS: HOME must be a nonempty absolute sandbox path"))
    (setq neomacs-hcss-single-root
          (expand-file-name "helm-css-scss-single/"
                            (file-name-as-directory home))))
  (when (file-exists-p neomacs-hcss-single-root)
    (error "NEOMACS-HCSS: single owned root already exists: %s"
           neomacs-hcss-single-root))
  (let ((file (expand-file-name "tui-fixture.scss"
                                neomacs-hcss-single-root)))
    (make-directory neomacs-hcss-single-root)
    (setq neomacs-hcss-single-root-owned t)
    (with-temp-file file (insert neomacs-hcss-fixture))
    (find-file file)
    (scss-mode)
    (setq-local helm-css-scss-include-commented-selector nil)
    (goto-char (point-min))
    (search-forward ".card")
    (setq neomacs-hcss-single-fold
          (make-overlay (line-beginning-position)
                        (min (point-max) (1+ (line-end-position)))))
    (overlay-put neomacs-hcss-single-fold 'invisible
                 'neomacs-hcss-single-fold)
    (search-forward "padding")
    (beginning-of-line)
    (set-buffer-modified-p nil)
    (setq neomacs-hcss-single-source (current-buffer)
          neomacs-hcss-single-original-display
          helm-css-scss-split-window-function
          neomacs-hcss-single-map (copy-keymap helm-css-scss-map))
    (define-key neomacs-hcss-single-map (kbd "C-c t")
                #'neomacs-hcss-single-observe)
    (message "HCSS-SINGLE-READY")))

(defun neomacs-hcss-single-finish ()
  (interactive)
  (let ((source neomacs-hcss-single-source)
        (root neomacs-hcss-single-root)
        cleanup-error)
    (condition-case condition
        (when helm-alive-p (helm-keyboard-quit))
      (error (setq cleanup-error condition)))
    (condition-case condition
        (when (overlayp helm-css-scss-overlay)
          (delete-overlay helm-css-scss-overlay))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (condition-case condition
        (helm-css-scss--restore-unveiled-overlay)
      (error (unless cleanup-error (setq cleanup-error condition))))
    (condition-case condition
        (when (overlayp neomacs-hcss-single-fold)
          (delete-overlay neomacs-hcss-single-fold))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (dolist (name (list helm-css-scss-buffer helm-css-scss-multi-buffer
                        "*helm action*"))
      (condition-case condition
          (when (get-buffer name) (kill-buffer name))
        (error (unless cleanup-error (setq cleanup-error condition)))))
    (condition-case condition
        (when (buffer-live-p source)
          (with-current-buffer source (set-buffer-modified-p nil))
          (kill-buffer source))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (setq helm-css-scss-split-window-function
          neomacs-hcss-single-original-display)
    (condition-case condition
        (when neomacs-hcss-single-root-owned
          (when (file-exists-p root) (delete-directory root t))
          (unless (file-exists-p root)
            (setq neomacs-hcss-single-root-owned nil)))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (let ((cleanup
           (list :source-live (and (buffer-live-p source) t)
                 :root-exists (file-exists-p root)
                 :fold-buffer (and (overlayp neomacs-hcss-single-fold)
                                   (overlay-buffer neomacs-hcss-single-fold))
                 :overlay-buffer (and (overlayp helm-css-scss-overlay)
                                      (overlay-buffer helm-css-scss-overlay))
                 :unveiled helm-css-scss-invisible-targets
                 :advices (neomacs-hcss-single-advice-state)
                 :update-hook
                 (and (memq #'helm-css-scss--keep-nearest-position
                            helm-after-update-hook) t)
                 :helm-alive (and helm-alive-p t)
                 :helm-buffers
                 (seq-filter #'get-buffer
                             (list helm-css-scss-buffer
                                   helm-css-scss-multi-buffer "*helm action*"))
                 :display-restored
                 (eq helm-css-scss-split-window-function
                     neomacs-hcss-single-original-display)
                 :cleanup-error cleanup-error)))
      (setq neomacs-hcss-single-ledger
            (append neomacs-hcss-single-ledger (list (list :cleanup cleanup))))
      (neomacs-hcss-single-write "hcss-single-report.sexp"
                                 neomacs-hcss-single-ledger)
      (let ((report (get-buffer-create "*Helm CSS SCSS Single Report*")))
        (switch-to-buffer report)
        (delete-other-windows)
        (erase-buffer)
        (insert "HCSS-SINGLE-CLEAN\n")
        (prin1 cleanup (current-buffer))
        (insert "\n")
        (goto-char (point-min))
        (special-mode)))))

(add-hook 'emacs-startup-hook #'neomacs-hcss-single-setup 100)
"####;

pub(super) const HELM_CSS_SCSS_MULTI_TUI_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

(defconst neomacs-hcss-multi-scss
  "/* A disabled prototype is intentionally excluded. */
/* .disabled {
  color: gray;
} */

.dashboard,
.dashboard--compact {
  color: red;

  .card {
    padding: 1rem;

    &__title,
    &__subtitle {
      color: blue;
    }
  }
}

.footer {
  color: black;
}
")
(defconst neomacs-hcss-multi-css
  ".button {
  display: inline-flex;
}

.button:hover {
  color: rebeccapurple;
}
")
(defconst neomacs-hcss-multi-less
  ".theme {
  color: navy;

  .link {
    text-decoration: underline;
  }
}
")
(defconst neomacs-hcss-multi-uppercase
  ".upper-case-extension {
  color: red;
}
")

(defvar neomacs-hcss-multi-root nil)
(defvar neomacs-hcss-multi-root-owned nil)
(defvar neomacs-hcss-multi-buffers nil)
(defvar neomacs-hcss-multi-fileless nil)
(defvar neomacs-hcss-multi-original-display nil)
(defvar neomacs-hcss-multi-map nil)
(defvar neomacs-hcss-multi-ledger nil)
(defvar neomacs-hcss-multi-event 0)

(defun neomacs-hcss-multi-write (name value)
  (with-temp-file (expand-file-name name (getenv "HOME"))
    (let ((print-length nil) (print-level nil) (print-circle nil))
      (prin1 value (current-buffer)))))

(defun neomacs-hcss-multi-face-runs (string)
  (let ((position 0) runs)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (or (next-single-property-change position 'face string)
                       (length string))))
        (when face
          (push (list position next face
                      (substring-no-properties string position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-hcss-multi-advice-state ()
  (list
   (and (ad-advice-enabled
         (ad-find-advice 'helm-next-line 'around
                         'helm-css-scss-multi--next-line)) t)
   (and (ad-advice-enabled
         (ad-find-advice 'helm-previous-line 'around
                         'helm-css-scss-multi--previous-line)) t)
   (and (ad-advice-enabled
         (ad-find-advice 'helm-move--next-line-fn 'around
                         'helm-css-scss--next-line-cycle)) t)
   (and (ad-advice-enabled
         (ad-find-advice 'helm-move--previous-line-fn 'around
                         'helm-css-scss--previous-line-cycle)) t)))

(defun neomacs-hcss-multi-compatible-display (buffer &optional _resume)
  (funcall neomacs-hcss-multi-original-display buffer))

(defun neomacs-hcss-multi-buffer-points ()
  (mapcar (lambda (buffer)
            (with-current-buffer buffer
              (list (buffer-name) (point) (line-number-at-pos))))
          neomacs-hcss-multi-buffers))

(defun neomacs-hcss-multi-active-state (stage)
  (let* ((helm-text (with-current-buffer helm-buffer
                      (buffer-substring (point-min) (point-max))))
         (selection (with-helm-window
                      (buffer-substring (line-beginning-position)
                                        (line-end-position))))
         (current-source (helm-get-current-source))
         (target (get-buffer (assoc-default 'name current-source))))
    (list
     :stage stage
     :alive (and helm-alive-p t)
     :prompt (and (minibufferp) (minibuffer-prompt))
     :pattern helm-pattern
     :helm-buffer-text (substring-no-properties helm-text)
     :helm-face-runs (neomacs-hcss-multi-face-runs helm-text)
     :selected (substring-no-properties selection)
     :selected-runs (neomacs-hcss-multi-face-runs selection)
     :selected-real (copy-tree (helm-get-selection))
     :current-source (assoc-default 'name current-source)
     :target
     (and (buffer-live-p target)
          (with-current-buffer target
            (list (buffer-name) (point) (line-number-at-pos) (current-column)
                  (char-before) (char-after)
                  (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position)))))
     :overlay
     (and (overlayp helm-css-scss-overlay)
          (overlay-buffer helm-css-scss-overlay)
          (let ((buffer (overlay-buffer helm-css-scss-overlay)))
            (with-current-buffer buffer
              (list (buffer-name buffer)
                    (overlay-start helm-css-scss-overlay)
                    (overlay-end helm-css-scss-overlay)
                    (buffer-substring-no-properties
                     (overlay-start helm-css-scss-overlay)
                     (overlay-end helm-css-scss-overlay))))))
     :buffer-points (neomacs-hcss-multi-buffer-points)
     :fileless-present
     (and (string-match-p "not-a-file\\.css" helm-text) t)
     :advices (neomacs-hcss-multi-advice-state)
     :helm-windows
     (mapcar (lambda (window) (buffer-name (window-buffer window)))
             (seq-remove #'window-minibuffer-p (window-list))))))

(defun neomacs-hcss-multi-observe ()
  (interactive)
  (setq neomacs-hcss-multi-event (1+ neomacs-hcss-multi-event))
  (setq neomacs-hcss-multi-ledger
        (append neomacs-hcss-multi-ledger
                (list
                 (condition-case condition
                     (neomacs-hcss-multi-active-state
                      (intern (format "active-%d" neomacs-hcss-multi-event)))
                   (error
                    (list :stage
                          (intern (format "active-%d" neomacs-hcss-multi-event))
                          :observer-error condition))))))
  (neomacs-hcss-multi-write "hcss-multi-ledger.sexp"
                            neomacs-hcss-multi-ledger)
  (message "HCSS-MULTI-OBSERVED-%d" neomacs-hcss-multi-event))

(defun neomacs-hcss-multi-start ()
  (interactive)
  (let ((helm-map neomacs-hcss-multi-map)
        (helm-css-scss-map neomacs-hcss-multi-map)
        (helm-css-scss-include-commented-selector nil)
        (helm-css-scss-split-window-function
         #'neomacs-hcss-multi-compatible-display))
    (helm-css-scss-multi)))

(defun neomacs-hcss-multi-post-action ()
  (interactive)
  (let ((state
         (list
          :stage 'post-action
          :selected-buffer (buffer-name)
          :point (point) :line (line-number-at-pos)
          :column (current-column)
          :char-before (char-before) :char-after (char-after)
          :buffer-points (neomacs-hcss-multi-buffer-points)
          :overlay-buffer (and (overlayp helm-css-scss-overlay)
                               (overlay-buffer helm-css-scss-overlay))
          :advices (neomacs-hcss-multi-advice-state)
          :helm-alive (and helm-alive-p t)
          :helm-buffers
          (seq-filter #'get-buffer
                      (list helm-css-scss-buffer helm-css-scss-multi-buffer
                            "*helm action*")))))
    (setq neomacs-hcss-multi-ledger
          (append neomacs-hcss-multi-ledger (list state)))
    (neomacs-hcss-multi-write "hcss-multi-ledger.sexp"
                              neomacs-hcss-multi-ledger)
    (message "HCSS-MULTI-POST-ACTION")))

(defun neomacs-hcss-multi-setup ()
  (require 'css-mode)
  (require 'less-css-mode)
  (require 'helm-css-scss)
  (let ((home (getenv "HOME")))
    (unless (and (stringp home) (> (length home) 0)
                 (file-name-absolute-p home))
      (error "NEOMACS-HCSS: HOME must be a nonempty absolute sandbox path"))
    (setq neomacs-hcss-multi-root
          (expand-file-name "helm-css-scss-multi/"
                            (file-name-as-directory home))))
  (when (file-exists-p neomacs-hcss-multi-root)
    (error "NEOMACS-HCSS: multi owned root already exists: %s"
           neomacs-hcss-multi-root))
  (make-directory neomacs-hcss-multi-root)
  (setq neomacs-hcss-multi-root-owned t)
  (dolist (entry `(("tui-fixture.scss" ,neomacs-hcss-multi-scss)
                   ("component.css" ,neomacs-hcss-multi-css)
                   ("theme.less" ,neomacs-hcss-multi-less)
                   ("IGNORED.CSS" ,neomacs-hcss-multi-uppercase)))
    (with-temp-file (expand-file-name (car entry) neomacs-hcss-multi-root)
      (insert (cadr entry))))
  (let* ((scss (find-file (expand-file-name "tui-fixture.scss"
                                             neomacs-hcss-multi-root)))
         (css (find-file-noselect (expand-file-name "component.css"
                                                    neomacs-hcss-multi-root)))
         (less (find-file-noselect (expand-file-name "theme.less"
                                                     neomacs-hcss-multi-root)))
         (uppercase (find-file-noselect (expand-file-name "IGNORED.CSS"
                                                          neomacs-hcss-multi-root))))
    (with-current-buffer scss (scss-mode))
    (with-current-buffer css (css-mode))
    (with-current-buffer less (less-css-mode))
    (with-current-buffer uppercase (css-mode))
    (setq neomacs-hcss-multi-buffers (list scss css less uppercase))
    (setq neomacs-hcss-multi-fileless (get-buffer-create "not-a-file.css"))
    (with-current-buffer neomacs-hcss-multi-fileless
      (erase-buffer)
      (insert ".memory-only { color: green; }\n")
      (css-mode))
    (switch-to-buffer scss)
    (goto-char (point-min))
    (search-forward "padding")
    (beginning-of-line)
    (set-buffer-modified-p nil)
    (setq neomacs-hcss-multi-original-display
          helm-css-scss-split-window-function
          neomacs-hcss-multi-map (copy-keymap helm-css-scss-map))
    (define-key neomacs-hcss-multi-map (kbd "C-c t")
                #'neomacs-hcss-multi-observe)
    (message "HCSS-MULTI-READY")))

(defun neomacs-hcss-multi-finish ()
  (interactive)
  (let ((buffers (append neomacs-hcss-multi-buffers
                         (list neomacs-hcss-multi-fileless)))
        (root neomacs-hcss-multi-root)
        cleanup-error)
    (condition-case condition
        (when helm-alive-p (helm-keyboard-quit))
      (error (setq cleanup-error condition)))
    (condition-case condition
        (when (overlayp helm-css-scss-overlay)
          (delete-overlay helm-css-scss-overlay))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (condition-case condition
        (helm-css-scss--restore-unveiled-overlay)
      (error (unless cleanup-error (setq cleanup-error condition))))
    (dolist (name (list helm-css-scss-buffer helm-css-scss-multi-buffer
                        "*helm action*"))
      (condition-case condition
          (when (get-buffer name) (kill-buffer name))
        (error (unless cleanup-error (setq cleanup-error condition)))))
    (dolist (buffer buffers)
      (condition-case condition
          (when (buffer-live-p buffer)
            (with-current-buffer buffer (set-buffer-modified-p nil))
            (kill-buffer buffer))
        (error (unless cleanup-error (setq cleanup-error condition)))))
    (condition-case condition
        (when neomacs-hcss-multi-root-owned
          (when (file-exists-p root) (delete-directory root t))
          (unless (file-exists-p root)
            (setq neomacs-hcss-multi-root-owned nil)))
      (error (unless cleanup-error (setq cleanup-error condition))))
    (let ((cleanup
           (list
            :owned-live (delq nil (mapcar (lambda (buffer)
                                           (and (buffer-live-p buffer)
                                                (buffer-name buffer)))
                                         buffers))
            :root-exists (file-exists-p root)
            :overlay-buffer (and (overlayp helm-css-scss-overlay)
                                 (overlay-buffer helm-css-scss-overlay))
            :advices (neomacs-hcss-multi-advice-state)
            :helm-alive (and helm-alive-p t)
            :helm-buffers
            (seq-filter #'get-buffer
                        (list helm-css-scss-buffer helm-css-scss-multi-buffer
                              "*helm action*"))
            :cleanup-error cleanup-error)))
      (setq neomacs-hcss-multi-ledger
            (append neomacs-hcss-multi-ledger (list (list :cleanup cleanup))))
      (neomacs-hcss-multi-write "hcss-multi-report.sexp"
                                neomacs-hcss-multi-ledger)
      (let ((report (get-buffer-create "*Helm CSS SCSS Multi Report*")))
        (switch-to-buffer report)
        (delete-other-windows)
        (erase-buffer)
        (insert "HCSS-MULTI-CLEAN\n")
        (prin1 cleanup (current-buffer))
        (insert "\n")
        (goto-char (point-min))
        (special-mode)))))

(add-hook 'emacs-startup-hook #'neomacs-hcss-multi-setup 100)
"####;
