use std::fs;
use std::time::Duration;

use expect_test::expect;
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};

use crate::{CachedMelpaOracle, HELM_CSS_SCSS_MELPA_PIN};

use super::support::PackageTuiPair;

const HELM_CSS_SCSS_DEFAULT_TUI_PRELUDE: &str = r####"
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

const HELM_CSS_SCSS_SINGLE_TUI_PRELUDE: &str = r####"
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

const HELM_CSS_SCSS_MULTI_TUI_PRELUDE: &str = r####"
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

fn wait_for<F>(session: &mut TuiSession, label: &str, predicate: F)
where
    F: Fn(&[String]) -> bool + Copy,
{
    session.read_until(Duration::from_secs(20), predicate);
    assert!(
        predicate(&session.text_grid()),
        "{label} did not reach the expected terminal state:\n{}",
        session.text_grid().join("\n")
    );
}

fn send_default_probe(session: &mut TuiSession) {
    session.send_key("M-x");
    session.send(b"neomacs-hcss-default-run");
    session.send_key("RET");
}

fn semantic_rows(session: &TuiSession) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.contains("HCSS DEFAULT FAILURE") || row.contains("HCSS-DEFAULT-CLEAN")
        })
        .map(|(index, row)| format!("{index:02} |{}\n", row.trim_end()))
        .collect()
}

fn invoke(session: &mut TuiSession, command: &str) {
    session.send_key("M-x");
    session.send(command.as_bytes());
    session.send_key("RET");
}

fn helm_grid(session: &TuiSession) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .map(|(index, row)| format!("{index:02} |{}\n", row.trim_end()))
        .collect()
}

fn exact_grid_rows_from(session: &TuiSession, first_row: usize) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .skip(first_row)
        .map(|(index, row)| format!("{index:02} |{}\n", row.trim_end()))
        .collect()
}

fn exact_split_panes_from(session: &TuiSession, first_row: usize) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .skip(first_row)
        .map(|(index, row)| {
            let left = row.chars().take(79).collect::<String>();
            let right = row.chars().skip(80).collect::<String>();
            format!(
                "{index:02} L|{}\n{index:02} R|{}\n",
                left.trim_end(),
                right.trim_end()
            )
        })
        .collect()
}

fn send_single_observer(session: &mut TuiSession) {
    session.send_key("C-c");
    session.send(b"t");
}

fn helm_css_scss_unadapted_public_command_preserves_exact_helm_arity_failure() {
    let oracle = CachedMelpaOracle::new(HELM_CSS_SCSS_MELPA_PIN, "helm-css-scss.el")
        .expect("prepare exact revision-pinned helm-css-scss source")
        .with_prelude(HELM_CSS_SCSS_DEFAULT_TUI_PRELUDE);
    let mut pair =
        PackageTuiPair::spawn("helm-css-scss-default-failure", oracle.prepared_packages())
            .expect("spawn fresh helm-css-scss GNU/Neomacs PTY pair");

    wait_for(&mut pair.gnu, "GNU fixture startup", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
    });
    wait_for(&mut pair.neo, "Neomacs fixture startup", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
    });

    send_default_probe(&mut pair.gnu);
    send_default_probe(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU default failure cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-DEFAULT-CLEAN"))
    });
    wait_for(&mut pair.neo, "Neomacs default failure cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-DEFAULT-CLEAN"))
    });

    let gnu_report = fs::read_to_string(pair.gnu.home_dir().join("hcss-default-report.sexp"))
        .expect("read GNU default-failure report");
    let neo_report = fs::read_to_string(pair.neo.home_dir().join("hcss-default-report.sexp"))
        .expect("read Neomacs default-failure report");
    let expect = expect![[
        r#"(:post-public (:outcome (:error wrong-number-of-arguments :arity (1 . 1) :received 2) :buffer "tui-fixture.scss" :point 150 :line 11 :cache-count 4 :last-point (150 . "tui-fixture.scss") :last-query "" :fold-invisible neomacs-hcss-test-fold :recorded-invisible nil :package-overlay-buffer nil :session-advices (nil nil) :session-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*") :modified nil) :cleanup (:source-live nil :fold-buffer nil :package-overlay-buffer nil :session-advices (nil nil) :session-hook nil :helm-alive nil :helm-buffers nil :root-exists nil :cleanup-error nil))"#
    ]];
    expect.assert_eq(&gnu_report);
    assert_eq!(
        neo_report, gnu_report,
        "helm-css-scss default public failure diverged from GNU"
    );

    let gnu_rows = semantic_rows(&pair.gnu);
    let neo_rows = semantic_rows(&pair.neo);
    let rows_expect = expect![[r#"
        01 |HCSS DEFAULT FAILURE
        08 |HCSS-DEFAULT-CLEAN
    "#]];
    rows_expect.assert_eq(&gnu_rows);
    assert_eq!(neo_rows, gnu_rows, "default failure semantic rows differ");

    for (index, row) in pair.gnu.text_grid().iter().enumerate() {
        if row.contains("HCSS DEFAULT FAILURE") || row.contains("HCSS-DEFAULT-CLEAN") {
            let gnu = RawTerminalSnapshot::capture_rows(
                pair.gnu.screen(),
                index as u16..index as u16 + 1,
            );
            let neo = RawTerminalSnapshot::capture_rows(
                pair.neo.screen(),
                index as u16..index as u16 + 1,
            );
            assert_eq!(neo, gnu, "default failure raw row {index} differs");
        }
    }
}

fn helm_css_scss_named_display_adapter_drives_real_single_buffer_helm() {
    let oracle = CachedMelpaOracle::new(HELM_CSS_SCSS_MELPA_PIN, "helm-css-scss.el")
        .expect("prepare exact revision-pinned helm-css-scss source")
        .with_prelude(HELM_CSS_SCSS_SINGLE_TUI_PRELUDE);
    let mut pair = PackageTuiPair::spawn("helm-css-scss-single", oracle.prepared_packages())
        .expect("spawn fresh configured helm-css-scss GNU/Neomacs PTY pair");

    wait_for(&mut pair.gnu, "GNU single fixture startup", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
    });
    wait_for(&mut pair.neo, "Neomacs single fixture startup", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
    });

    invoke(&mut pair.gnu, "neomacs-hcss-single-start");
    invoke(&mut pair.neo, "neomacs-hcss-single-start");
    wait_for(&mut pair.gnu, "GNU real Helm candidates", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid.iter().any(|row| row.contains("Selector:"))
    });
    wait_for(&mut pair.neo, "Neomacs real Helm candidates", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid.iter().any(|row| row.contains("Selector:"))
    });
    let gnu_initial = helm_grid(&pair.gnu);
    let neo_initial = helm_grid(&pair.neo);
    assert_eq!(
        neo_initial, gnu_initial,
        "initial real single-buffer Helm grid differs"
    );

    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU initial Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-1"))
    });
    wait_for(&mut pair.neo, "Neomacs initial Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-1"))
    });
    pair.gnu.send_key("C-n");
    pair.neo.send_key("C-n");
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU next-row Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-2"))
    });
    wait_for(&mut pair.neo, "Neomacs next-row Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-2"))
    });
    pair.gnu.send(b"footer");
    pair.neo.send(b"footer");
    wait_for(&mut pair.gnu, "GNU live Helm filter", |grid| {
        grid.iter().any(|row| row.contains("Selector: footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
            && !grid.iter().any(|row| row.contains("10: .dashboard"))
    });
    wait_for(&mut pair.neo, "Neomacs live Helm filter", |grid| {
        grid.iter().any(|row| row.contains("Selector: footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
            && !grid.iter().any(|row| row.contains("10: .dashboard"))
    });
    let gnu_filtered = helm_grid(&pair.gnu);
    let neo_filtered = helm_grid(&pair.neo);
    assert_eq!(
        neo_filtered, gnu_filtered,
        "filtered real Helm grid differs"
    );
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU filtered Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-3"))
    });
    wait_for(&mut pair.neo, "Neomacs filtered Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-3"))
    });
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU cancellation returned", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs cancellation returned", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-cancel");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-cancel");
    wait_for(&mut pair.gnu, "GNU cancellation postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-POST-cancel"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs cancellation postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-SINGLE-POST-cancel"))
        },
    );
    pair.gnu.send_key("C-x");
    pair.gnu.send(b"1");
    pair.neo.send_key("C-x");
    pair.neo.send(b"1");
    wait_for(&mut pair.gnu, "GNU controlled source window", |grid| {
        !grid.iter().any(|row| row.contains("*scratch*"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs controlled source window", |grid| {
        !grid.iter().any(|row| row.contains("*scratch*"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });

    // A second public session must work immediately after cancellation.  RET
    // executes the package's real default open-brace action.
    invoke(&mut pair.gnu, "neomacs-hcss-single-start");
    invoke(&mut pair.neo, "neomacs-hcss-single-start");
    wait_for(&mut pair.gnu, "GNU recovery Helm session", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
    });
    wait_for(&mut pair.neo, "Neomacs recovery Helm session", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
    });
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU open action returned", |grid| {
        grid.iter().any(|row| row.contains(".card {"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs open action returned", |grid| {
        grid.iter().any(|row| row.contains(".card {"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-open");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-open");

    // Enter the real Helm action menu, choose its second public action with
    // navigation keys, and then exercise the public back-to-last-point toggle.
    invoke(&mut pair.gnu, "neomacs-hcss-single-start");
    invoke(&mut pair.neo, "neomacs-hcss-single-start");
    wait_for(&mut pair.gnu, "GNU close-action source session", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs close-action source session",
        |grid| grid.iter().any(|row| row.contains("[4 Candidate(s)]")),
    );
    pair.gnu.send_key("TAB");
    pair.neo.send_key("TAB");
    wait_for(&mut pair.gnu, "GNU Helm action menu", |grid| {
        grid.iter().any(|row| row.contains("Goto open brace"))
            && grid.iter().any(|row| row.contains("Goto close brace"))
    });
    wait_for(&mut pair.neo, "Neomacs Helm action menu", |grid| {
        grid.iter().any(|row| row.contains("Goto open brace"))
            && grid.iter().any(|row| row.contains("Goto close brace"))
    });
    let gnu_actions = helm_grid(&pair.gnu);
    let neo_actions = helm_grid(&pair.neo);
    assert_eq!(
        exact_split_panes_from(&pair.neo, 25),
        exact_split_panes_from(&pair.gnu, 25),
        "exact real Helm action panes differ:\nGNU:\n{gnu_actions}\nNeomacs:\n{neo_actions}"
    );
    pair.gnu.send_key("C-n");
    pair.neo.send_key("C-n");
    wait_for(&mut pair.gnu, "GNU second Helm action selected", |grid| {
        grid.iter().any(|row| row.contains("*helm action* L2"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs second Helm action selected",
        |grid| grid.iter().any(|row| row.contains("*helm action* L2")),
    );
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU close action returned", |grid| {
        grid.iter().any(|row| row.contains("color: black"))
            && !grid.iter().any(|row| row.contains("Select action:"))
            && !grid.iter().any(|row| row.contains("*helm action*"))
    });
    wait_for(&mut pair.neo, "Neomacs close action returned", |grid| {
        grid.iter().any(|row| row.contains("color: black"))
            && !grid.iter().any(|row| row.contains("Select action:"))
            && !grid.iter().any(|row| row.contains("*helm action*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-close");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-close");
    invoke(&mut pair.gnu, "neomacs-hcss-single-back-one");
    invoke(&mut pair.neo, "neomacs-hcss-single-back-one");
    invoke(&mut pair.gnu, "neomacs-hcss-single-back-two");
    invoke(&mut pair.neo, "neomacs-hcss-single-back-two");
    invoke(&mut pair.gnu, "neomacs-hcss-single-restore-test-fold");
    invoke(&mut pair.neo, "neomacs-hcss-single-restore-test-fold");

    // Real user editing invalidates the modified-buffer cache on the next
    // public session; a real C-x C-s then runs the package's after-save hook.
    pair.gnu.send_key("M->");
    pair.neo.send_key("M->");
    pair.gnu.send(b".new-release { color: teal; }\n");
    pair.neo.send(b".new-release { color: teal; }\n");
    invoke(&mut pair.gnu, "neomacs-hcss-single-start-new-release");
    invoke(&mut pair.neo, "neomacs-hcss-single-start-new-release");
    wait_for(&mut pair.gnu, "GNU unsaved selector rebuilt", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && grid.iter().any(|row| row.contains("Selector: new-release"))
    });
    wait_for(&mut pair.neo, "Neomacs unsaved selector rebuilt", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && grid.iter().any(|row| row.contains("Selector: new-release"))
    });
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU unsaved-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-4"))
    });
    wait_for(&mut pair.neo, "Neomacs unsaved-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-4"))
    });
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU unsaved-cache cancel", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs unsaved-cache cancel", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-unsaved");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-unsaved");
    pair.gnu.send_key("C-x");
    pair.gnu.send_key("C-s");
    pair.neo.send_key("C-x");
    pair.neo.send_key("C-s");
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-save");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-save");

    invoke(&mut pair.gnu, "neomacs-hcss-single-start-new-release");
    invoke(&mut pair.neo, "neomacs-hcss-single-start-new-release");
    wait_for(&mut pair.gnu, "GNU post-save selector rebuilt", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs post-save selector rebuilt",
        |grid| grid.iter().any(|row| row.contains(".new-release")),
    );
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU rebuilt-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-5"))
    });
    wait_for(&mut pair.neo, "Neomacs rebuilt-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-5"))
    });
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU rebuilt-cache action", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
    });
    wait_for(&mut pair.neo, "Neomacs rebuilt-cache action", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-rebuilt");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-rebuilt");

    // Exercise the actual public isearch handoff.  Only the documented public
    // display option is configured; literal quoting and regexp preservation
    // are performed by `helm-css-scss-from-isearch' itself.
    wait_for(&mut pair.gnu, "GNU rebuilt action postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-POST-saved-cache-rebuilt-action"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs rebuilt action postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-SINGLE-POST-saved-cache-rebuilt-action"))
        },
    );
    pair.gnu.send_key("C-x");
    pair.gnu.send(b"1");
    pair.neo.send_key("C-x");
    pair.neo.send(b"1");
    invoke(
        &mut pair.gnu,
        "neomacs-hcss-single-configure-public-display",
    );
    invoke(
        &mut pair.neo,
        "neomacs-hcss-single-configure-public-display",
    );
    wait_for(&mut pair.gnu, "GNU public display configured", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-PUBLIC-DISPLAY-CONFIGURED"))
    });
    wait_for(&mut pair.neo, "Neomacs public display configured", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-PUBLIC-DISPLAY-CONFIGURED"))
    });

    pair.gnu.send_key("M-<");
    pair.neo.send_key("M-<");
    pair.gnu.send_key("C-s");
    pair.neo.send_key("C-s");
    pair.gnu.send(b".footer");
    pair.neo.send(b".footer");
    wait_for(&mut pair.gnu, "GNU literal isearch", |grid| {
        grid.iter().any(|row| row.contains("I-search: .footer"))
    });
    wait_for(&mut pair.neo, "Neomacs literal isearch", |grid| {
        grid.iter().any(|row| row.contains("I-search: .footer"))
    });
    invoke(&mut pair.gnu, "helm-css-scss-from-isearch");
    invoke(&mut pair.neo, "helm-css-scss-from-isearch");
    wait_for(&mut pair.gnu, "GNU literal isearch Helm handoff", |grid| {
        grid.iter().any(|row| row.contains("Selector: \\.footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs literal isearch Helm handoff",
        |grid| {
            grid.iter().any(|row| row.contains("Selector: \\.footer"))
                && grid.iter().any(|row| row.contains("20: .footer"))
        },
    );
    let gnu_isearch_literal = helm_grid(&pair.gnu);
    let neo_isearch_literal = helm_grid(&pair.neo);
    assert_eq!(
        exact_grid_rows_from(&pair.neo, 25),
        exact_grid_rows_from(&pair.gnu, 25),
        "literal isearch exact Helm pane differs:\nGNU:\n{gnu_isearch_literal}\nNeomacs:\n{neo_isearch_literal}"
    );
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU literal isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs literal isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-isearch-literal");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-isearch-literal");
    wait_for(&mut pair.gnu, "GNU literal isearch postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-POST-isearch-literal-cancel"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs literal isearch postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-SINGLE-POST-isearch-literal-cancel"))
        },
    );
    pair.gnu.send_key("C-x");
    pair.gnu.send(b"1");
    pair.neo.send_key("C-x");
    pair.neo.send(b"1");

    pair.gnu.send_key("M-<");
    pair.neo.send_key("M-<");
    pair.gnu.send_key("C-M-s");
    pair.neo.send_key("C-M-s");
    pair.gnu.send(b".footer");
    pair.neo.send(b".footer");
    wait_for(&mut pair.gnu, "GNU regexp isearch", |grid| {
        grid.iter()
            .any(|row| row.contains("Regexp I-search: .footer"))
    });
    wait_for(&mut pair.neo, "Neomacs regexp isearch", |grid| {
        grid.iter()
            .any(|row| row.contains("Regexp I-search: .footer"))
    });
    invoke(&mut pair.gnu, "helm-css-scss-from-isearch");
    invoke(&mut pair.neo, "helm-css-scss-from-isearch");
    wait_for(&mut pair.gnu, "GNU regexp isearch Helm handoff", |grid| {
        grid.iter().any(|row| row.contains("Selector: .footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs regexp isearch Helm handoff",
        |grid| {
            grid.iter().any(|row| row.contains("Selector: .footer"))
                && grid.iter().any(|row| row.contains("20: .footer"))
        },
    );
    let gnu_isearch_regexp = helm_grid(&pair.gnu);
    let neo_isearch_regexp = helm_grid(&pair.neo);
    assert_eq!(
        exact_grid_rows_from(&pair.neo, 25),
        exact_grid_rows_from(&pair.gnu, 25),
        "regexp isearch exact Helm pane differs:\nGNU:\n{gnu_isearch_regexp}\nNeomacs:\n{neo_isearch_regexp}"
    );
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU regexp isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs regexp isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-isearch-regexp");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-isearch-regexp");

    invoke(&mut pair.gnu, "neomacs-hcss-single-finish");
    invoke(&mut pair.neo, "neomacs-hcss-single-finish");
    wait_for(&mut pair.gnu, "GNU configured single cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-SINGLE-CLEAN"))
    });
    wait_for(&mut pair.neo, "Neomacs configured single cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-SINGLE-CLEAN"))
    });

    let gnu_report = fs::read_to_string(pair.gnu.home_dir().join("hcss-single-report.sexp"))
        .expect("read GNU configured-single report");
    let neo_report = fs::read_to_string(pair.neo.home_dir().join("hcss-single-report.sexp"))
        .expect("read Neomacs configured-single report");
    let report_expect = expect![[r#"
        ((:stage active-1 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "" :source-name "tui-fixture.scss" :helm-buffer-text "tui-fixture.scss
        6: .dashboard, .dashboard--compact
        10: .dashboard, .dashboard--compact .card
        13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        20: .footer
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 18 font-lock-function-name-face "6") (20 51 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (52 54 font-lock-function-name-face "10") (56 87 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (88 93 helm-css-scss-selector-depth-face-2 ".card") (94 96 font-lock-function-name-face "13") (98 129 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (130 135 helm-css-scss-selector-depth-face-2 ".card") (136 157 helm-css-scss-selector-depth-face-3 "&__title, &__subtitle") (158 160 font-lock-function-name-face "20") (162 169 helm-css-scss-selector-depth-face-1 ".footer")) :selected "10: .dashboard, .dashboard--compact .card" :selected-face-runs ((0 2 font-lock-function-name-face "10") (4 35 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (36 41 helm-css-scss-selector-depth-face-2 ".card")) :selected-real (148 230 2 140 148 10) :source ("tui-fixture.scss" 148 10 8 32 123 "  .card {") :overlay ("tui-fixture.scss" 140 148 "  .card " helm-css-scss-target-line-face) :last-line ("tui-fixture.scss" 10) :fold-invisible nil :unveiled ((140 150 neomacs-hcss-single-fold)) :advices (t t) :update-hook t :windows ("tui-fixture.scss" "*Helm Css SCSS*") :cache-count 4) (:stage active-2 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "" :source-name "tui-fixture.scss" :helm-buffer-text "tui-fixture.scss
        6: .dashboard, .dashboard--compact
        10: .dashboard, .dashboard--compact .card
        13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        20: .footer
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 18 font-lock-function-name-face "6") (20 51 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (52 54 font-lock-function-name-face "10") (56 87 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (88 93 helm-css-scss-selector-depth-face-2 ".card") (94 96 font-lock-function-name-face "13") (98 129 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (130 135 helm-css-scss-selector-depth-face-2 ".card") (136 157 helm-css-scss-selector-depth-face-3 "&__title, &__subtitle") (158 160 font-lock-function-name-face "20") (162 169 helm-css-scss-selector-depth-face-1 ".footer")) :selected "13: .dashboard, .dashboard--compact .card &__title, &__subtitle" :selected-face-runs ((0 2 font-lock-function-name-face "13") (4 35 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (36 41 helm-css-scss-selector-depth-face-2 ".card") (42 63 helm-css-scss-selector-depth-face-3 "&__title, &__subtitle")) :selected-real (200 226 3 170 200 13) :source ("tui-fixture.scss" 200 14 16 32 123 "    &__subtitle {") :overlay ("tui-fixture.scss" 170 200 "    &__title,
            &__subtitle " helm-css-scss-target-line-face) :last-line ("tui-fixture.scss" 13) :fold-invisible neomacs-hcss-single-fold :unveiled nil :advices (t t) :update-hook t :windows ("tui-fixture.scss" "*Helm Css SCSS*") :cache-count 4) (:stage active-3 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "footer" :source-name "tui-fixture.scss" :helm-buffer-text "tui-fixture.scss
        20: .footer
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 19 font-lock-function-name-face "20") (21 28 helm-css-scss-selector-depth-face-1 ".footer")) :selected "20: .footer" :selected-face-runs ((0 2 font-lock-function-name-face "20") (4 11 helm-css-scss-selector-depth-face-1 ".footer")) :selected-real (242 261 1 234 242 20) :source ("tui-fixture.scss" 242 20 8 32 123 ".footer {") :overlay ("tui-fixture.scss" 234 242 ".footer " helm-css-scss-target-line-face) :last-line ("tui-fixture.scss" 20) :fold-invisible neomacs-hcss-single-fold :unveiled nil :advices (t t) :update-hook t :windows ("tui-fixture.scss" "*Helm Css SCSS*") :cache-count 4) (:stage cancel :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 150 11 0 10 32 nil 4 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        " :last-point (150 "tui-fixture.scss") :fold-invisible neomacs-hcss-single-fold :unveiled nil :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage open-action :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 148 10 8 32 123 nil 4 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        " :last-point (150 "tui-fixture.scss") :fold-invisible nil :unveiled ((140 150 neomacs-hcss-single-fold)) :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage close-action :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 230 17 3 125 10 nil 4 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        " :last-point (148 "tui-fixture.scss") :fold-invisible nil :unveiled ((140 150 neomacs-hcss-single-fold)) :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage back-one :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 148 10 8 32 123 nil 4 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        " :last-point (230 "tui-fixture.scss") :fold-invisible nil :unveiled ((140 150 neomacs-hcss-single-fold)) :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage back-two :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 230 17 3 125 10 nil 4 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        " :last-point (148 "tui-fixture.scss") :fold-invisible nil :unveiled ((140 150 neomacs-hcss-single-fold)) :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage active-4 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "new-release" :source-name "tui-fixture.scss" :helm-buffer-text "tui-fixture.scss
        23: .new-release
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 19 font-lock-function-name-face "23") (21 33 helm-css-scss-selector-depth-face-1 ".new-release")) :selected "23: .new-release" :selected-face-runs ((0 2 font-lock-function-name-face "23") (4 16 helm-css-scss-selector-depth-face-1 ".new-release")) :selected-real (275 291 1 262 275 23) :source ("tui-fixture.scss" 275 23 13 32 123 ".new-release { color: teal; }") :overlay ("tui-fixture.scss" 262 275 ".new-release " helm-css-scss-target-line-face) :last-line ("tui-fixture.scss" 23) :fold-invisible neomacs-hcss-single-fold :unveiled nil :advices (t t) :update-hook t :windows ("tui-fixture.scss" "*Helm Css SCSS*") :cache-count 5) (:stage unsaved-cache :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 292 24 0 10 nil t 5 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        " :last-point (292 "tui-fixture.scss") :fold-invisible neomacs-hcss-single-fold :unveiled nil :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage saved-cache-cleared :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 292 24 0 10 nil nil 0 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        .new-release { color: teal; }
        " :last-point (292 "tui-fixture.scss") :fold-invisible neomacs-hcss-single-fold :unveiled nil :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage active-5 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "new-release" :source-name "tui-fixture.scss" :helm-buffer-text "tui-fixture.scss
        23: .new-release
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 19 font-lock-function-name-face "23") (21 33 helm-css-scss-selector-depth-face-1 ".new-release")) :selected "23: .new-release" :selected-face-runs ((0 2 font-lock-function-name-face "23") (4 16 helm-css-scss-selector-depth-face-1 ".new-release")) :selected-real (275 291 1 262 275 23) :source ("tui-fixture.scss" 275 23 13 32 123 ".new-release { color: teal; }") :overlay ("tui-fixture.scss" 262 275 ".new-release " helm-css-scss-target-line-face) :last-line ("tui-fixture.scss" 23) :fold-invisible neomacs-hcss-single-fold :unveiled nil :advices (t t) :update-hook t :windows ("tui-fixture.scss" "*Helm Css SCSS*") :cache-count 5) (:stage saved-cache-rebuilt-action :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 275 23 13 32 123 nil 5 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        .new-release { color: teal; }
        " :last-point (292 "tui-fixture.scss") :fold-invisible neomacs-hcss-single-fold :unveiled nil :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage isearch-literal-cancel :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 241 20 7 114 32 nil 5 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        .new-release { color: teal; }
        " :last-point (241 "tui-fixture.scss") :fold-invisible neomacs-hcss-single-fold :unveiled nil :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:stage isearch-regexp-cancel :current "tui-fixture.scss" :windows ("tui-fixture.scss") :source ("tui-fixture.scss" 241 20 7 114 32 nil 5 "") :disk-bytes "/* A disabled prototype is intentionally excluded. */
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
        .new-release { color: teal; }
        " :last-point (241 "tui-fixture.scss") :fold-invisible neomacs-hcss-single-fold :unveiled nil :package-overlay-buffer nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*")) (:cleanup (:source-live nil :root-exists nil :fold-buffer nil :overlay-buffer nil :unveiled nil :advices (nil nil) :update-hook nil :helm-alive nil :helm-buffers nil :display-restored t :cleanup-error nil)))"#]];
    report_expect.assert_eq(&gnu_report);
    assert_eq!(neo_report, gnu_report, "configured single report differs");

    let initial_expect = expect![[r#"
        00 |File Edit Options Buffers Tools Minibuf Help
        01 |/* A disabled prototype is intentionally excluded. */
        02 |/* .disabled {
        03 |  color: gray;
        04 |} */
        05 |
        06 |.dashboard,
        07 |.dashboard--compact {
        08 |  color: red;
        09 |
        10 |  .card {
        11 |    padding: 1rem;
        12 |
        13 |    &__title,
        14 |    &__subtitle {
        15 |      color: blue;
        16 |    }
        17 |  }
        18 |}
        19 |
        20 |.footer {
        21 |  color: black;
        22 |}
        23 |
        24 |-UU-:--- F1  tui-fixture.scss   All   L10    (SCSS ElDoc) ------------------------------------------------------------------------------------------------------
        25 | helm-css-scss
        26 |tui-fixture.scss
        27 |6: .dashboard, .dashboard--compact
        28 |10: .dashboard, .dashboard--compact .card
        29 |13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        30 |20: .footer
        31 |
        32 |
        33 |
        34 |
        35 |
        36 |
        37 |
        38 |
        39 |
        40 |
        41 |
        42 |
        43 |
        44 |
        45 |
        46 |
        47 |
        48 | *Helm Css SCSS* L2    [4 Candidate(s)]   C-h m:Help TAB:Act C-o:NextSrc RET/f1..f12:NthAct C-!:Tog.suspend C-h c:Conf
        49 |Selector:
    "#]];
    initial_expect.assert_eq(&gnu_initial);
    let filtered_expect = expect![[r#"
        00 |File Edit Options Buffers Tools Minibuf Help
        01 |.dashboard--compact {
        02 |  color: red;
        03 |
        04 |    padding: 1rem;
        05 |
        06 |    &__title,
        07 |    &__subtitle {
        08 |      color: blue;
        09 |    }
        10 |  }
        11 |}
        12 |
        13 |.footer {
        14 |  color: black;
        15 |}
        16 |
        17 |
        18 |
        19 |
        20 |
        21 |
        22 |
        23 |
        24 |-UU-:--- F1  tui-fixture.scss   Bot   L20    (SCSS ElDoc) ------------------------------------------------------------------------------------------------------
        25 | helm-css-scss
        26 |tui-fixture.scss
        27 |20: .footer
        28 |
        29 |
        30 |
        31 |
        32 |
        33 |
        34 |
        35 |
        36 |
        37 |
        38 |
        39 |
        40 |
        41 |
        42 |
        43 |
        44 |
        45 |
        46 |
        47 |
        48 | *Helm Css SCSS* L1    [1 Candidate(s)]   C-h m:Help TAB:Act C-o:NextSrc RET/f1..f12:NthAct C-!:Tog.suspend C-h c:Conf
        49 |Selector: footer
    "#]];
    filtered_expect.assert_eq(&gnu_filtered);
    let action_expect = expect![[r#"
        00 |File Edit Options Buffers Tools Minibuf Help
        01 |/* A disabled prototype is intentionally excluded. */
        02 |/* .disabled {
        03 |  color: gray;
        04 |} */
        05 |
        06 |.dashboard,
        07 |.dashboard--compact {
        08 |  color: red;
        09 |
        10 |  .card {
        11 |    padding: 1rem;
        12 |
        13 |    &__title,
        14 |    &__subtitle {
        15 |      color: blue;
        16 |    }
        17 |  }
        18 |}
        19 |
        20 |.footer {
        21 |  color: black;
        22 |}
        23 |
        24 |-UU-:--- F1  tui-fixture.scss   All   L10    (SCSS ElDoc) ------------------------------------------------------------------------------------------------------
        25 | C-j: DoNothing (keeping session)                                              | helm-css-scss
        26 |Actions                                                                        |tui-fixture.scss
        27 |[f1]  Goto open brace                                                          |6: .dashboard, .dashboard--compact
        28 |[f2]  Goto close brace                                                         |10: .dashboard, .dashboard--compact .card
        29 |                                                                               |13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        30 |                                                                               |20: .footer
        31 |                                                                               |
        32 |                                                                               |
        33 |                                                                               |
        34 |                                                                               |
        35 |                                                                               |
        36 |                                                                               |
        37 |                                                                               |
        38 |                                                                               |
        39 |                                                                               |
        40 |                                                                               |
        41 |                                                                               |
        42 |                                                                               |
        43 |                                                                               |
        44 |                                                                               |
        45 |                                                                               |
        46 |                                                                               |
        47 |                                                                               |
        48 | *helm action* L1    [2 Action(s)]   TAB:BackToCands RET/f1/f2/fn:NthAct       | *Helm Css SCSS* L1    [2 Action(s)]   TAB:BackToCands RET/f1/f2/fn:NthAct
        49 |Select action:
    "#]];
    action_expect.assert_eq(&gnu_actions);
    let literal_isearch_expect = expect![[r#"
        00 |File Edit Options Buffers Tools Minibuf Help
        01 |.dashboard--compact {
        02 |  color: red;
        03 |
        04 |    padding: 1rem;
        05 |
        06 |    &__title,
        07 |    &__subtitle {
        08 |      color: blue;
        09 |    }
        10 |  }
        11 |}
        12 |
        13 |.footer {
        14 |  color: black;
        15 |}
        16 |.new-release { color: teal; }
        17 |
        18 |
        19 |
        20 |
        21 |
        22 |
        23 |
        24 |-UU-:--- F1  tui-fixture.scss   Bot   L20    (SCSS ElDoc) ------------------------------------------------------------------------------------------------------
        25 | helm-css-scss
        26 |tui-fixture.scss
        27 |20: .footer
        28 |
        29 |
        30 |
        31 |
        32 |
        33 |
        34 |
        35 |
        36 |
        37 |
        38 |
        39 |
        40 |
        41 |
        42 |
        43 |
        44 |
        45 |
        46 |
        47 |
        48 | *Helm Css SCSS* L1    [1 Candidate(s)]   C-h m:Help TAB:Act C-o:NextSrc RET/f1..f12:NthAct C-!:Tog.suspend C-h c:Conf
        49 |Selector: \.footer
    "#]];
    literal_isearch_expect.assert_eq(&gnu_isearch_literal);
    let regexp_isearch_expect = expect![[r#"
        00 |File Edit Options Buffers Tools Minibuf Help
        01 |.dashboard--compact {
        02 |  color: red;
        03 |
        04 |    padding: 1rem;
        05 |
        06 |    &__title,
        07 |    &__subtitle {
        08 |      color: blue;
        09 |    }
        10 |  }
        11 |}
        12 |
        13 |.footer {
        14 |  color: black;
        15 |}
        16 |.new-release { color: teal; }
        17 |
        18 |
        19 |
        20 |
        21 |
        22 |
        23 |
        24 |-UU-:--- F1  tui-fixture.scss   Bot   L20    (SCSS ElDoc) ------------------------------------------------------------------------------------------------------
        25 | helm-css-scss
        26 |tui-fixture.scss
        27 |20: .footer
        28 |
        29 |
        30 |
        31 |
        32 |
        33 |
        34 |
        35 |
        36 |
        37 |
        38 |
        39 |
        40 |
        41 |
        42 |
        43 |
        44 |
        45 |
        46 |
        47 |
        48 | *Helm Css SCSS* L1    [1 Candidate(s)]   C-h m:Help TAB:Act C-o:NextSrc RET/f1..f12:NthAct C-!:Tog.suspend C-h c:Conf
        49 |Selector: .footer
    "#]];
    regexp_isearch_expect.assert_eq(&gnu_isearch_regexp);
}

fn helm_css_scss_named_display_adapter_drives_real_multi_buffer_helm() {
    let oracle = CachedMelpaOracle::new(HELM_CSS_SCSS_MELPA_PIN, "helm-css-scss.el")
        .expect("prepare exact revision-pinned helm-css-scss source")
        .with_prelude(HELM_CSS_SCSS_MULTI_TUI_PRELUDE);
    let mut pair = PackageTuiPair::spawn("helm-css-scss-multi", oracle.prepared_packages())
        .expect("spawn fresh configured multi-buffer GNU/Neomacs PTY pair");

    wait_for(&mut pair.gnu, "GNU multi fixture startup", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
    });
    wait_for(&mut pair.neo, "Neomacs multi fixture startup", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-multi-start");
    invoke(&mut pair.neo, "neomacs-hcss-multi-start");
    wait_for(&mut pair.gnu, "GNU real multi-source Helm", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    wait_for(&mut pair.neo, "Neomacs real multi-source Helm", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    let gnu_initial = helm_grid(&pair.gnu);
    let neo_initial = helm_grid(&pair.neo);
    assert_eq!(
        neo_initial, gnu_initial,
        "full initial multi-source Helm grid differs"
    );

    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU initial multi observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-1"))
    });
    wait_for(&mut pair.neo, "Neomacs initial multi observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-1"))
    });
    pair.gnu.send_key("C-o");
    pair.neo.send_key("C-o");
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU component source observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-2"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs component source observation",
        |grid| grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-2")),
    );

    pair.gnu.send_key("C-n");
    pair.neo.send_key("C-n");
    wait_for(&mut pair.gnu, "GNU exact multi preview error", |grid| {
        grid.iter()
            .any(|row| row.contains("‘recenter’ing a window that does not display current-buffer"))
    });
    wait_for(&mut pair.neo, "Neomacs exact multi preview error", |grid| {
        grid.iter()
            .any(|row| row.contains("‘recenter’ing a window that does not display current-buffer"))
    });
    let gnu_error_grid = helm_grid(&pair.gnu);
    let neo_error_grid = helm_grid(&pair.neo);
    assert_eq!(
        neo_error_grid, gnu_error_grid,
        "full multi-preview error grid differs"
    );
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU partial preview observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-3"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs partial preview observation",
        |grid| grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-3")),
    );
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU multi action returned", |grid| {
        grid.iter().any(|row| row.contains(".button:hover"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    wait_for(&mut pair.neo, "Neomacs multi action returned", |grid| {
        grid.iter().any(|row| row.contains(".button:hover"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-multi-post-action");
    invoke(&mut pair.neo, "neomacs-hcss-multi-post-action");
    wait_for(&mut pair.gnu, "GNU multi action postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-MULTI-POST-ACTION"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs multi action postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-MULTI-POST-ACTION"))
        },
    );
    invoke(&mut pair.gnu, "neomacs-hcss-multi-finish");
    invoke(&mut pair.neo, "neomacs-hcss-multi-finish");
    wait_for(&mut pair.gnu, "GNU multi cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-CLEAN"))
    });
    wait_for(&mut pair.neo, "Neomacs multi cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-CLEAN"))
    });

    let gnu_report = fs::read_to_string(pair.gnu.home_dir().join("hcss-multi-report.sexp"))
        .expect("read GNU configured-multi report");
    let neo_report = fs::read_to_string(pair.neo.home_dir().join("hcss-multi-report.sexp"))
        .expect("read Neomacs configured-multi report");
    let report_expect = expect![[r#"
        ((:stage active-1 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "" :helm-buffer-text "tui-fixture.scss
        6: .dashboard, .dashboard--compact
        10: .dashboard, .dashboard--compact .card
        13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        20: .footer

        component.css
        1: .button
        5: .button:hover

        theme.less
        1: .theme
        4: .theme .link

        IGNORED.CSS
        1: .upper-case-extension
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 18 font-lock-function-name-face "6") (20 51 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (52 54 font-lock-function-name-face "10") (56 87 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (88 93 helm-css-scss-selector-depth-face-2 ".card") (94 96 font-lock-function-name-face "13") (98 129 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (130 135 helm-css-scss-selector-depth-face-2 ".card") (136 157 helm-css-scss-selector-depth-face-3 "&__title, &__subtitle") (158 160 font-lock-function-name-face "20") (162 169 helm-css-scss-selector-depth-face-1 ".footer") (170 171 helm-eob-line "
        ") (171 185 helm-source-header "component.css
        ") (185 186 font-lock-function-name-face "1") (188 195 helm-css-scss-selector-depth-face-1 ".button") (196 197 font-lock-function-name-face "5") (199 212 helm-css-scss-selector-depth-face-1 ".button:hover") (213 214 helm-eob-line "
        ") (214 225 helm-source-header "theme.less
        ") (225 226 font-lock-function-name-face "1") (228 234 helm-css-scss-selector-depth-face-1 ".theme") (235 236 font-lock-function-name-face "4") (238 244 helm-css-scss-selector-depth-face-1 ".theme") (245 250 helm-css-scss-selector-depth-face-2 ".link") (251 252 helm-eob-line "
        ") (252 264 helm-source-header "IGNORED.CSS
        ") (264 265 font-lock-function-name-face "1") (267 288 helm-css-scss-selector-depth-face-1 ".upper-case-extension")) :selected "10: .dashboard, .dashboard--compact .card" :selected-runs ((0 2 font-lock-function-name-face "10") (4 35 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (36 41 helm-css-scss-selector-depth-face-2 ".card")) :selected-real (148 230 2 140 148 10) :current-source "tui-fixture.scss" :target ("tui-fixture.scss" 150 11 0 10 32 "    padding: 1rem;") :overlay ("tui-fixture.scss" 150 150 "") :buffer-points (("tui-fixture.scss" 150 11) ("component.css" 1 1) ("theme.less" 1 1) ("IGNORED.CSS" 1 1)) :fileless-present nil :advices (t t t t) :helm-windows ("tui-fixture.scss" "*Helm Css SCSS multi buffers*")) (:stage active-2 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "" :helm-buffer-text "tui-fixture.scss
        6: .dashboard, .dashboard--compact
        10: .dashboard, .dashboard--compact .card
        13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        20: .footer

        component.css
        1: .button
        5: .button:hover

        theme.less
        1: .theme
        4: .theme .link

        IGNORED.CSS
        1: .upper-case-extension
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 18 font-lock-function-name-face "6") (20 51 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (52 54 font-lock-function-name-face "10") (56 87 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (88 93 helm-css-scss-selector-depth-face-2 ".card") (94 96 font-lock-function-name-face "13") (98 129 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (130 135 helm-css-scss-selector-depth-face-2 ".card") (136 157 helm-css-scss-selector-depth-face-3 "&__title, &__subtitle") (158 160 font-lock-function-name-face "20") (162 169 helm-css-scss-selector-depth-face-1 ".footer") (170 171 helm-eob-line "
        ") (171 185 helm-source-header "component.css
        ") (185 186 font-lock-function-name-face "1") (188 195 helm-css-scss-selector-depth-face-1 ".button") (196 197 font-lock-function-name-face "5") (199 212 helm-css-scss-selector-depth-face-1 ".button:hover") (213 214 helm-eob-line "
        ") (214 225 helm-source-header "theme.less
        ") (225 226 font-lock-function-name-face "1") (228 234 helm-css-scss-selector-depth-face-1 ".theme") (235 236 font-lock-function-name-face "4") (238 244 helm-css-scss-selector-depth-face-1 ".theme") (245 250 helm-css-scss-selector-depth-face-2 ".link") (251 252 helm-eob-line "
        ") (252 264 helm-source-header "IGNORED.CSS
        ") (264 265 font-lock-function-name-face "1") (267 288 helm-css-scss-selector-depth-face-1 ".upper-case-extension")) :selected "1: .button" :selected-runs ((0 1 font-lock-function-name-face "1") (3 10 helm-css-scss-selector-depth-face-1 ".button")) :selected-real (9 36 1 1 9 1) :current-source "component.css" :target ("component.css" 1 1 0 nil 46 ".button {") :overlay ("tui-fixture.scss" 150 150 "") :buffer-points (("tui-fixture.scss" 150 11) ("component.css" 1 1) ("theme.less" 1 1) ("IGNORED.CSS" 1 1)) :fileless-present nil :advices (t t t t) :helm-windows ("tui-fixture.scss" "*Helm Css SCSS multi buffers*")) (:stage active-3 :alive t :prompt #("Selector: " 0 10 (face helm-minibuffer-prompt)) :pattern "" :helm-buffer-text "tui-fixture.scss
        6: .dashboard, .dashboard--compact
        10: .dashboard, .dashboard--compact .card
        13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        20: .footer

        component.css
        1: .button
        5: .button:hover

        theme.less
        1: .theme
        4: .theme .link

        IGNORED.CSS
        1: .upper-case-extension
        " :helm-face-runs ((0 17 helm-source-header "tui-fixture.scss
        ") (17 18 font-lock-function-name-face "6") (20 51 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (52 54 font-lock-function-name-face "10") (56 87 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (88 93 helm-css-scss-selector-depth-face-2 ".card") (94 96 font-lock-function-name-face "13") (98 129 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (130 135 helm-css-scss-selector-depth-face-2 ".card") (136 157 helm-css-scss-selector-depth-face-3 "&__title, &__subtitle") (158 160 font-lock-function-name-face "20") (162 169 helm-css-scss-selector-depth-face-1 ".footer") (170 171 helm-eob-line "
        ") (171 185 helm-source-header "component.css
        ") (185 186 font-lock-function-name-face "1") (188 195 helm-css-scss-selector-depth-face-1 ".button") (196 197 font-lock-function-name-face "5") (199 212 helm-css-scss-selector-depth-face-1 ".button:hover") (213 214 helm-eob-line "
        ") (214 225 helm-source-header "theme.less
        ") (225 226 font-lock-function-name-face "1") (228 234 helm-css-scss-selector-depth-face-1 ".theme") (235 236 font-lock-function-name-face "4") (238 244 helm-css-scss-selector-depth-face-1 ".theme") (245 250 helm-css-scss-selector-depth-face-2 ".link") (251 252 helm-eob-line "
        ") (252 264 helm-source-header "IGNORED.CSS
        ") (264 265 font-lock-function-name-face "1") (267 288 helm-css-scss-selector-depth-face-1 ".upper-case-extension")) :selected "5: .button:hover" :selected-runs ((0 1 font-lock-function-name-face "5") (3 16 helm-css-scss-selector-depth-face-1 ".button:hover")) :selected-real (52 79 1 38 52 5) :current-source "component.css" :target ("component.css" 52 5 14 32 123 ".button:hover {") :overlay ("component.css" 38 52 ".button:hover ") :buffer-points (("tui-fixture.scss" 150 11) ("component.css" 52 5) ("theme.less" 1 1) ("IGNORED.CSS" 1 1)) :fileless-present nil :advices (t t t t) :helm-windows ("component.css" "*Helm Css SCSS multi buffers*")) (:stage post-action :selected-buffer "component.css" :point 52 :line 5 :column 14 :char-before 32 :char-after 123 :buffer-points (("tui-fixture.scss" 150 11) ("component.css" 52 5) ("theme.less" 1 1) ("IGNORED.CSS" 1 1)) :overlay-buffer nil :advices (nil nil nil nil) :helm-alive nil :helm-buffers ("*Helm Css SCSS multi buffers*")) (:cleanup (:owned-live nil :root-exists nil :overlay-buffer nil :advices (nil nil nil nil) :helm-alive nil :helm-buffers nil :cleanup-error nil)))"#]];
    report_expect.assert_eq(&gnu_report);
    assert_eq!(neo_report, gnu_report, "configured multi report differs");
    let initial_expect = expect![[r#"
        00 |File Edit Options Buffers Tools Minibuf Help
        01 |/* A disabled prototype is intentionally excluded. */
        02 |/* .disabled {
        03 |  color: gray;
        04 |} */
        05 |
        06 |.dashboard,
        07 |.dashboard--compact {
        08 |  color: red;
        09 |
        10 |  .card {
        11 |    padding: 1rem;
        12 |
        13 |    &__title,
        14 |    &__subtitle {
        15 |      color: blue;
        16 |    }
        17 |  }
        18 |}
        19 |
        20 |.footer {
        21 |  color: black;
        22 |}
        23 |
        24 |-UU-:--- F1  tui-fixture.scss   All   L11    (SCSS ElDoc) ------------------------------------------------------------------------------------------------------
        25 | helm-css-scss-multi
        26 |tui-fixture.scss
        27 |6: .dashboard, .dashboard--compact
        28 |10: .dashboard, .dashboard--compact .card
        29 |13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        30 |20: .footer
        31 |
        32 |component.css
        33 |1: .button
        34 |5: .button:hover
        35 |
        36 |theme.less
        37 |1: .theme
        38 |4: .theme .link
        39 |
        40 |IGNORED.CSS
        41 |1: .upper-case-extension
        42 |
        43 |
        44 |
        45 |
        46 |
        47 |
        48 | *Helm Css SCSS multi buffers* L2    [4 Candidate(s)]   C-h m:Help TAB:Act C-o:NextSrc RET/f1..f12:NthAct C-!:Tog.suspend C-h c:Conf
        49 |Selector:
    "#]];
    initial_expect.assert_eq(&gnu_initial);
    let error_expect = expect![[r#"
        00 |File Edit Options Buffers Tools Minibuf Help
        01 |.button {
        02 |  display: inline-flex;
        03 |}
        04 |
        05 |.button:hover {
        06 |  color: rebeccapurple;
        07 |}
        08 |
        09 |
        10 |
        11 |
        12 |
        13 |
        14 |
        15 |
        16 |
        17 |
        18 |
        19 |
        20 |
        21 |
        22 |
        23 |
        24 |-UU-:--- F1  component.css   All   L5     (CSS ElDoc) ----------------------------------------------------------------------------------------------------------
        25 | helm-css-scss-multi
        26 |tui-fixture.scss
        27 |6: .dashboard, .dashboard--compact
        28 |10: .dashboard, .dashboard--compact .card
        29 |13: .dashboard, .dashboard--compact .card &__title, &__subtitle
        30 |20: .footer
        31 |
        32 |component.css
        33 |1: .button
        34 |5: .button:hover
        35 |
        36 |theme.less
        37 |1: .theme
        38 |4: .theme .link
        39 |
        40 |IGNORED.CSS
        41 |1: .upper-case-extension
        42 |
        43 |
        44 |
        45 |
        46 |
        47 |
        48 | *Helm Css SCSS multi buffers* L2    [2 Candidate(s)]   C-h m:Help TAB:Act C-o:NextSrc RET/f1..f12:NthAct C-!:Tog.suspend C-h c:Conf
        49 |Selector:  [‘recenter’ing a window that does not display current-buffer]
    "#]];
    error_expect.assert_eq(&gnu_error_grid);
}

#[test]
fn helm_css_scss_public_tui_workflows_match_gnu() {
    helm_css_scss_unadapted_public_command_preserves_exact_helm_arity_failure();
    helm_css_scss_named_display_adapter_drives_real_single_buffer_helm();
    helm_css_scss_named_display_adapter_drives_real_multi_buffer_helm();
}
