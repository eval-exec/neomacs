//! Gruvbox's two real terminal palette branches and rendered editing surfaces.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use expect_test::{Expect, expect};
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};

use crate::{
    COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, GRUVBOX_THEME_MELPA_PIN, ORDERLESS_MELPA_PIN,
    PreparedPackageSet,
};

use super::support::{DisplayEnvOverride, PackageTuiPair};

const GRUVBOX_TUI_PRELUDE: &str = r####"
(require 'ansi-color)
(require 'cl-lib)
(require 'diff-mode)
(require 'hl-line)
(let ((load-suffixes '(".elc" ".el")))
  (require 'org))
(defconst gt357-org-compiled
  (let ((source (symbol-file 'org-mode 'defun)))
    (and source (string-suffix-p ".elc" source))))
(unless (and (featurep 'org)
             gt357-org-compiled
             (not (featurep 'gnus-sum))
             (not (facep 'gnus-group-news-low))
             (equal load-suffixes '(".el")))
  (error "Gruvbox real Org load boundary failed: org=%S/%S gnus=%S face=%S suffixes=%S"
         (featurep 'org) (symbol-file 'org-mode 'defun)
         (featurep 'gnus-sum) (facep 'gnus-group-news-low) load-suffixes))
(require 'gruvbox)
(require 'orderless)

(defvar pdf-view-midnight-colors
  '("gruvbox-tui-baseline-light" . "gruvbox-tui-baseline-dark"))

(defconst gt357-themes
  '(gruvbox gruvbox-dark-hard gruvbox-dark-medium gruvbox-dark-soft
    gruvbox-light-hard gruvbox-light-medium gruvbox-light-soft))
(defconst gt357-owned-names
  '("*Gruvbox Control*" "*Gruvbox Properties*" "*Gruvbox Elisp*"
    "*Gruvbox Org*" "*Gruvbox Diff*" "*Completions*"))
(defconst gt357-page-size 18)

(defun gt357-copy (value)
  (cond ((consp value) (cons (gt357-copy (car value))
                             (gt357-copy (cdr value))))
        ((vectorp value) (apply #'vector
                                (mapcar #'gt357-copy (append value nil))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun gt357-var (symbol)
  (if (boundp symbol)
      (list :bound t :value (gt357-copy (symbol-value symbol)))
    '(:bound nil)))

(defun gt357-restore-var (symbol state)
  (if (plist-get state :bound)
      (set symbol (gt357-copy (plist-get state :value)))
    (makunbound symbol)))

(defun gt357-disable-all ()
  (dolist (theme gt357-themes)
    (when (custom-theme-enabled-p theme)
      (disable-theme theme))))

(defun gt357-face (face attributes)
  (list
   face
   :direct (mapcar (lambda (attribute)
                     (cons attribute
                           (face-attribute face attribute nil nil)))
                   attributes)
   :resolved (mapcar (lambda (attribute)
                       (cons attribute
                             (face-attribute face attribute nil 'default)))
                     attributes)))

(defun gt357-compact-state ()
  (list
   :enabled (copy-sequence custom-enabled-themes)
   :mode (frame-parameter nil 'background-mode)
   :default (gt357-face 'default '(:foreground :background))
   :syntax
   (list (gt357-face 'font-lock-keyword-face '(:foreground :weight))
         (gt357-face 'font-lock-string-face '(:foreground)))
   :org (gt357-face 'org-link '(:foreground :underline))
   :diff
   (list (gt357-face 'diff-added '(:foreground :background))
         (gt357-face 'diff-removed '(:foreground :background))
         (gt357-face 'diff-context '(:foreground :background)))
   :ui
   (list (gt357-face 'mode-line-inactive '(:foreground :background))
         (gt357-face 'region '(:foreground :background))
         (gt357-face 'hl-line '(:foreground :background)))
   :orderless
   (mapcar (lambda (face) (gt357-face face '(:foreground :weight)))
           '(orderless-match-face-0 orderless-match-face-1
             orderless-match-face-2 orderless-match-face-3))
   :ansi (gt357-var 'ansi-color-names-vector)
   :pdf (gt357-var 'pdf-view-midnight-colors)))

(dolist (theme gt357-themes)
  (load-theme theme t t))
(setq gt357-baseline-captured nil
      gt357-next-theme 0)
(unless (cl-every #'custom-theme-p gt357-themes)
  (error "Gruvbox public theme registration incomplete: %S"
         (mapcar (lambda (theme)
                   (cons theme (and (custom-theme-p theme) t)))
                 gt357-themes)))

(defun gt357-control (&rest lines)
  (when (> (length lines) 20)
    (error "Gruvbox report exceeds visible terminal rows: %d"
           (length lines)))
  (with-current-buffer (get-buffer-create "*Gruvbox Control*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (dolist (line lines)
        (when (> (string-width line) 78)
          (error "Gruvbox report row exceeds terminal contract: %S" line))
        (insert line "\n"))
      (goto-char (point-min))
      (special-mode)
      (switch-to-buffer (current-buffer))
      (delete-other-windows)
      (redisplay t))))

(defun gt357-capability ()
  (list :term (getenv "TERM")
        :colorterm (getenv "COLORTERM")
        :cells (display-color-cells)
        :visual-class (display-visual-class)
        :display-type (frame-parameter nil 'display-type)
        :graphic (display-graphic-p)
        :truecolor
        (face-spec-set-match-display
         '((class color) (min-colors 16777215)) nil)
        :color256
        (face-spec-set-match-display
         '((class color) (min-colors 255)) nil)))

(defun gt357-show-boot ()
  (interactive)
  (when gt357-baseline-captured
    (error "Gruvbox boot baseline already captured"))
  ;; Capture one post-startup baseline atomically, before creating the first
  ;; owned report buffer.  Startup-owned windows/resources must never be
  ;; mistaken for package workflow state.
  (setq gt357-baseline-captured t
        gt357-enabled-before (copy-sequence custom-enabled-themes)
        gt357-known-before (copy-sequence custom-known-themes)
        gt357-ansi-before (gt357-var 'ansi-color-names-vector)
        gt357-pdf-before (gt357-var 'pdf-view-midnight-colors)
        gt357-bold-before (gt357-var 'gruvbox-bold-constructs)
        gt357-screenshot-before (gt357-var 'gruvbox-screenshot-command)
        gt357-org-modules-before (copy-tree org-modules)
        gt357-consumer-profile-before (gt357-var 'gt357-consumer-profile)
        gt357-gnus-before (featurep 'gnus-sum)
        gt357-background-before (frame-parameter nil 'background-mode)
        gt357-window-before (current-window-configuration)
        gt357-selected-window-before (selected-window)
        gt357-buffer-before (current-buffer)
        gt357-buffers-before (buffer-list)
        gt357-processes-before (process-list)
        gt357-timers-before (copy-sequence timer-list)
        gt357-face-before
        (mapcar (lambda (spec) (gt357-face (car spec) (cdr spec)))
                '((default :foreground :background)
                  (font-lock-keyword-face :foreground :weight)
                  (org-link :foreground :underline)
                  (diff-added :foreground :background)))
        gt357-autothemer-before autothemer-current-theme)
  (let ((cap (gt357-capability)))
    (apply #'gt357-control
           (append
            (list (format "CAP TERM %S" (plist-get cap :term))
                  (format "CAP COLORTERM %S" (plist-get cap :colorterm))
                  (format "CAP CELLS %S" (plist-get cap :cells))
                  (format "CAP VISUAL %S" (plist-get cap :visual-class))
                  (format "CAP DISPLAY %S" (plist-get cap :display-type))
                  (format "CAP GRAPHIC %S" (plist-get cap :graphic))
                  (format "CAP TRUECOLOR %S" (plist-get cap :truecolor))
                  (format "CAP COLOR256 %S" (plist-get cap :color256))
                  (format "CAP ORG-COMPILED %S" gt357-org-compiled)
                  (format "CAP GNUS-BEFORE %S" (featurep 'gnus-sum))
                  (format "CAP LOAD-SUFFIXES %S" load-suffixes)
                  "THEMES-KNOWN t"
                  "GRUVBOX-TUI-BOOT")))))

(defun gt357-configure-core-org ()
  (interactive)
  (unless (and (not (featurep 'gnus-sum))
               (not (facep 'gnus-group-news-low))
               (not (facep 'gnus-group-news-low-empty)))
    (error "Gruvbox core Org precondition changed: %S/%S/%S"
           (featurep 'gnus-sum)
           (facep 'gnus-group-news-low)
           (facep 'gnus-group-news-low-empty)))
  (let ((before (copy-tree org-modules)))
    (setq org-modules nil)
    (setq gt357-consumer-profile 'core)
    (gt357-control
     (format "CORE-ORG BEFORE-1 %S"
             (cl-subseq before 0 (min 5 (length before))))
     (format "CORE-ORG BEFORE-2 %S" (nthcdr 5 before))
     (format "CORE-ORG AFTER %S" org-modules)
     (format "CORE-ORG GNUS %S"
             (list (featurep 'gnus-sum)
                   (and (facep 'gnus-group-news-low) t)
                   (and (facep 'gnus-group-news-low-empty) t)))
     "GRUVBOX-CORE-ORG-READY")))

(defun gt357-configure-default-org ()
  (interactive)
  (unless (and (memq 'ol-gnus org-modules)
               (not (featurep 'gnus-sum))
               (not (facep 'gnus-group-news-low))
               (not (facep 'gnus-group-news-low-empty)))
    (error "Gruvbox default Org precondition changed: %S/%S/%S/%S"
           org-modules (featurep 'gnus-sum)
           (facep 'gnus-group-news-low)
           (facep 'gnus-group-news-low-empty)))
  (setq gt357-consumer-profile 'default)
  (gt357-control
   (format "DEFAULT-ORG MODULES-1 %S"
           (cl-subseq org-modules 0 (min 5 (length org-modules))))
   (format "DEFAULT-ORG MODULES-2 %S" (nthcdr 5 org-modules))
   "DEFAULT-ORG GNUS (nil nil nil)"
   "GRUVBOX-DEFAULT-ORG-READY"))

(defun gt357-state-lines (theme)
  (let ((state (gt357-compact-state)))
    (append
      (list (format "THEME %S" theme)
            (format "ENABLED %S" (plist-get state :enabled))
            (format "MODE %S" (plist-get state :mode)))
      (apply
       #'append
       (mapcar
        (lambda (spec)
          (mapcar
           (lambda (attribute)
             (format "FACE %s %s %S %S"
                     (car spec) attribute
                     (face-attribute (cadr spec) attribute nil nil)
                     (face-attribute (cadr spec) attribute nil 'default)))
           (cddr spec)))
        '((default default :foreground :background)
          (keyword font-lock-keyword-face :foreground :weight)
          (string font-lock-string-face :foreground)
          (org-link org-link :foreground :underline)
          (diff-added diff-added :foreground :background)
          (diff-removed diff-removed :foreground :background)
          (diff-context diff-context :foreground :background)
          (mode-line-inactive mode-line-inactive :foreground :background)
          (region region :foreground :background)
          (hl-line hl-line :foreground :background)
          (cursor cursor :background)
          (orderless-0 orderless-match-face-0 :foreground :weight)
          (orderless-1 orderless-match-face-1 :foreground :weight)
          (orderless-2 orderless-match-face-2 :foreground :weight)
          (orderless-3 orderless-match-face-3 :foreground :weight))))
      (let ((ansi (plist-get state :ansi))
            (pdf (plist-get state :pdf)))
        (append
         (list (format "VAR ANSI-BOUND %S" (plist-get ansi :bound)))
         (cl-loop for value across (plist-get ansi :value)
                  for index from 0
                  collect (format "VAR ANSI %d %S" index value))
         (list (format "VAR PDF-BOUND %S" (plist-get pdf :bound))
               (format "VAR PDF-LIGHT %S" (car (plist-get pdf :value)))
               (format "VAR PDF-DARK %S" (cdr (plist-get pdf :value))))))
      nil)))

(defun gt357-render-state-page ()
  (let* ((total (ceiling (/ (float (length gt357-state-lines))
                            gt357-page-size)))
         (start (* gt357-state-page gt357-page-size))
         (end (min (length gt357-state-lines)
                   (+ start gt357-page-size))))
    (unless (< start (length gt357-state-lines))
      (error "Gruvbox state pagination exhausted"))
    (apply #'gt357-control
           (append
            (list (format "GRUVBOX-THEME-PAGE %d/%d"
                          (1+ gt357-state-page) total))
            (cl-subseq gt357-state-lines start end)
            (list (format "GRUVBOX-THEME-PAGE-DONE %d/%d"
                          (1+ gt357-state-page) total))
            (when (= end (length gt357-state-lines))
              '("GRUVBOX-THEME-READY"))))))

(defun gt357-show-state (theme)
  (setq gt357-state-lines (gt357-state-lines theme)
        gt357-state-page 0)
  (gt357-render-state-page))

(defun gt357-next-state-page ()
  (interactive)
  (setq gt357-state-page (1+ gt357-state-page))
  (gt357-render-state-page))

(defun gt357-next-theme ()
  (interactive)
  (let ((theme (nth gt357-next-theme gt357-themes)))
    (unless theme
      (error "Gruvbox theme matrix exhausted"))
    (setq gt357-next-theme (1+ gt357-next-theme))
    (gt357-disable-all)
    (enable-theme theme)
    (gt357-show-state theme)))

(defun gt357-property-runs (buffer)
  (with-current-buffer buffer
    (font-lock-ensure)
    (let ((position (point-min)) runs)
      (while (< position (point-max))
        (let ((next (next-single-property-change
                     position 'face nil (point-max))))
          (push (list (buffer-substring-no-properties position next)
                      (get-text-property position 'face))
                runs)
          (setq position next)))
      (nreverse runs))))

(defun gt357-property-runs-between (buffer start end)
  (with-current-buffer buffer
    (let ((position start) runs)
      (while (< position end)
        (let ((next (next-single-property-change position 'face nil end)))
          (push (list (buffer-substring-no-properties position next)
                      (get-text-property position 'face))
                runs)
          (setq position next)))
      (nreverse runs))))

(defvar gt357-orderless-observed-runs nil)
(defvar gt357-orderless-observer-calls nil)

(defun gt357-orderless-observe-completions ()
  (let ((completions (if (bufferp standard-output)
                         standard-output
                       (get-buffer "*Completions*"))))
    (push (list :current (buffer-name)
                :output (and (bufferp standard-output)
                             (buffer-name standard-output))
                :completions (and completions (buffer-live-p completions)))
          gt357-orderless-observer-calls)
    (when completions
      (with-current-buffer completions
        (save-excursion
          (goto-char (point-min))
          (when (search-forward "alpha beta gamma delta" nil t)
            (setq gt357-orderless-observed-runs
                  (gt357-property-runs-between
                   completions (match-beginning 0) (match-end 0)))))))))

(defun gt357-orderless-select ()
  (interactive)
  (when (get-buffer "*Completions*")
    (error "Gruvbox Orderless completion buffer was not owned"))
  (unless (equal custom-enabled-themes '(gruvbox-dark-medium))
    (error "Gruvbox Orderless theme precondition changed: %S"
           custom-enabled-themes))
  (let* ((completion-styles '(orderless))
         (completion-category-defaults nil)
         (completion-category-overrides nil)
         (minibuffer-history (copy-tree minibuffer-history))
         (gt357-orderless-observed-runs nil)
         (gt357-orderless-observer-calls nil)
         (candidates '("alpha beta gamma delta"
                       "alpha bravo gamma deluxe"
                       "alpha gamma" "beta delta"))
         final-input
         choice history runs)
    (setq choice
          (minibuffer-with-setup-hook
              (lambda ()
                ;; The command following TAB sees the fully rendered public
                ;; completion buffer.  This local observer records it before
                ;; GNU's exact selection closes `*Completions*'.
                (add-hook 'pre-command-hook
                          #'gt357-orderless-observe-completions nil t)
                (add-hook
                 'minibuffer-exit-hook
                 (lambda ()
                   (setq final-input
                         (minibuffer-contents-no-properties)))
                 nil t))
            (completing-read
             "Gruvbox Orderless: " candidates nil t)))
    (setq history (copy-tree minibuffer-history)
          runs (or gt357-orderless-observed-runs
                   (error "Orderless properties absent: calls=%S"
                          (nreverse gt357-orderless-observer-calls))))
    (apply #'gt357-control
           (append
            (list (format "ORDERLESS CHOICE %S" choice)
                  (format "ORDERLESS FINAL-INPUT %S" final-input)
                  (format "ORDERLESS HISTORY-HEAD %S" (car history))
                  (format "ORDERLESS MINIBUFFER %S"
                          (active-minibuffer-window)))
            (mapcar (lambda (run) (format "ORDERLESS RUN %S" run)) runs)
            (cl-loop
             for face across orderless-match-faces
             for index from 0
             collect
             (format "ORDERLESS FACE %d %S %S %S %S"
                     index
                     (face-attribute face :foreground nil nil)
                     (face-attribute face :foreground nil 'default)
                     (face-attribute face :weight nil nil)
                     (face-attribute face :weight nil 'default)))
            '("GRUVBOX-ORDERLESS-READY")))))

(defun gt357-populate ()
  (with-current-buffer (get-buffer-create "*Gruvbox Elisp*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "; comment Ω\n"
              "(defun greet (name)\n"
              "  \"Doc.\"\n"
              "  (if name (message \"Hello %s\" name) nil))\n")
      (emacs-lisp-mode)
      (font-lock-ensure)))
  (with-current-buffer (get-buffer-create "*Gruvbox Org*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "#+title: Plan Ω\n"
              "* TODO Ship release\n"
              "** DONE Verify rollback\n"
              "A [[https://example.invalid][link]] and =code=.\n"
              "#+begin_src emacs-lisp\n"
              "(message \"ship\")\n"
              "#+end_src\n")
      (let* ((first (not (bound-and-true-p gt357-first-consumer)))
             (modules (copy-tree org-modules))
             (profile (and (boundp 'gt357-consumer-profile)
                           gt357-consumer-profile))
             (default-consumer (eq profile 'default)))
        (unless (and (memq profile '(core default))
                     (if default-consumer
                         (memq 'ol-gnus modules)
                       (null modules)))
          (error "Gruvbox Org profile/configuration mismatch: %S/%S"
                 profile modules))
        (when first
          (let ((before
                 (list (featurep 'gnus-sum)
                       (and (facep 'gnus-group-news-low) t)
                       (and (facep 'gnus-group-news-low-empty) t))))
            (unless (equal before '(nil nil nil))
              (error "Gruvbox first consumer precondition changed: %S" before))
            (setq gt357-first-consumer-before before
                  gt357-first-consumer-theme
                  (copy-sequence custom-enabled-themes)
                  gt357-first-consumer-modules modules)))
        (let ((load-suffixes '(".elc" ".el")))
          (setq gt357-consumer-outcome
                (condition-case condition
                    (progn (org-mode) (font-lock-ensure) '(:value returned))
                  (error
                   (list :signal (car condition)
                         :data (cdr condition)
                         :message (error-message-string condition))))))
        (let* ((source (symbol-file 'gnus-summary-mode 'defun))
               (compiled (and source (string-suffix-p ".elc" source)))
               (after
                (list (featurep 'gnus-sum)
                      (and (facep 'gnus-group-news-low) t)
                      (and (facep 'gnus-group-news-low-empty) t)))
               (inherit
                (and (cadr after) (caddr after)
                     (list
                      (face-attribute
                       'gnus-group-news-low :inherit nil nil)
                      (face-attribute
                       'gnus-group-news-low-empty :inherit nil nil)))))
          (setq gt357-gnus-compiled compiled)
          (unless (and (equal gt357-consumer-outcome '(:value returned))
                       (equal load-suffixes '(".el"))
                       (if default-consumer
                           (and (equal after '(t t t)) compiled)
                         (and (null modules)
                              (equal after '(nil nil nil))
                              (not compiled))))
            (error "Gruvbox Org boundary failed: modules=%S outcome=%S feature=%S source=%S faces=%S suffixes=%S"
                   modules gt357-consumer-outcome (featurep 'gnus-sum)
                   source (cdr after) load-suffixes))
          (when first
            (setq gt357-first-consumer
                  (list
                   :modules gt357-first-consumer-modules
                   :before gt357-first-consumer-before
                   :theme gt357-first-consumer-theme
                   :outcome gt357-consumer-outcome
                   :source (and source (file-name-nondirectory source))
                   :compiled compiled
                   :after after
                   :inherit inherit
                   :suffixes (copy-sequence load-suffixes))))))))
  (with-current-buffer (get-buffer-create "*Gruvbox Diff*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "diff --git a/a.el b/a.el\n"
              "--- a/a.el\n"
              "+++ b/a.el\n"
              "@@ -1 +1 @@\n"
              "-(old)\n"
              "+(new)\n"
              " context\n")
      (diff-mode)
      (font-lock-ensure))))

(defun gt357-use-theme (theme)
  (gt357-disable-all)
  (enable-theme theme)
  (gt357-populate)
  (switch-to-buffer "*Gruvbox Elisp*")
  (setq-local header-line-format (format "GRUVBOX %S ELISP" theme))
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun gt357-use-dark-medium ()
  (interactive)
  (gt357-use-theme 'gruvbox-dark-medium))

(defun gt357-use-light-medium ()
  (interactive)
  (gt357-use-theme 'gruvbox-light-medium))

(defun gt357-show-consumer-state ()
  (interactive)
  (unless (bound-and-true-p gt357-first-consumer)
    (error "Gruvbox first consumer observation is absent"))
  (let ((modules (plist-get gt357-first-consumer :modules)))
    (gt357-control
     (format "CONSUMER MODULES-1 %S"
             (cl-subseq modules 0 (min 5 (length modules))))
     (format "CONSUMER MODULES-2 %S" (nthcdr 5 modules))
     (format "CONSUMER BEFORE %S" (plist-get gt357-first-consumer :before))
     (format "CONSUMER THEME %S" (plist-get gt357-first-consumer :theme))
     (format "CONSUMER OUTCOME %S" (plist-get gt357-first-consumer :outcome))
     (format "CONSUMER SOURCE %S %S"
             (plist-get gt357-first-consumer :source)
             (plist-get gt357-first-consumer :compiled))
     (format "CONSUMER AFTER %S" (plist-get gt357-first-consumer :after))
     (format "CONSUMER INHERIT %S"
             (plist-get gt357-first-consumer :inherit))
     (format "CONSUMER SUFFIXES %S"
             (plist-get gt357-first-consumer :suffixes))
     "GRUVBOX-CONSUMER-READY")))

(defun gt357-show-elisp ()
  (interactive)
  (switch-to-buffer "*Gruvbox Elisp*")
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun gt357-show-org ()
  (interactive)
  (switch-to-buffer "*Gruvbox Org*")
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun gt357-show-diff ()
  (interactive)
  (switch-to-buffer "*Gruvbox Diff*")
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun gt357-show-buffer-properties (tag name)
  (let ((print-escape-newlines t)
        (print-escape-control-characters t))
    (setq gt357-property-tag tag
          gt357-property-lines
          (mapcar (lambda (run) (format "RUN %s %S" tag run))
                  (gt357-property-runs (get-buffer name)))
          gt357-property-count (length gt357-property-lines)
          gt357-property-page 0))
  (dolist (line gt357-property-lines)
    (when (> (string-width line) 78)
      (error "Gruvbox property row exceeds terminal contract: %S" line)))
  (gt357-render-property-page))

(defun gt357-render-property-page ()
  (let* ((page-size 15)
         (total (ceiling (/ (float gt357-property-count) page-size)))
         (start (* gt357-property-page page-size))
         (end (min gt357-property-count (+ start page-size))))
    (unless (and (> total 0) (< start gt357-property-count))
      (error "Gruvbox property page out of range: %s %d/%d count=%d"
             gt357-property-tag (1+ gt357-property-page) total
             gt357-property-count))
    (apply #'gt357-control
           (append
            (list
             (format "PROPERTIES %S %s PAGE %d/%d"
                     (car custom-enabled-themes) gt357-property-tag
                     (1+ gt357-property-page) total)
             (format "PROPERTY-COUNT %s %d"
                     gt357-property-tag gt357-property-count))
            (cl-subseq gt357-property-lines start end)
            (list
             (format "GRUVBOX-PROPERTIES-%s-PAGE-DONE %d/%d"
                     gt357-property-tag (1+ gt357-property-page) total))
            (when (= (1+ gt357-property-page) total)
              (list (format "GRUVBOX-PROPERTIES-%s-READY"
                            gt357-property-tag)))))))

(defun gt357-next-property-page ()
  (interactive)
  (setq gt357-property-page (1+ gt357-property-page))
  (gt357-render-property-page))

(defun gt357-show-elisp-properties ()
  (interactive)
  (gt357-show-buffer-properties "E" "*Gruvbox Elisp*"))

(defun gt357-show-org-properties ()
  (interactive)
  (gt357-show-buffer-properties "O" "*Gruvbox Org*"))

(defun gt357-show-diff-properties ()
  (interactive)
  (gt357-show-buffer-properties "D" "*Gruvbox Diff*"))

(defun gt357-use-light-over-dark ()
  (interactive)
  (gt357-disable-all)
  (enable-theme 'gruvbox-dark-medium)
  (gt357-populate)
  (enable-theme 'gruvbox-light-medium)
  (switch-to-buffer "*Gruvbox Elisp*")
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun gt357-disable-stack-light ()
  (interactive)
  (disable-theme 'gruvbox-light-medium)
  (switch-to-buffer "*Gruvbox Elisp*")
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun gt357-show-current-state ()
  (interactive)
  (gt357-show-state (car custom-enabled-themes)))

(defun gt357-show-bold-cycle ()
  (interactive)
  (gt357-disable-all)
  (setq gruvbox-bold-constructs nil)
  (load-theme 'gruvbox-dark-medium t)
  (let ((plain
         (list (face-attribute 'font-lock-keyword-face :weight nil nil)
               (face-attribute 'org-level-1 :weight nil nil))))
    (setq gruvbox-bold-constructs t)
    (let ((before-reload
           (list (face-attribute 'font-lock-keyword-face :weight nil nil)
                 (face-attribute 'org-level-1 :weight nil nil))))
      (load-theme 'gruvbox-dark-medium t)
      (let ((bold
             (list (face-attribute 'font-lock-keyword-face :weight nil nil)
                   (face-attribute 'org-level-1 :weight nil nil))))
        (setq gruvbox-bold-constructs nil)
        (load-theme 'gruvbox-dark-medium t)
        (let ((plain-again
               (list (face-attribute 'font-lock-keyword-face :weight nil nil)
                     (face-attribute 'org-level-1 :weight nil nil))))
          (gt357-populate)
          (gt357-control
           (format "BOLD PLAIN %S" plain)
           (format "BOLD BEFORE-RELOAD %S" before-reload)
           (format "BOLD RELOADED %S" bold)
           (format "BOLD PLAIN-AGAIN %S" plain-again)
           (format "BOLD ORG-RUN %S"
                   (cl-find-if (lambda (run)
                                 (string-match-p "TODO" (car run)))
                               (gt357-property-runs
                                (get-buffer "*Gruvbox Org*"))))
           "GRUVBOX-BOLD-READY"))))))

(defun gt357-cleanup-call (phase thunk errors)
  (condition-case condition
      (progn (funcall thunk) errors)
    (error (cons (list phase condition) errors))))

(defun gt357-finish ()
  (interactive)
  (let (errors)
    (setq errors (gt357-cleanup-call
                  'disable #'gt357-disable-all errors))
    (setq errors
          (gt357-cleanup-call
           'restore-options
           (lambda ()
             (gt357-restore-var 'gruvbox-bold-constructs gt357-bold-before)
             (setq autothemer-current-theme gt357-autothemer-before)
             (gt357-restore-var
              'ansi-color-names-vector gt357-ansi-before)
             (gt357-restore-var
              'pdf-view-midnight-colors gt357-pdf-before)
             (gt357-restore-var
              'gruvbox-screenshot-command gt357-screenshot-before)
             (setq org-modules (copy-tree gt357-org-modules-before))
             (gt357-restore-var
              'gt357-consumer-profile gt357-consumer-profile-before)
             (set-frame-parameter nil 'background-mode
                                  gt357-background-before))
           errors))
    (dotimes (sweep 2)
      (dolist (process
               (cl-set-difference (process-list) gt357-processes-before))
        (setq errors
              (gt357-cleanup-call
               (list 'process sweep (process-name process))
               (lambda () (delete-process process)) errors)))
      (dolist (timer (cl-set-difference timer-list gt357-timers-before))
        (setq errors
              (gt357-cleanup-call
               (list 'timer sweep timer)
               (lambda () (cancel-timer timer)) errors)))
      (dolist (buffer (cl-set-difference (buffer-list)
                                         gt357-buffers-before))
        (setq errors
              (gt357-cleanup-call
               (list 'buffer sweep (buffer-name buffer))
               (lambda ()
                 (when (buffer-live-p buffer)
                   (kill-buffer buffer)))
               errors))))
    (setq errors
          (gt357-cleanup-call
           'windows
           (lambda ()
             (set-window-configuration gt357-window-before)
             (when (window-live-p gt357-selected-window-before)
               (select-window gt357-selected-window-before))
             (when (buffer-live-p gt357-buffer-before)
               (set-window-buffer (selected-window) gt357-buffer-before)))
           errors))
    (let ((state
           (list
            :enabled (equal custom-enabled-themes gt357-enabled-before)
            :known (equal custom-known-themes gt357-known-before)
            :faces
            (equal
             (mapcar (lambda (spec) (gt357-face (car spec) (cdr spec)))
                     '((default :foreground :background)
                       (font-lock-keyword-face :foreground :weight)
                       (org-link :foreground :underline)
                       (diff-added :foreground :background)))
             gt357-face-before)
            :ansi (equal (gt357-var 'ansi-color-names-vector)
                         gt357-ansi-before)
            :pdf (equal (gt357-var 'pdf-view-midnight-colors)
                        gt357-pdf-before)
            :bold (equal (gt357-var 'gruvbox-bold-constructs)
                         gt357-bold-before)
            :screenshot
            (equal (gt357-var 'gruvbox-screenshot-command)
                   gt357-screenshot-before)
            :autothemer (eq autothemer-current-theme
                             gt357-autothemer-before)
            :org-modules (equal org-modules gt357-org-modules-before)
            :consumer-profile
            (equal (gt357-var 'gt357-consumer-profile)
                   gt357-consumer-profile-before)
            :consumer
            (and
             (not gt357-gnus-before)
             (bound-and-true-p gt357-first-consumer)
             (equal
              (list (featurep 'gnus-sum)
                    (and (facep 'gnus-group-news-low) t)
                    (and (facep 'gnus-group-news-low-empty) t))
              (plist-get gt357-first-consumer :after))
             (equal (and (bound-and-true-p gt357-gnus-compiled) t)
                    (plist-get gt357-first-consumer :compiled)))
            :background (eq (frame-parameter nil 'background-mode)
                            gt357-background-before)
            :window (compare-window-configurations
                     (current-window-configuration)
                     gt357-window-before)
            :selected-window (eq (selected-window)
                                 gt357-selected-window-before)
            :owned-buffers
            (delq nil (mapcar #'get-buffer gt357-owned-names))
            :new-buffers
            (cl-set-difference (buffer-list) gt357-buffers-before)
            :new-processes
            (cl-set-difference (process-list) gt357-processes-before)
            :new-timers
            (cl-set-difference timer-list gt357-timers-before)
            :buffer (eq (current-buffer) gt357-buffer-before))))
      (unless (and (null errors)
                   (equal state
                          '(:enabled t :known t :faces t
                            :ansi t :pdf t :bold t :screenshot t
                            :autothemer t :org-modules t
                            :consumer-profile t :consumer t
                            :background t :window t
                            :selected-window t :owned-buffers nil
                            :new-buffers nil
                            :new-processes nil :new-timers nil
                            :buffer t)))
        (error "Gruvbox TUI cleanup failed: errors=%S state=%S"
               (nreverse errors) state))
      (message "GRUVBOX-TUI-CLEAN (:state t :errors nil)"))))

"####;

const REPORT_PREFIXES: &[&str] = &[
    "CAP ",
    "THEMES-KNOWN ",
    "GRUVBOX-TUI-BOOT",
    "GRUVBOX-THEME-PAGE ",
    "GRUVBOX-THEME-PAGE-DONE ",
    "THEME ",
    "ENABLED ",
    "MODE ",
    "FACE ",
    "VAR ",
    "GRUVBOX-THEME-READY",
    "PROPERTIES ",
    "PROPERTY-COUNT ",
    "RUN ",
    "GRUVBOX-PROPERTIES-",
    "BOLD ",
    "GRUVBOX-BOLD-READY",
    "ORDERLESS ",
    "GRUVBOX-ORDERLESS-READY",
    "CONSUMER ",
    "GRUVBOX-CONSUMER-READY",
    "CORE-ORG ",
    "GRUVBOX-CORE-ORG-READY",
    "DEFAULT-ORG ",
    "GRUVBOX-DEFAULT-ORG-READY",
];

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GRUVBOX_THEME_MELPA_PIN, "gruvbox.el")
        .expect("prepare exact Gruvbox Theme source below ./tmp")
        .with_installed_autoloads()
        .with_melpa_dependency(ORDERLESS_MELPA_PIN)
        .expect("prepare exact Orderless optional-integration source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat closure for Orderless below ./tmp")
        .with_prelude(GRUVBOX_TUI_PRELUDE)
}

fn wait_for<F>(session: &mut TuiSession, description: &str, predicate: F)
where
    F: Fn(&[String]) -> bool,
{
    session.read_until(Duration::from_secs(20), |grid| predicate(grid));
    let grid = session.text_grid();
    assert!(
        predicate(&grid),
        "{} timed out waiting for {description}:\n{}",
        session.name,
        grid.join("\n")
    );
}

fn invoke(session: &mut TuiSession, command: &str, ready: &str) {
    session.send_keys("M-x");
    wait_for(session, "M-x prompt", |grid| {
        grid.iter().any(|row| row.contains("M-x"))
    });
    session.send(command.as_bytes());
    session.send_keys("RET");
    wait_for(session, ready, |grid| {
        grid.iter().any(|row| row.contains(ready))
    });
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|value| (*value).to_owned())
        })
        .unwrap_or_else(|| "non-string panic payload".to_owned())
}

fn catch_phase<T>(label: &str, phase: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(phase))
        .map_err(|payload| format!("{label}: {}", panic_text(payload)))
}

fn invoke_both(pair: &mut PackageTuiPair, command: &str, ready: &str) {
    let gnu = catch_phase(&format!("GNU {command}"), || {
        invoke(&mut pair.gnu, command, ready)
    });
    let neo = catch_phase(&format!("Neo {command}"), || {
        invoke(&mut pair.neo, command, ready)
    });
    let errors = [gnu.err(), neo.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "dual-peer command failed:\n{}",
        errors.join("\n")
    );
}

fn wait_for_boot_both(pair: &mut PackageTuiPair) {
    // Queue the public command into each real command loop.  GNU may display
    // an informational startup warning after the startup file finishes; the
    // explicit command makes the final visible state the owned report.
    invoke_both(pair, "gt357-show-boot", "GRUVBOX-TUI-BOOT");
}

fn report(session: &TuiSession) -> String {
    session
        .text_grid()
        .into_iter()
        .map(|row| row.trim_end().to_owned())
        .filter(|row| REPORT_PREFIXES.iter().any(|prefix| row.starts_with(prefix)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn record_pair(
    pair: &PackageTuiPair,
    label: &str,
    expected: Expect,
    mismatches: &mut Vec<String>,
) -> String {
    let gnu = report(&pair.gnu);
    let neo = report(&pair.neo);
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_eq(&gnu);
    gnu
}

fn ansi_rows(session: &TuiSession, needles: &[&str]) -> String {
    let grid = session.text_grid();
    needles
        .iter()
        .map(|needle| {
            let row = grid
                .iter()
                .position(|contents| contents.contains(needle))
                .unwrap_or_else(|| {
                    panic!(
                        "{} never rendered {needle:?}:\n{}",
                        session.name,
                        grid.join("\n")
                    )
                }) as u16;
            let mut snapshot = RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1);
            let meaningful_end = snapshot.rows[0]
                .cells
                .iter()
                .rposition(|cell| {
                    cell.contents()
                        .chars()
                        .any(|character| !character.is_whitespace())
                })
                .unwrap_or(0)
                + 1;
            snapshot.rows[0].cells.truncate(meaningful_end);
            snapshot.ansi_grid()
        })
        .collect::<Vec<_>>()
        .join("")
}

fn record_grid(
    pair: &PackageTuiPair,
    label: &str,
    needles: &[&str],
    expected: Expect,
    mismatches: &mut Vec<String>,
) -> String {
    let gnu = catch_phase(&format!("GNU {label} grid"), || {
        ansi_rows(&pair.gnu, needles)
    });
    let neo = catch_phase(&format!("Neo {label} grid"), || {
        ansi_rows(&pair.neo, needles)
    });
    let errors = [gnu.as_ref().err(), neo.as_ref().err()]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "dual-peer grid capture failed:\n{}",
        errors.join("\n")
    );
    let gnu = gnu.expect("checked GNU grid result");
    let neo = neo.expect("checked Neo grid result");
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU: {gnu:?}\nNeo: {neo:?}"));
    }
    expected.assert_eq(&gnu);
    gnu
}

fn record_properties(
    pair: &mut PackageTuiPair,
    label: &str,
    expected: Expect,
    mismatches: &mut Vec<String>,
) -> String {
    let mut gnu = Vec::new();
    let mut neo = Vec::new();
    for (command, tag) in [
        ("gt357-show-elisp-properties", "E"),
        ("gt357-show-org-properties", "O"),
        ("gt357-show-diff-properties", "D"),
    ] {
        invoke_both(
            pair,
            command,
            &format!("GRUVBOX-PROPERTIES-{tag}-PAGE-DONE 1/"),
        );
        let mut gnu_pages = vec![report(&pair.gnu)];
        let mut neo_pages = vec![report(&pair.neo)];
        let page_total = |editor: &str, report: &str| {
            let line = report
                .lines()
                .find(|line| line.contains(&format!(" {tag} PAGE 1/")))
                .unwrap_or_else(|| {
                    panic!(
                        "{editor} property report omitted the exact {tag} page header:\n{report}"
                    )
                });
            line.rsplit_once('/')
                .and_then(|(_, total)| total.parse::<usize>().ok())
                .filter(|total| *total > 0)
                .unwrap_or_else(|| {
                    panic!("{editor} property report has invalid {tag} page total:\n{report}")
                })
        };
        let gnu_total = page_total("GNU", &gnu_pages[0]);
        let neo_total = page_total("Neo", &neo_pages[0]);
        assert_eq!(
            neo_total, gnu_total,
            "{label} {tag} property page count differs before snapshots"
        );
        for page in 2..=gnu_total {
            let ready = if page == gnu_total {
                format!("GRUVBOX-PROPERTIES-{tag}-READY")
            } else {
                format!("GRUVBOX-PROPERTIES-{tag}-PAGE-DONE {page}/{gnu_total}")
            };
            invoke_both(pair, "gt357-next-property-page", &ready);
            let gnu_page = report(&pair.gnu);
            let neo_page = report(&pair.neo);
            for (editor, page_report) in [("GNU", &gnu_page), ("Neo", &neo_page)] {
                assert!(
                    page_report
                        .lines()
                        .any(|line| line.contains(&format!(" {tag} PAGE {page}/{gnu_total}"))),
                    "{editor} property report omitted exact {tag} page {page}/{gnu_total}:\n{page_report}"
                );
            }
            gnu_pages.push(gnu_page);
            neo_pages.push(neo_page);
        }
        let gnu_report = gnu_pages.join("\n--\n");
        let neo_report = neo_pages.join("\n--\n");
        for (editor, final_page) in [
            ("GNU", gnu_pages.last().expect("GNU property page")),
            ("Neo", neo_pages.last().expect("Neo property page")),
        ] {
            assert!(
                final_page
                    .lines()
                    .any(|line| line == format!("GRUVBOX-PROPERTIES-{tag}-READY")),
                "{editor} property report omitted the trailing {tag} ready marker:\n{final_page}"
            );
        }
        assert!(
            gnu_report.contains(&format!("PROPERTY-COUNT {tag} ")),
            "GNU property report omitted the exact {tag} run count:\n{gnu_report}"
        );
        assert!(
            neo_report.contains(&format!("PROPERTY-COUNT {tag} ")),
            "Neo property report omitted the exact {tag} run count:\n{neo_report}"
        );
        gnu.push(gnu_report);
        neo.push(neo_report);
    }
    let gnu = gnu.join("\n--\n");
    let neo = neo.join("\n--\n");
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_eq(&gnu);
    gnu
}

fn drive_orderless_completion(session: &mut TuiSession) -> (String, String) {
    session.send_keys("M-x");
    wait_for(session, "M-x before Orderless completion", |grid| {
        grid.iter().any(|row| row.contains("M-x"))
    });
    session.send(b"gt357-orderless-select");
    session.send_keys("RET");
    wait_for(session, "real Orderless minibuffer prompt", |grid| {
        grid.iter().any(|row| row.contains("Gruvbox Orderless:"))
    });
    session.send(b"alp b gam del");
    session.send_keys("TAB");
    wait_for(session, "Orderless four-component completion row", |grid| {
        grid.iter()
            .any(|row| row.contains("alpha beta gamma delta"))
    });
    let grid = ansi_rows(session, &["alpha beta gamma delta"]);
    session.send_keys("C-a");
    session.send_keys("C-k");
    session.send(b"alpha beta gamma delta");
    session.send_keys("RET");
    wait_for(session, "completed Orderless selection", |grid| {
        grid.iter()
            .any(|row| row.contains("GRUVBOX-ORDERLESS-READY"))
    });
    (grid, report(session))
}

fn record_orderless_completion(
    pair: &mut PackageTuiPair,
    grid_expected: Expect,
    report_expected: Expect,
    mismatches: &mut Vec<String>,
) {
    let gnu = catch_phase("GNU real Orderless completion", || {
        drive_orderless_completion(&mut pair.gnu)
    });
    let neo = catch_phase("Neo real Orderless completion", || {
        drive_orderless_completion(&mut pair.neo)
    });
    let errors = [gnu.as_ref().err(), neo.as_ref().err()]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "dual-peer Orderless completion failed:\n{}",
        errors.join("\n")
    );
    let (gnu_grid, gnu_report) = gnu.expect("checked GNU Orderless result");
    let (neo_grid, neo_report) = neo.expect("checked Neo Orderless result");
    if neo_grid != gnu_grid {
        mismatches.push(format!(
            "Orderless rendered completion differs\nGNU: {gnu_grid:?}\nNeo: {neo_grid:?}"
        ));
    }
    if neo_report != gnu_report {
        mismatches.push(format!(
            "Orderless completion report differs\nGNU:\n{gnu_report}\nNeo:\n{neo_report}"
        ));
    }
    grid_expected.assert_eq(&gnu_grid);
    report_expected.assert_eq(&gnu_report);
}

fn initialize_consumer(
    pair: &mut PackageTuiPair,
    profile: &str,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    invoke_both(pair, "gt357-use-dark-medium", "; comment Ω");
    invoke_both(pair, "gt357-show-consumer-state", "GRUVBOX-CONSUMER-READY");
    record_pair(
        pair,
        &format!("{profile} first lazy compiled consumer"),
        expected,
        mismatches,
    );
}

fn spawn_profile(
    label: &str,
    packages: &PreparedPackageSet,
    display: DisplayEnvOverride<'_>,
) -> Result<PackageTuiPair, String> {
    PackageTuiPair::spawn_with_display_env(label, packages, &[display])
}

fn capture_matrix_pair(pair: &mut PackageTuiPair) -> (String, String) {
    let mut gnu = Vec::new();
    let mut neo = Vec::new();
    for _ in 0..7 {
        invoke_both(pair, "gt357-next-theme", "GRUVBOX-THEME-PAGE-DONE 1/3");
        gnu.push(report(&pair.gnu));
        neo.push(report(&pair.neo));
        invoke_both(pair, "gt357-next-state-page", "GRUVBOX-THEME-PAGE-DONE 2/3");
        gnu.push(report(&pair.gnu));
        neo.push(report(&pair.neo));
        invoke_both(pair, "gt357-next-state-page", "GRUVBOX-THEME-READY");
        let gnu_final = report(&pair.gnu);
        let neo_final = report(&pair.neo);
        assert!(
            gnu_final
                .lines()
                .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
            "GNU final matrix page lacks the exact 3/3 header:\n{gnu_final}"
        );
        assert!(
            neo_final
                .lines()
                .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
            "Neo final matrix page lacks the exact 3/3 header:\n{neo_final}"
        );
        gnu.push(gnu_final);
        neo.push(neo_final);
    }
    (gnu.join("\n--\n"), neo.join("\n--\n"))
}

fn assert_matrix(
    pair: &mut PackageTuiPair,
    label: &str,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    let (gnu, neo) = capture_matrix_pair(pair);
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_eq(&gnu);
}

fn finish(pair: &mut PackageTuiPair, mismatches: &mut Vec<String>) {
    invoke_both(pair, "gt357-finish", "GRUVBOX-TUI-CLEAN");
    let gnu = pair
        .gnu
        .text_grid()
        .into_iter()
        .find(|row| row.contains("GRUVBOX-TUI-CLEAN"))
        .unwrap_or_else(|| panic!("GNU did not report Gruvbox cleanup"));
    let neo = pair
        .neo
        .text_grid()
        .into_iter()
        .find(|row| row.contains("GRUVBOX-TUI-CLEAN"))
        .unwrap_or_else(|| panic!("Neo did not report Gruvbox cleanup"));
    if neo.trim_end() != gnu.trim_end() {
        mismatches.push(format!(
            "final cleanup differs\nGNU: {:?}\nNeo: {:?}",
            gnu.trim_end(),
            neo.trim_end()
        ));
    }
    expect!["GRUVBOX-TUI-CLEAN (:state t :errors nil)"].assert_eq(gnu.trim_end());
}

struct RenderingExpectations {
    dark_elisp: Expect,
    dark_org: Expect,
    dark_diff: Expect,
    dark_properties: Expect,
    dark_state: Expect,
    light_elisp: Expect,
    light_org: Expect,
    light_diff: Expect,
    light_properties: Expect,
    light_state: Expect,
}

struct RenderingSnapshots {
    dark_elisp: String,
    dark_org: String,
    dark_diff: String,
    dark_properties: String,
    dark_state: String,
    light_elisp: String,
    light_org: String,
    light_diff: String,
    light_properties: String,
    light_state: String,
}

fn exercise_rendering(
    pair: &mut PackageTuiPair,
    profile: &str,
    expected: RenderingExpectations,
    mismatches: &mut Vec<String>,
) -> RenderingSnapshots {
    let RenderingExpectations {
        dark_elisp,
        dark_org,
        dark_diff,
        dark_properties,
        dark_state,
        light_elisp,
        light_org,
        light_diff,
        light_properties,
        light_state,
    } = expected;
    invoke_both(pair, "gt357-use-dark-medium", "; comment Ω");
    let dark_elisp = record_grid(
        pair,
        &format!("{profile} dark Elisp"),
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        dark_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let dark_org = record_grid(
        pair,
        &format!("{profile} dark Org"),
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        dark_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let dark_diff = record_grid(
        pair,
        &format!("{profile} dark Diff"),
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        dark_diff,
        mismatches,
    );
    let dark_properties = record_properties(
        pair,
        &format!("{profile} dark property runs"),
        dark_properties,
        mismatches,
    );
    let dark_state = record_current_state(
        pair,
        &format!("{profile} dark state"),
        dark_state,
        mismatches,
    );

    invoke_both(pair, "gt357-use-light-medium", "; comment Ω");
    let light_elisp = record_grid(
        pair,
        &format!("{profile} light Elisp"),
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        light_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let light_org = record_grid(
        pair,
        &format!("{profile} light Org"),
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        light_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let light_diff = record_grid(
        pair,
        &format!("{profile} light Diff"),
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        light_diff,
        mismatches,
    );
    let light_properties = record_properties(
        pair,
        &format!("{profile} light property runs"),
        light_properties,
        mismatches,
    );
    let light_state = record_current_state(
        pair,
        &format!("{profile} light state"),
        light_state,
        mismatches,
    );

    RenderingSnapshots {
        dark_elisp,
        dark_org,
        dark_diff,
        dark_properties,
        dark_state,
        light_elisp,
        light_org,
        light_diff,
        light_properties,
        light_state,
    }
}

struct StackRenderingExpectations {
    light_elisp: Expect,
    light_org: Expect,
    light_diff: Expect,
    light_properties: Expect,
    light_state: Expect,
    restored_elisp: Expect,
    restored_org: Expect,
    restored_diff: Expect,
    restored_properties: Expect,
    restored_state: Expect,
}

fn record_current_state(
    pair: &mut PackageTuiPair,
    label: &str,
    expected: Expect,
    mismatches: &mut Vec<String>,
) -> String {
    let mut gnu = Vec::new();
    let mut neo = Vec::new();
    invoke_both(
        pair,
        "gt357-show-current-state",
        "GRUVBOX-THEME-PAGE-DONE 1/3",
    );
    gnu.push(report(&pair.gnu));
    neo.push(report(&pair.neo));
    for page in 2..=3 {
        let ready = if page == 3 {
            "GRUVBOX-THEME-READY".to_owned()
        } else {
            format!("GRUVBOX-THEME-PAGE-DONE {page}/3")
        };
        invoke_both(pair, "gt357-next-state-page", &ready);
        let gnu_page = report(&pair.gnu);
        let neo_page = report(&pair.neo);
        if page == 3 {
            assert!(
                gnu_page
                    .lines()
                    .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
                "GNU final state page lacks the exact 3/3 header:\n{gnu_page}"
            );
            assert!(
                neo_page
                    .lines()
                    .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
                "Neo final state page lacks the exact 3/3 header:\n{neo_page}"
            );
        }
        gnu.push(gnu_page);
        neo.push(neo_page);
    }
    let gnu = gnu.join("\n--\n");
    let neo = neo.join("\n--\n");
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_eq(&gnu);
    gnu
}

fn exercise_stack_rendering(
    pair: &mut PackageTuiPair,
    ordinary: &RenderingSnapshots,
    expected: StackRenderingExpectations,
    mismatches: &mut Vec<String>,
) {
    let StackRenderingExpectations {
        light_elisp,
        light_org,
        light_diff,
        light_properties,
        light_state,
        restored_elisp,
        restored_org,
        restored_diff,
        restored_properties,
        restored_state,
    } = expected;

    invoke_both(pair, "gt357-use-light-over-dark", "; comment Ω");
    let light_elisp_actual = record_grid(
        pair,
        "truecolor stacked light Elisp",
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        light_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let light_org_actual = record_grid(
        pair,
        "truecolor stacked light Org",
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        light_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let light_diff_actual = record_grid(
        pair,
        "truecolor stacked light Diff",
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        light_diff,
        mismatches,
    );
    let light_properties_actual = record_properties(
        pair,
        "truecolor stacked light property runs",
        light_properties,
        mismatches,
    );
    let light_state_actual = record_current_state(
        pair,
        "truecolor stacked light state",
        light_state,
        mismatches,
    );

    invoke_both(pair, "gt357-disable-stack-light", "; comment Ω");
    let restored_elisp_actual = record_grid(
        pair,
        "truecolor restored dark Elisp",
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        restored_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let restored_org_actual = record_grid(
        pair,
        "truecolor restored dark Org",
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        restored_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let restored_diff_actual = record_grid(
        pair,
        "truecolor restored dark Diff",
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        restored_diff,
        mismatches,
    );
    let restored_properties_actual = record_properties(
        pair,
        "truecolor restored dark property runs",
        restored_properties,
        mismatches,
    );
    let restored_state_actual = record_current_state(
        pair,
        "truecolor restored dark state",
        restored_state,
        mismatches,
    );

    for (label, actual, ordinary) in [
        (
            "stacked light Elisp",
            &light_elisp_actual,
            &ordinary.light_elisp,
        ),
        ("stacked light Org", &light_org_actual, &ordinary.light_org),
        (
            "stacked light Diff",
            &light_diff_actual,
            &ordinary.light_diff,
        ),
        (
            "stacked light properties",
            &light_properties_actual,
            &ordinary.light_properties,
        ),
        (
            "restored dark Elisp",
            &restored_elisp_actual,
            &ordinary.dark_elisp,
        ),
        (
            "restored dark Org",
            &restored_org_actual,
            &ordinary.dark_org,
        ),
        (
            "restored dark Diff",
            &restored_diff_actual,
            &ordinary.dark_diff,
        ),
        (
            "restored dark properties",
            &restored_properties_actual,
            &ordinary.dark_properties,
        ),
    ] {
        if actual != ordinary {
            mismatches.push(format!(
                "truecolor {label} differs from its ordinary rendering\nORDINARY:\n{ordinary}\nSTACKED/RESTORED:\n{actual}"
            ));
        }
    }
    let without_enabled = |state: &str| {
        state
            .lines()
            .filter(|line| !line.starts_with("ENABLED "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (label, actual, ordinary) in [
        (
            "stacked light state",
            &light_state_actual,
            &ordinary.light_state,
        ),
        (
            "restored dark state",
            &restored_state_actual,
            &ordinary.dark_state,
        ),
    ] {
        if without_enabled(actual) != without_enabled(ordinary) {
            mismatches.push(format!(
                "truecolor {label} differs beyond the intentional enabled stack\nORDINARY:\n{ordinary}\nSTACKED/RESTORED:\n{actual}"
            ));
        }
    }
}

fn run_profile(
    label: &str,
    packages: &PreparedPackageSet,
    display: DisplayEnvOverride<'_>,
    body: impl FnOnce(&mut PackageTuiPair, &mut Vec<String>),
) -> Result<(), String> {
    let mut pair = spawn_profile(label, packages, display)?;
    let mut mismatches = Vec::new();
    let body_result = catch_phase(&format!("{label} body"), || {
        wait_for_boot_both(&mut pair);
        body(&mut pair, &mut mismatches);
    });
    let cleanup_result = catch_phase(&format!("{label} cleanup"), || {
        finish(&mut pair, &mut mismatches)
    });
    let mut failures = [body_result.err(), cleanup_result.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    failures.extend(mismatches);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

fn default_org_consumer(packages: &PreparedPackageSet) -> Result<(), String> {
    run_profile(
        "gruvbox-theme-default-org-consumer",
        packages,
        DisplayEnvOverride::Set {
            key: "COLORTERM",
            value: "truecolor",
        },
        |pair, mismatches| {
            record_pair(
                pair,
                "default Org consumer capability",
                expect![[r#"
                    CAP TERM "dumb"
                    CAP COLORTERM "truecolor"
                    CAP CELLS 16777216
                    CAP VISUAL static-color
                    CAP DISPLAY color
                    CAP GRAPHIC nil
                    CAP TRUECOLOR t
                    CAP COLOR256 t
                    CAP ORG-COMPILED t
                    CAP GNUS-BEFORE nil
                    CAP LOAD-SUFFIXES (".el")
                    THEMES-KNOWN t
                    GRUVBOX-TUI-BOOT"#]],
                mismatches,
            );
            invoke_both(
                pair,
                "gt357-configure-default-org",
                "GRUVBOX-DEFAULT-ORG-READY",
            );
            record_pair(
                pair,
                "default Org module configuration",
                expect![[r#"
                    DEFAULT-ORG MODULES-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                    DEFAULT-ORG MODULES-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                    DEFAULT-ORG GNUS (nil nil nil)
                    GRUVBOX-DEFAULT-ORG-READY"#]],
                mismatches,
            );
            initialize_consumer(
                pair,
                "default Org",
                expect![[r#"
                CONSUMER MODULES-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                CONSUMER MODULES-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                CONSUMER BEFORE (nil nil nil)
                CONSUMER THEME (gruvbox-dark-medium)
                CONSUMER OUTCOME (:value returned)
                CONSUMER SOURCE "gnus-sum.elc" t
                CONSUMER AFTER (t t t)
                CONSUMER INHERIT (gnus-group-mail-1 gnus-group-news-low)
                CONSUMER SUFFIXES (".el")
                GRUVBOX-CONSUMER-READY"#]],
                mismatches,
            );
            invoke_both(pair, "gt357-show-org", "Plan Ω");
            record_grid(
                pair,
                "default Org rendered consumer",
                &[
                    "#+title: Plan Ω",
                    "TODO Ship release",
                    "DONE Verify rollback",
                    "A link and =code=.",
                    "#+begin_src",
                    "message \"ship\"",
                    "#+end_src",
                ],
                expect![[r#"
                    [0;38;2;124;111;100;48;2;40;40;40m#+title:[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;69;133;136;48;2;40;40;40mPlan Ω[0m
                    [0;38;2;131;165;152;48;2;40;40;40m* [0;1;38;2;251;73;51;48;2;40;40;40mTODO[0;38;2;131;165;152;48;2;40;40;40m Ship release[0m
                    [0;38;2;250;189;47;48;2;40;40;40m** [0;1;38;2;142;192;124;48;2;40;40;40mDONE[0;38;2;250;189;47;48;2;40;40;40m [0;38;2;142;192;124;48;2;40;40;40mVerify rollback[0m
                    [0;38;2;235;219;178;48;2;40;40;40mA [0;4;38;2;104;157;106;48;2;40;40;40mlink[0;38;2;235;219;178;48;2;40;40;40m and [0;38;2;124;111;100;48;2;40;40;40m=code=[0;38;2;235;219;178;48;2;40;40;40m.[0m
                    [0;38;2;235;219;178;48;2;60;56;54m#+begin_src emacs-lisp[0m
                    [0;38;2;235;219;178;48;2;50;48;47m(message [0;38;2;184;187;38;48;2;50;48;47m"ship"[0;38;2;235;219;178;48;2;50;48;47m)[0m
                    [0;38;2;235;219;178;48;2;60;56;54m#+end_src[0m
                "#]],
                mismatches,
            );
        },
    )
}

fn truecolor(packages: &PreparedPackageSet) -> Result<(), String> {
    run_profile(
        "gruvbox-theme-truecolor",
        packages,
        DisplayEnvOverride::Set {
            key: "COLORTERM",
            value: "truecolor",
        },
        |pair, mismatches| {
            record_pair(
                pair,
                "truecolor capability",
                expect![[r#"
                    CAP TERM "dumb"
                    CAP COLORTERM "truecolor"
                    CAP CELLS 16777216
                    CAP VISUAL static-color
                    CAP DISPLAY color
                    CAP GRAPHIC nil
                    CAP TRUECOLOR t
                    CAP COLOR256 t
                    CAP ORG-COMPILED t
                    CAP GNUS-BEFORE nil
                    CAP LOAD-SUFFIXES (".el")
                    THEMES-KNOWN t
                    GRUVBOX-TUI-BOOT"#]],
                mismatches,
            );
            invoke_both(pair, "gt357-configure-core-org", "GRUVBOX-CORE-ORG-READY");
            record_pair(
                pair,
                "truecolor core Org configuration",
                expect![[r#"
                    CORE-ORG BEFORE-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                    CORE-ORG BEFORE-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                    CORE-ORG AFTER nil
                    CORE-ORG GNUS (nil nil nil)
                    GRUVBOX-CORE-ORG-READY"#]],
                mismatches,
            );
            initialize_consumer(
                pair,
                "truecolor core",
                expect![[r#"
                CONSUMER MODULES-1 nil
                CONSUMER MODULES-2 nil
                CONSUMER BEFORE (nil nil nil)
                CONSUMER THEME (gruvbox-dark-medium)
                CONSUMER OUTCOME (:value returned)
                CONSUMER SOURCE nil nil
                CONSUMER AFTER (nil nil nil)
                CONSUMER INHERIT nil
                CONSUMER SUFFIXES (".el")
                GRUVBOX-CONSUMER-READY"#]],
                mismatches,
            );
            record_orderless_completion(
                pair,
                expect![[r#"
                [0;1;38;2;102;153;157;48;2;40;40;40malpha[0;38;2;235;219;178;48;2;40;40;40m [0;1;38;2;214;93;14;48;2;40;40;40mb[0;38;2;235;219;178;48;2;40;40;40meta [0;1;38;2;142;192;124;48;2;40;40;40mgam[0;38;2;235;219;178;48;2;40;40;40mma [0;1;38;2;215;153;33;48;2;40;40;40mdel[0;38;2;235;219;178;48;2;40;40;40mta[0m
            "#]],
                expect![[r##"
                ORDERLESS CHOICE "alpha beta gamma delta"
                ORDERLESS FINAL-INPUT "alpha beta gamma delta"
                ORDERLESS HISTORY-HEAD "alpha beta gamma delta"
                ORDERLESS MINIBUFFER nil
                ORDERLESS RUN ("alpha" orderless-match-face-0)
                ORDERLESS RUN (" " nil)
                ORDERLESS RUN ("b" orderless-match-face-1)
                ORDERLESS RUN ("eta " nil)
                ORDERLESS RUN ("gam" orderless-match-face-2)
                ORDERLESS RUN ("ma " nil)
                ORDERLESS RUN ("del" orderless-match-face-3)
                ORDERLESS RUN ("ta" nil)
                ORDERLESS FACE 0 "#66999D" "#66999D" bold bold
                ORDERLESS FACE 1 "#d65d0e" "#d65d0e" bold bold
                ORDERLESS FACE 2 "#8ec07c" "#8ec07c" bold bold
                ORDERLESS FACE 3 "#d79921" "#d79921" bold bold
                GRUVBOX-ORDERLESS-READY"##]],
                mismatches,
            );
            assert_matrix(
                pair,
                "truecolor seven-theme matrix",
                expect![[r##"
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox
                    ENABLED (gruvbox)
                    MODE dark
                    FACE default :foreground "#ebdbb2" "#ebdbb2"
                    FACE default :background "#282828" "#282828"
                    FACE keyword :foreground "#fb4933" "#fb4933"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#b8bb26" "#b8bb26"
                    FACE org-link :foreground "#427b58" "#427b58"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#b8bb26" "#b8bb26"
                    FACE diff-added :background unspecified "#282828"
                    FACE diff-removed :foreground "#fb4934" "#fb4934"
                    FACE diff-removed :background unspecified "#282828"
                    FACE diff-context :foreground "#ebdbb2" "#ebdbb2"
                    FACE diff-context :background "#3c3836" "#3c3836"
                    FACE mode-line-inactive :foreground "#a89984" "#a89984"
                    FACE mode-line-inactive :background "#3c3836" "#3c3836"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ebdbb2"
                    FACE region :background "#504945" "#504945"
                    FACE hl-line :foreground unspecified "#ebdbb2"
                    FACE hl-line :background "#3c3836" "#3c3836"
                    FACE cursor :background "#ebdbb2" "#ebdbb2"
                    FACE orderless-0 :foreground "#66999D" "#66999D"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#b57614" "#b57614"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4934"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "gruvbox-tui-baseline-light"
                    VAR PDF-DARK "gruvbox-tui-baseline-dark"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-dark-hard
                    ENABLED (gruvbox-dark-hard)
                    MODE dark
                    FACE default :foreground "#ebdbb2" "#ebdbb2"
                    FACE default :background "#1d2021" "#1d2021"
                    FACE keyword :foreground "#fb4933" "#fb4933"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#b8bb26" "#b8bb26"
                    FACE org-link :foreground "#689d6a" "#689d6a"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#b8bb26" "#b8bb26"
                    FACE diff-added :background unspecified "#1d2021"
                    FACE diff-removed :foreground "#fb4934" "#fb4934"
                    FACE diff-removed :background unspecified "#1d2021"
                    FACE diff-context :foreground "#ebdbb2" "#ebdbb2"
                    FACE diff-context :background "#3c3836" "#3c3836"
                    FACE mode-line-inactive :foreground "#a89984" "#a89984"
                    FACE mode-line-inactive :background "#3c3836" "#3c3836"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ebdbb2"
                    FACE region :background "#504945" "#504945"
                    FACE hl-line :foreground unspecified "#ebdbb2"
                    FACE hl-line :background "#3c3836" "#3c3836"
                    FACE cursor :background "#ebdbb2" "#ebdbb2"
                    FACE orderless-0 :foreground "#66999D" "#66999D"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#d79921" "#d79921"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4933"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#fdf4c1"
                    VAR PDF-DARK "#1d2021"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-dark-medium
                    ENABLED (gruvbox-dark-medium)
                    MODE dark
                    FACE default :foreground "#ebdbb2" "#ebdbb2"
                    FACE default :background "#282828" "#282828"
                    FACE keyword :foreground "#fb4933" "#fb4933"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#b8bb26" "#b8bb26"
                    FACE org-link :foreground "#689d6a" "#689d6a"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#b8bb26" "#b8bb26"
                    FACE diff-added :background unspecified "#282828"
                    FACE diff-removed :foreground "#fb4933" "#fb4933"
                    FACE diff-removed :background unspecified "#282828"
                    FACE diff-context :foreground "#ebdbb2" "#ebdbb2"
                    FACE diff-context :background "#3c3836" "#3c3836"
                    FACE mode-line-inactive :foreground "#a89984" "#a89984"
                    FACE mode-line-inactive :background "#3c3836" "#3c3836"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ebdbb2"
                    FACE region :background "#504945" "#504945"
                    FACE hl-line :foreground unspecified "#ebdbb2"
                    FACE hl-line :background "#3c3836" "#3c3836"
                    FACE cursor :background "#ebdbb2" "#ebdbb2"
                    FACE orderless-0 :foreground "#66999D" "#66999D"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#d79921" "#d79921"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4933"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#fdf4c1"
                    VAR PDF-DARK "#282828"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-dark-soft
                    ENABLED (gruvbox-dark-soft)
                    MODE dark
                    FACE default :foreground "#ebdbb2" "#ebdbb2"
                    FACE default :background "#32302f" "#32302f"
                    FACE keyword :foreground "#fb4933" "#fb4933"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#b8bb26" "#b8bb26"
                    FACE org-link :foreground "#689d6a" "#689d6a"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#b8bb26" "#b8bb26"
                    FACE diff-added :background unspecified "#32302f"
                    FACE diff-removed :foreground "#fb4933" "#fb4933"
                    FACE diff-removed :background unspecified "#32302f"
                    FACE diff-context :foreground "#ebdbb2" "#ebdbb2"
                    FACE diff-context :background "#3c3836" "#3c3836"
                    FACE mode-line-inactive :foreground "#a89984" "#a89984"
                    FACE mode-line-inactive :background "#3c3836" "#3c3836"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ebdbb2"
                    FACE region :background "#504945" "#504945"
                    FACE hl-line :foreground unspecified "#ebdbb2"
                    FACE hl-line :background "#3c3836" "#3c3836"
                    FACE cursor :background "#ebdbb2" "#ebdbb2"
                    FACE orderless-0 :foreground "#66999D" "#66999D"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#d79921" "#d79921"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4933"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#fdf4c1"
                    VAR PDF-DARK "#32302f"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-light-hard
                    ENABLED (gruvbox-light-hard)
                    MODE light
                    FACE default :foreground "#3c3836" "#3c3836"
                    FACE default :background "#f9f5d7" "#f9f5d7"
                    FACE keyword :foreground "#9d0006" "#9d0006"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#79740e" "#79740e"
                    FACE org-link :foreground "#689d6a" "#689d6a"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#79740e" "#79740e"
                    FACE diff-added :background unspecified "#f9f5d7"
                    FACE diff-removed :foreground "#9d0006" "#9d0006"
                    FACE diff-removed :background unspecified "#f9f5d7"
                    FACE diff-context :foreground "#3c3836" "#3c3836"
                    FACE diff-context :background "#ebdbb2" "#ebdbb2"
                    FACE mode-line-inactive :foreground "#7c6f64" "#7c6f64"
                    FACE mode-line-inactive :background "#ebdbb2" "#ebdbb2"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#3c3836"
                    FACE region :background "#d5c4a1" "#d5c4a1"
                    FACE hl-line :foreground unspecified "#3c3836"
                    FACE hl-line :background "#ebdbb2" "#ebdbb2"
                    FACE cursor :background "#3c3836" "#3c3836"
                    FACE orderless-0 :foreground "#66999D" "#66999D"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#d79921" "#d79921"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#ebdbb2"
                    VAR ANSI 1 "#cc241d"
                    VAR ANSI 2 "#98971a"
                    VAR ANSI 3 "#d79921"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#458588"
                    VAR ANSI 5 "#b16286"
                    VAR ANSI 6 "#689d6a"
                    VAR ANSI 7 "#3c3836"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#282828"
                    VAR PDF-DARK "#f9f5d7"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-light-medium
                    ENABLED (gruvbox-light-medium)
                    MODE light
                    FACE default :foreground "#3c3836" "#3c3836"
                    FACE default :background "#fbf1c7" "#fbf1c7"
                    FACE keyword :foreground "#9d0006" "#9d0006"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#79740e" "#79740e"
                    FACE org-link :foreground "#689d6a" "#689d6a"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#79740e" "#79740e"
                    FACE diff-added :background unspecified "#fbf1c7"
                    FACE diff-removed :foreground "#9d0006" "#9d0006"
                    FACE diff-removed :background unspecified "#fbf1c7"
                    FACE diff-context :foreground "#3c3836" "#3c3836"
                    FACE diff-context :background "#ebdbb2" "#ebdbb2"
                    FACE mode-line-inactive :foreground "#7c6f64" "#7c6f64"
                    FACE mode-line-inactive :background "#ebdbb2" "#ebdbb2"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#3c3836"
                    FACE region :background "#d5c4a1" "#d5c4a1"
                    FACE hl-line :foreground unspecified "#3c3836"
                    FACE hl-line :background "#ebdbb2" "#ebdbb2"
                    FACE cursor :background "#3c3836" "#3c3836"
                    FACE orderless-0 :foreground "#66999D" "#66999D"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#d79921" "#d79921"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#ebdbb2"
                    VAR ANSI 1 "#cc241d"
                    VAR ANSI 2 "#98971a"
                    VAR ANSI 3 "#d79921"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#458588"
                    VAR ANSI 5 "#b16286"
                    VAR ANSI 6 "#689d6a"
                    VAR ANSI 7 "#3c3836"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#282828"
                    VAR PDF-DARK "#fbf1c7"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-light-soft
                    ENABLED (gruvbox-light-soft)
                    MODE light
                    FACE default :foreground "#3c3836" "#3c3836"
                    FACE default :background "#f2e5bc" "#f2e5bc"
                    FACE keyword :foreground "#9d0006" "#9d0006"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#79740e" "#79740e"
                    FACE org-link :foreground "#689d6a" "#689d6a"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#79740e" "#79740e"
                    FACE diff-added :background unspecified "#f2e5bc"
                    FACE diff-removed :foreground "#9d0006" "#9d0006"
                    FACE diff-removed :background unspecified "#f2e5bc"
                    FACE diff-context :foreground "#3c3836" "#3c3836"
                    FACE diff-context :background "#ebdbb2" "#ebdbb2"
                    FACE mode-line-inactive :foreground "#7c6f64" "#7c6f64"
                    FACE mode-line-inactive :background "#ebdbb2" "#ebdbb2"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#3c3836"
                    FACE region :background "#d5c4a1" "#d5c4a1"
                    FACE hl-line :foreground unspecified "#3c3836"
                    FACE hl-line :background "#ebdbb2" "#ebdbb2"
                    FACE cursor :background "#3c3836" "#3c3836"
                    FACE orderless-0 :foreground "#66999D" "#66999D"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#d79921" "#d79921"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#ebdbb2"
                    VAR ANSI 1 "#9d0006"
                    VAR ANSI 2 "#79740e"
                    VAR ANSI 3 "#b57614"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#076678"
                    VAR ANSI 5 "#8f3f71"
                    VAR ANSI 6 "#427b58"
                    VAR ANSI 7 "#3c3836"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#282828"
                    VAR PDF-DARK "#f2e5bc"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY"##]],
                mismatches,
            );
            let ordinary = exercise_rendering(
                pair,
                "truecolor",
                RenderingExpectations {
                    dark_elisp: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m; comment Ω[0m
                        [0;38;2;235;219;178;48;2;40;40;40m([0;38;2;251;73;51;48;2;40;40;40mdefun[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;250;189;47;48;2;40;40;40mgreet[0;38;2;235;219;178;48;2;40;40;40m (name)[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  [0;38;2;184;187;38;48;2;40;40;40m"Doc."[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  ([0;38;2;251;73;51;48;2;40;40;40mif[0;38;2;235;219;178;48;2;40;40;40m name (message [0;38;2;184;187;38;48;2;40;40;40m"Hello %s"[0;38;2;235;219;178;48;2;40;40;40m name) nil))[0m
                    "#]],
                    dark_org: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m#+title:[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;69;133;136;48;2;40;40;40mPlan Ω[0m
                        [0;38;2;131;165;152;48;2;40;40;40m* [0;1;38;2;251;73;51;48;2;40;40;40mTODO[0;38;2;131;165;152;48;2;40;40;40m Ship release[0m
                        [0;38;2;250;189;47;48;2;40;40;40m** [0;1;38;2;142;192;124;48;2;40;40;40mDONE[0;38;2;250;189;47;48;2;40;40;40m [0;38;2;142;192;124;48;2;40;40;40mVerify rollback[0m
                        [0;38;2;235;219;178;48;2;40;40;40mA [0;4;38;2;104;157;106;48;2;40;40;40mlink[0;38;2;235;219;178;48;2;40;40;40m and [0;38;2;124;111;100;48;2;40;40;40m=code=[0;38;2;235;219;178;48;2;40;40;40m.[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+begin_src emacs-lisp[0m
                        [0;38;2;235;219;178;48;2;50;48;47m(message [0;38;2;184;187;38;48;2;50;48;47m"ship"[0;38;2;235;219;178;48;2;50;48;47m)[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+end_src[0m
                    "#]],
                    dark_diff: expect![[r#"
                        [0;38;2;235;219;178;48;2;60;56;54mdiff --git a/a.el b/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m--- [0;38;2;235;219;178;48;2;80;73;69ma/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m+++ [0;38;2;235;219;178;48;2;80;73;69mb/a.el[0m
                        [0;38;2;235;219;178;48;2;80;73;69m@@ -1 +1 @@[0m
                        [0;38;2;251;73;51;48;2;40;40;40m-([0;38;2;235;219;178;48;2;204;36;29mold[0;38;2;251;73;51;48;2;40;40;40m)[0m
                        [0;38;2;184;187;38;48;2;40;40;40m+([0;38;2;235;219;178;48;2;152;151;26mnew[0;38;2;184;187;38;48;2;40;40;40m)[0m
                    "#]],
                    dark_properties: expect![[r##"
                        PROPERTIES gruvbox-dark-medium E PAGE 1/1
                        PROPERTY-COUNT E 13
                        RUN E ("; " font-lock-comment-delimiter-face)
                        RUN E ("comment Ω\n" font-lock-comment-face)
                        RUN E ("(" nil)
                        RUN E ("defun" font-lock-keyword-face)
                        RUN E (" " nil)
                        RUN E ("greet" font-lock-function-name-face)
                        RUN E (" (name)\n  " nil)
                        RUN E ("\"Doc.\"" font-lock-doc-face)
                        RUN E ("\n  (" nil)
                        RUN E ("if" font-lock-keyword-face)
                        RUN E (" name (message " nil)
                        RUN E ("\"Hello %s\"" font-lock-string-face)
                        RUN E (" name) nil))\n" nil)
                        GRUVBOX-PROPERTIES-E-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-E-READY
                        --
                        PROPERTIES gruvbox-dark-medium O PAGE 1/2
                        PROPERTY-COUNT O 21
                        RUN O ("#+title:" org-document-info-keyword)
                        RUN O (" " nil)
                        RUN O ("Plan Ω\n" org-document-title)
                        RUN O ("* " org-level-1)
                        RUN O ("TODO" (org-todo org-level-1))
                        RUN O (" Ship release" org-level-1)
                        RUN O ("\n" nil)
                        RUN O ("** " org-level-2)
                        RUN O ("DONE" (org-done org-level-2))
                        RUN O (" " org-level-2)
                        RUN O ("Verify rollback" (org-headline-done org-level-2))
                        RUN O ("\nA " nil)
                        RUN O ("[[https://example.invalid][link]]" org-link)
                        RUN O (" and " nil)
                        RUN O ("=code=" (org-verbatim))
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 1/2
                        --
                        PROPERTIES gruvbox-dark-medium O PAGE 2/2
                        PROPERTY-COUNT O 21
                        RUN O (".\n" nil)
                        RUN O ("#+begin_src emacs-lisp\n" org-block-begin-line)
                        RUN O ("(message " (org-block))
                        RUN O ("\"ship\"" (font-lock-string-face org-block))
                        RUN O (")\n" (org-block))
                        RUN O ("#+end_src\n" org-block-end-line)
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 2/2
                        GRUVBOX-PROPERTIES-O-READY
                        --
                        PROPERTIES gruvbox-dark-medium D PAGE 1/1
                        PROPERTY-COUNT D 12
                        RUN D ("diff --git a/a.el b/a.el\n--- " diff-header)
                        RUN D ("a/a.el" (diff-file-header diff-header))
                        RUN D ("\n+++ " diff-header)
                        RUN D ("b/a.el" (diff-file-header diff-header))
                        RUN D ("\n" diff-header)
                        RUN D ("@@ -1 +1 @@" diff-hunk-header)
                        RUN D ("\n" nil)
                        RUN D ("-" diff-indicator-removed)
                        RUN D ("(old)\n" diff-removed)
                        RUN D ("+" diff-indicator-added)
                        RUN D ("(new)\n" diff-added)
                        RUN D (" context\n" diff-context)
                        GRUVBOX-PROPERTIES-D-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-D-READY"##]],
                    dark_state: expect![[r##"
                        GRUVBOX-THEME-PAGE 1/3
                        THEME gruvbox-dark-medium
                        ENABLED (gruvbox-dark-medium)
                        MODE dark
                        FACE default :foreground "#ebdbb2" "#ebdbb2"
                        FACE default :background "#282828" "#282828"
                        FACE keyword :foreground "#fb4933" "#fb4933"
                        FACE keyword :weight normal normal
                        FACE string :foreground "#b8bb26" "#b8bb26"
                        FACE org-link :foreground "#689d6a" "#689d6a"
                        FACE org-link :underline t t
                        FACE diff-added :foreground "#b8bb26" "#b8bb26"
                        FACE diff-added :background unspecified "#282828"
                        FACE diff-removed :foreground "#fb4933" "#fb4933"
                        FACE diff-removed :background unspecified "#282828"
                        FACE diff-context :foreground "#ebdbb2" "#ebdbb2"
                        FACE diff-context :background "#3c3836" "#3c3836"
                        FACE mode-line-inactive :foreground "#a89984" "#a89984"
                        FACE mode-line-inactive :background "#3c3836" "#3c3836"
                        GRUVBOX-THEME-PAGE-DONE 1/3
                        --
                        GRUVBOX-THEME-PAGE 2/3
                        FACE region :foreground unspecified "#ebdbb2"
                        FACE region :background "#504945" "#504945"
                        FACE hl-line :foreground unspecified "#ebdbb2"
                        FACE hl-line :background "#3c3836" "#3c3836"
                        FACE cursor :background "#ebdbb2" "#ebdbb2"
                        FACE orderless-0 :foreground "#66999D" "#66999D"
                        FACE orderless-0 :weight bold bold
                        FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                        FACE orderless-1 :weight bold bold
                        FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                        FACE orderless-2 :weight bold bold
                        FACE orderless-3 :foreground "#d79921" "#d79921"
                        FACE orderless-3 :weight bold bold
                        VAR ANSI-BOUND t
                        VAR ANSI 0 "#3c3836"
                        VAR ANSI 1 "#fb4933"
                        VAR ANSI 2 "#b8bb26"
                        VAR ANSI 3 "#fabd2f"
                        GRUVBOX-THEME-PAGE-DONE 2/3
                        --
                        GRUVBOX-THEME-PAGE 3/3
                        VAR ANSI 4 "#83a598"
                        VAR ANSI 5 "#d3869b"
                        VAR ANSI 6 "#8ec07c"
                        VAR ANSI 7 "#ebdbb2"
                        VAR PDF-BOUND t
                        VAR PDF-LIGHT "#fdf4c1"
                        VAR PDF-DARK "#282828"
                        GRUVBOX-THEME-PAGE-DONE 3/3
                        GRUVBOX-THEME-READY"##]],
                    light_elisp: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m; comment Ω[0m
                        [0;38;2;60;56;54;48;2;251;241;199m([0;38;2;157;0;6;48;2;251;241;199mdefun[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;181;118;20;48;2;251;241;199mgreet[0;38;2;60;56;54;48;2;251;241;199m (name)[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  [0;38;2;121;116;14;48;2;251;241;199m"Doc."[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  ([0;38;2;157;0;6;48;2;251;241;199mif[0;38;2;60;56;54;48;2;251;241;199m name (message [0;38;2;121;116;14;48;2;251;241;199m"Hello %s"[0;38;2;60;56;54;48;2;251;241;199m name) nil))[0m
                    "#]],
                    light_org: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m#+title:[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;69;133;136;48;2;251;241;199mPlan Ω[0m
                        [0;38;2;7;102;120;48;2;251;241;199m* [0;1;38;2;157;0;6;48;2;251;241;199mTODO[0;38;2;7;102;120;48;2;251;241;199m Ship release[0m
                        [0;38;2;181;118;20;48;2;251;241;199m** [0;1;38;2;66;123;88;48;2;251;241;199mDONE[0;38;2;181;118;20;48;2;251;241;199m [0;38;2;66;123;88;48;2;251;241;199mVerify rollback[0m
                        [0;38;2;60;56;54;48;2;251;241;199mA [0;4;38;2;104;157;106;48;2;251;241;199mlink[0;38;2;60;56;54;48;2;251;241;199m and [0;38;2;168;153;132;48;2;251;241;199m=code=[0;38;2;60;56;54;48;2;251;241;199m.[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+begin_src emacs-lisp[0m
                        [0;38;2;60;56;54;48;2;242;229;188m(message [0;38;2;121;116;14;48;2;242;229;188m"ship"[0;38;2;60;56;54;48;2;242;229;188m)[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+end_src[0m
                    "#]],
                    light_diff: expect![[r#"
                        [0;38;2;60;56;54;48;2;235;219;178mdiff --git a/a.el b/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m--- [0;38;2;60;56;54;48;2;213;196;161ma/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m+++ [0;38;2;60;56;54;48;2;213;196;161mb/a.el[0m
                        [0;38;2;60;56;54;48;2;213;196;161m@@ -1 +1 @@[0m
                        [0;38;2;157;0;6;48;2;251;241;199m-([0;38;2;60;56;54;48;2;204;36;29mold[0;38;2;157;0;6;48;2;251;241;199m)[0m
                        [0;38;2;121;116;14;48;2;251;241;199m+([0;38;2;60;56;54;48;2;152;151;26mnew[0;38;2;121;116;14;48;2;251;241;199m)[0m
                    "#]],
                    light_properties: expect![[r##"
                        PROPERTIES gruvbox-light-medium E PAGE 1/1
                        PROPERTY-COUNT E 13
                        RUN E ("; " font-lock-comment-delimiter-face)
                        RUN E ("comment Ω\n" font-lock-comment-face)
                        RUN E ("(" nil)
                        RUN E ("defun" font-lock-keyword-face)
                        RUN E (" " nil)
                        RUN E ("greet" font-lock-function-name-face)
                        RUN E (" (name)\n  " nil)
                        RUN E ("\"Doc.\"" font-lock-doc-face)
                        RUN E ("\n  (" nil)
                        RUN E ("if" font-lock-keyword-face)
                        RUN E (" name (message " nil)
                        RUN E ("\"Hello %s\"" font-lock-string-face)
                        RUN E (" name) nil))\n" nil)
                        GRUVBOX-PROPERTIES-E-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-E-READY
                        --
                        PROPERTIES gruvbox-light-medium O PAGE 1/2
                        PROPERTY-COUNT O 21
                        RUN O ("#+title:" org-document-info-keyword)
                        RUN O (" " nil)
                        RUN O ("Plan Ω\n" org-document-title)
                        RUN O ("* " org-level-1)
                        RUN O ("TODO" (org-todo org-level-1))
                        RUN O (" Ship release" org-level-1)
                        RUN O ("\n" nil)
                        RUN O ("** " org-level-2)
                        RUN O ("DONE" (org-done org-level-2))
                        RUN O (" " org-level-2)
                        RUN O ("Verify rollback" (org-headline-done org-level-2))
                        RUN O ("\nA " nil)
                        RUN O ("[[https://example.invalid][link]]" org-link)
                        RUN O (" and " nil)
                        RUN O ("=code=" (org-verbatim))
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 1/2
                        --
                        PROPERTIES gruvbox-light-medium O PAGE 2/2
                        PROPERTY-COUNT O 21
                        RUN O (".\n" nil)
                        RUN O ("#+begin_src emacs-lisp\n" org-block-begin-line)
                        RUN O ("(message " (org-block))
                        RUN O ("\"ship\"" (font-lock-string-face org-block))
                        RUN O (")\n" (org-block))
                        RUN O ("#+end_src\n" org-block-end-line)
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 2/2
                        GRUVBOX-PROPERTIES-O-READY
                        --
                        PROPERTIES gruvbox-light-medium D PAGE 1/1
                        PROPERTY-COUNT D 12
                        RUN D ("diff --git a/a.el b/a.el\n--- " diff-header)
                        RUN D ("a/a.el" (diff-file-header diff-header))
                        RUN D ("\n+++ " diff-header)
                        RUN D ("b/a.el" (diff-file-header diff-header))
                        RUN D ("\n" diff-header)
                        RUN D ("@@ -1 +1 @@" diff-hunk-header)
                        RUN D ("\n" nil)
                        RUN D ("-" diff-indicator-removed)
                        RUN D ("(old)\n" diff-removed)
                        RUN D ("+" diff-indicator-added)
                        RUN D ("(new)\n" diff-added)
                        RUN D (" context\n" diff-context)
                        GRUVBOX-PROPERTIES-D-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-D-READY"##]],
                    light_state: expect![[r##"
                        GRUVBOX-THEME-PAGE 1/3
                        THEME gruvbox-light-medium
                        ENABLED (gruvbox-light-medium)
                        MODE light
                        FACE default :foreground "#3c3836" "#3c3836"
                        FACE default :background "#fbf1c7" "#fbf1c7"
                        FACE keyword :foreground "#9d0006" "#9d0006"
                        FACE keyword :weight normal normal
                        FACE string :foreground "#79740e" "#79740e"
                        FACE org-link :foreground "#689d6a" "#689d6a"
                        FACE org-link :underline t t
                        FACE diff-added :foreground "#79740e" "#79740e"
                        FACE diff-added :background unspecified "#fbf1c7"
                        FACE diff-removed :foreground "#9d0006" "#9d0006"
                        FACE diff-removed :background unspecified "#fbf1c7"
                        FACE diff-context :foreground "#3c3836" "#3c3836"
                        FACE diff-context :background "#ebdbb2" "#ebdbb2"
                        FACE mode-line-inactive :foreground "#7c6f64" "#7c6f64"
                        FACE mode-line-inactive :background "#ebdbb2" "#ebdbb2"
                        GRUVBOX-THEME-PAGE-DONE 1/3
                        --
                        GRUVBOX-THEME-PAGE 2/3
                        FACE region :foreground unspecified "#3c3836"
                        FACE region :background "#d5c4a1" "#d5c4a1"
                        FACE hl-line :foreground unspecified "#3c3836"
                        FACE hl-line :background "#ebdbb2" "#ebdbb2"
                        FACE cursor :background "#3c3836" "#3c3836"
                        FACE orderless-0 :foreground "#66999D" "#66999D"
                        FACE orderless-0 :weight bold bold
                        FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                        FACE orderless-1 :weight bold bold
                        FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                        FACE orderless-2 :weight bold bold
                        FACE orderless-3 :foreground "#d79921" "#d79921"
                        FACE orderless-3 :weight bold bold
                        VAR ANSI-BOUND t
                        VAR ANSI 0 "#ebdbb2"
                        VAR ANSI 1 "#cc241d"
                        VAR ANSI 2 "#98971a"
                        VAR ANSI 3 "#d79921"
                        GRUVBOX-THEME-PAGE-DONE 2/3
                        --
                        GRUVBOX-THEME-PAGE 3/3
                        VAR ANSI 4 "#458588"
                        VAR ANSI 5 "#b16286"
                        VAR ANSI 6 "#689d6a"
                        VAR ANSI 7 "#3c3836"
                        VAR PDF-BOUND t
                        VAR PDF-LIGHT "#282828"
                        VAR PDF-DARK "#fbf1c7"
                        GRUVBOX-THEME-PAGE-DONE 3/3
                        GRUVBOX-THEME-READY"##]],
                },
                mismatches,
            );
            exercise_stack_rendering(
                pair,
                &ordinary,
                StackRenderingExpectations {
                    light_elisp: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m; comment Ω[0m
                        [0;38;2;60;56;54;48;2;251;241;199m([0;38;2;157;0;6;48;2;251;241;199mdefun[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;181;118;20;48;2;251;241;199mgreet[0;38;2;60;56;54;48;2;251;241;199m (name)[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  [0;38;2;121;116;14;48;2;251;241;199m"Doc."[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  ([0;38;2;157;0;6;48;2;251;241;199mif[0;38;2;60;56;54;48;2;251;241;199m name (message [0;38;2;121;116;14;48;2;251;241;199m"Hello %s"[0;38;2;60;56;54;48;2;251;241;199m name) nil))[0m
                    "#]],
                    light_org: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m#+title:[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;69;133;136;48;2;251;241;199mPlan Ω[0m
                        [0;38;2;7;102;120;48;2;251;241;199m* [0;1;38;2;157;0;6;48;2;251;241;199mTODO[0;38;2;7;102;120;48;2;251;241;199m Ship release[0m
                        [0;38;2;181;118;20;48;2;251;241;199m** [0;1;38;2;66;123;88;48;2;251;241;199mDONE[0;38;2;181;118;20;48;2;251;241;199m [0;38;2;66;123;88;48;2;251;241;199mVerify rollback[0m
                        [0;38;2;60;56;54;48;2;251;241;199mA [0;4;38;2;104;157;106;48;2;251;241;199mlink[0;38;2;60;56;54;48;2;251;241;199m and [0;38;2;168;153;132;48;2;251;241;199m=code=[0;38;2;60;56;54;48;2;251;241;199m.[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+begin_src emacs-lisp[0m
                        [0;38;2;60;56;54;48;2;242;229;188m(message [0;38;2;121;116;14;48;2;242;229;188m"ship"[0;38;2;60;56;54;48;2;242;229;188m)[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+end_src[0m
                    "#]],
                    light_diff: expect![[r#"
                        [0;38;2;60;56;54;48;2;235;219;178mdiff --git a/a.el b/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m--- [0;38;2;60;56;54;48;2;213;196;161ma/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m+++ [0;38;2;60;56;54;48;2;213;196;161mb/a.el[0m
                        [0;38;2;60;56;54;48;2;213;196;161m@@ -1 +1 @@[0m
                        [0;38;2;157;0;6;48;2;251;241;199m-([0;38;2;60;56;54;48;2;204;36;29mold[0;38;2;157;0;6;48;2;251;241;199m)[0m
                        [0;38;2;121;116;14;48;2;251;241;199m+([0;38;2;60;56;54;48;2;152;151;26mnew[0;38;2;121;116;14;48;2;251;241;199m)[0m
                    "#]],
                    light_properties: expect![[r##"
                        PROPERTIES gruvbox-light-medium E PAGE 1/1
                        PROPERTY-COUNT E 13
                        RUN E ("; " font-lock-comment-delimiter-face)
                        RUN E ("comment Ω\n" font-lock-comment-face)
                        RUN E ("(" nil)
                        RUN E ("defun" font-lock-keyword-face)
                        RUN E (" " nil)
                        RUN E ("greet" font-lock-function-name-face)
                        RUN E (" (name)\n  " nil)
                        RUN E ("\"Doc.\"" font-lock-doc-face)
                        RUN E ("\n  (" nil)
                        RUN E ("if" font-lock-keyword-face)
                        RUN E (" name (message " nil)
                        RUN E ("\"Hello %s\"" font-lock-string-face)
                        RUN E (" name) nil))\n" nil)
                        GRUVBOX-PROPERTIES-E-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-E-READY
                        --
                        PROPERTIES gruvbox-light-medium O PAGE 1/2
                        PROPERTY-COUNT O 21
                        RUN O ("#+title:" org-document-info-keyword)
                        RUN O (" " nil)
                        RUN O ("Plan Ω\n" org-document-title)
                        RUN O ("* " org-level-1)
                        RUN O ("TODO" (org-todo org-level-1))
                        RUN O (" Ship release" org-level-1)
                        RUN O ("\n" nil)
                        RUN O ("** " org-level-2)
                        RUN O ("DONE" (org-done org-level-2))
                        RUN O (" " org-level-2)
                        RUN O ("Verify rollback" (org-headline-done org-level-2))
                        RUN O ("\nA " nil)
                        RUN O ("[[https://example.invalid][link]]" org-link)
                        RUN O (" and " nil)
                        RUN O ("=code=" (org-verbatim))
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 1/2
                        --
                        PROPERTIES gruvbox-light-medium O PAGE 2/2
                        PROPERTY-COUNT O 21
                        RUN O (".\n" nil)
                        RUN O ("#+begin_src emacs-lisp\n" org-block-begin-line)
                        RUN O ("(message " (org-block))
                        RUN O ("\"ship\"" (font-lock-string-face org-block))
                        RUN O (")\n" (org-block))
                        RUN O ("#+end_src\n" org-block-end-line)
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 2/2
                        GRUVBOX-PROPERTIES-O-READY
                        --
                        PROPERTIES gruvbox-light-medium D PAGE 1/1
                        PROPERTY-COUNT D 12
                        RUN D ("diff --git a/a.el b/a.el\n--- " diff-header)
                        RUN D ("a/a.el" (diff-file-header diff-header))
                        RUN D ("\n+++ " diff-header)
                        RUN D ("b/a.el" (diff-file-header diff-header))
                        RUN D ("\n" diff-header)
                        RUN D ("@@ -1 +1 @@" diff-hunk-header)
                        RUN D ("\n" nil)
                        RUN D ("-" diff-indicator-removed)
                        RUN D ("(old)\n" diff-removed)
                        RUN D ("+" diff-indicator-added)
                        RUN D ("(new)\n" diff-added)
                        RUN D (" context\n" diff-context)
                        GRUVBOX-PROPERTIES-D-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-D-READY"##]],
                    light_state: expect![[r##"
                        GRUVBOX-THEME-PAGE 1/3
                        THEME gruvbox-light-medium
                        ENABLED (gruvbox-light-medium gruvbox-dark-medium)
                        MODE light
                        FACE default :foreground "#3c3836" "#3c3836"
                        FACE default :background "#fbf1c7" "#fbf1c7"
                        FACE keyword :foreground "#9d0006" "#9d0006"
                        FACE keyword :weight normal normal
                        FACE string :foreground "#79740e" "#79740e"
                        FACE org-link :foreground "#689d6a" "#689d6a"
                        FACE org-link :underline t t
                        FACE diff-added :foreground "#79740e" "#79740e"
                        FACE diff-added :background unspecified "#fbf1c7"
                        FACE diff-removed :foreground "#9d0006" "#9d0006"
                        FACE diff-removed :background unspecified "#fbf1c7"
                        FACE diff-context :foreground "#3c3836" "#3c3836"
                        FACE diff-context :background "#ebdbb2" "#ebdbb2"
                        FACE mode-line-inactive :foreground "#7c6f64" "#7c6f64"
                        FACE mode-line-inactive :background "#ebdbb2" "#ebdbb2"
                        GRUVBOX-THEME-PAGE-DONE 1/3
                        --
                        GRUVBOX-THEME-PAGE 2/3
                        FACE region :foreground unspecified "#3c3836"
                        FACE region :background "#d5c4a1" "#d5c4a1"
                        FACE hl-line :foreground unspecified "#3c3836"
                        FACE hl-line :background "#ebdbb2" "#ebdbb2"
                        FACE cursor :background "#3c3836" "#3c3836"
                        FACE orderless-0 :foreground "#66999D" "#66999D"
                        FACE orderless-0 :weight bold bold
                        FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                        FACE orderless-1 :weight bold bold
                        FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                        FACE orderless-2 :weight bold bold
                        FACE orderless-3 :foreground "#d79921" "#d79921"
                        FACE orderless-3 :weight bold bold
                        VAR ANSI-BOUND t
                        VAR ANSI 0 "#ebdbb2"
                        VAR ANSI 1 "#cc241d"
                        VAR ANSI 2 "#98971a"
                        VAR ANSI 3 "#d79921"
                        GRUVBOX-THEME-PAGE-DONE 2/3
                        --
                        GRUVBOX-THEME-PAGE 3/3
                        VAR ANSI 4 "#458588"
                        VAR ANSI 5 "#b16286"
                        VAR ANSI 6 "#689d6a"
                        VAR ANSI 7 "#3c3836"
                        VAR PDF-BOUND t
                        VAR PDF-LIGHT "#282828"
                        VAR PDF-DARK "#fbf1c7"
                        GRUVBOX-THEME-PAGE-DONE 3/3
                        GRUVBOX-THEME-READY"##]],
                    restored_elisp: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m; comment Ω[0m
                        [0;38;2;235;219;178;48;2;40;40;40m([0;38;2;251;73;51;48;2;40;40;40mdefun[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;250;189;47;48;2;40;40;40mgreet[0;38;2;235;219;178;48;2;40;40;40m (name)[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  [0;38;2;184;187;38;48;2;40;40;40m"Doc."[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  ([0;38;2;251;73;51;48;2;40;40;40mif[0;38;2;235;219;178;48;2;40;40;40m name (message [0;38;2;184;187;38;48;2;40;40;40m"Hello %s"[0;38;2;235;219;178;48;2;40;40;40m name) nil))[0m
                    "#]],
                    restored_org: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m#+title:[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;69;133;136;48;2;40;40;40mPlan Ω[0m
                        [0;38;2;131;165;152;48;2;40;40;40m* [0;1;38;2;251;73;51;48;2;40;40;40mTODO[0;38;2;131;165;152;48;2;40;40;40m Ship release[0m
                        [0;38;2;250;189;47;48;2;40;40;40m** [0;1;38;2;142;192;124;48;2;40;40;40mDONE[0;38;2;250;189;47;48;2;40;40;40m [0;38;2;142;192;124;48;2;40;40;40mVerify rollback[0m
                        [0;38;2;235;219;178;48;2;40;40;40mA [0;4;38;2;104;157;106;48;2;40;40;40mlink[0;38;2;235;219;178;48;2;40;40;40m and [0;38;2;124;111;100;48;2;40;40;40m=code=[0;38;2;235;219;178;48;2;40;40;40m.[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+begin_src emacs-lisp[0m
                        [0;38;2;235;219;178;48;2;50;48;47m(message [0;38;2;184;187;38;48;2;50;48;47m"ship"[0;38;2;235;219;178;48;2;50;48;47m)[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+end_src[0m
                    "#]],
                    restored_diff: expect![[r#"
                        [0;38;2;235;219;178;48;2;60;56;54mdiff --git a/a.el b/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m--- [0;38;2;235;219;178;48;2;80;73;69ma/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m+++ [0;38;2;235;219;178;48;2;80;73;69mb/a.el[0m
                        [0;38;2;235;219;178;48;2;80;73;69m@@ -1 +1 @@[0m
                        [0;38;2;251;73;51;48;2;40;40;40m-([0;38;2;235;219;178;48;2;204;36;29mold[0;38;2;251;73;51;48;2;40;40;40m)[0m
                        [0;38;2;184;187;38;48;2;40;40;40m+([0;38;2;235;219;178;48;2;152;151;26mnew[0;38;2;184;187;38;48;2;40;40;40m)[0m
                    "#]],
                    restored_properties: expect![[r##"
                        PROPERTIES gruvbox-dark-medium E PAGE 1/1
                        PROPERTY-COUNT E 13
                        RUN E ("; " font-lock-comment-delimiter-face)
                        RUN E ("comment Ω\n" font-lock-comment-face)
                        RUN E ("(" nil)
                        RUN E ("defun" font-lock-keyword-face)
                        RUN E (" " nil)
                        RUN E ("greet" font-lock-function-name-face)
                        RUN E (" (name)\n  " nil)
                        RUN E ("\"Doc.\"" font-lock-doc-face)
                        RUN E ("\n  (" nil)
                        RUN E ("if" font-lock-keyword-face)
                        RUN E (" name (message " nil)
                        RUN E ("\"Hello %s\"" font-lock-string-face)
                        RUN E (" name) nil))\n" nil)
                        GRUVBOX-PROPERTIES-E-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-E-READY
                        --
                        PROPERTIES gruvbox-dark-medium O PAGE 1/2
                        PROPERTY-COUNT O 21
                        RUN O ("#+title:" org-document-info-keyword)
                        RUN O (" " nil)
                        RUN O ("Plan Ω\n" org-document-title)
                        RUN O ("* " org-level-1)
                        RUN O ("TODO" (org-todo org-level-1))
                        RUN O (" Ship release" org-level-1)
                        RUN O ("\n" nil)
                        RUN O ("** " org-level-2)
                        RUN O ("DONE" (org-done org-level-2))
                        RUN O (" " org-level-2)
                        RUN O ("Verify rollback" (org-headline-done org-level-2))
                        RUN O ("\nA " nil)
                        RUN O ("[[https://example.invalid][link]]" org-link)
                        RUN O (" and " nil)
                        RUN O ("=code=" (org-verbatim))
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 1/2
                        --
                        PROPERTIES gruvbox-dark-medium O PAGE 2/2
                        PROPERTY-COUNT O 21
                        RUN O (".\n" nil)
                        RUN O ("#+begin_src emacs-lisp\n" org-block-begin-line)
                        RUN O ("(message " (org-block))
                        RUN O ("\"ship\"" (font-lock-string-face org-block))
                        RUN O (")\n" (org-block))
                        RUN O ("#+end_src\n" org-block-end-line)
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 2/2
                        GRUVBOX-PROPERTIES-O-READY
                        --
                        PROPERTIES gruvbox-dark-medium D PAGE 1/1
                        PROPERTY-COUNT D 12
                        RUN D ("diff --git a/a.el b/a.el\n--- " diff-header)
                        RUN D ("a/a.el" (diff-file-header diff-header))
                        RUN D ("\n+++ " diff-header)
                        RUN D ("b/a.el" (diff-file-header diff-header))
                        RUN D ("\n" diff-header)
                        RUN D ("@@ -1 +1 @@" diff-hunk-header)
                        RUN D ("\n" nil)
                        RUN D ("-" diff-indicator-removed)
                        RUN D ("(old)\n" diff-removed)
                        RUN D ("+" diff-indicator-added)
                        RUN D ("(new)\n" diff-added)
                        RUN D (" context\n" diff-context)
                        GRUVBOX-PROPERTIES-D-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-D-READY"##]],
                    restored_state: expect![[r##"
                        GRUVBOX-THEME-PAGE 1/3
                        THEME gruvbox-dark-medium
                        ENABLED (gruvbox-dark-medium)
                        MODE dark
                        FACE default :foreground "#ebdbb2" "#ebdbb2"
                        FACE default :background "#282828" "#282828"
                        FACE keyword :foreground "#fb4933" "#fb4933"
                        FACE keyword :weight normal normal
                        FACE string :foreground "#b8bb26" "#b8bb26"
                        FACE org-link :foreground "#689d6a" "#689d6a"
                        FACE org-link :underline t t
                        FACE diff-added :foreground "#b8bb26" "#b8bb26"
                        FACE diff-added :background unspecified "#282828"
                        FACE diff-removed :foreground "#fb4933" "#fb4933"
                        FACE diff-removed :background unspecified "#282828"
                        FACE diff-context :foreground "#ebdbb2" "#ebdbb2"
                        FACE diff-context :background "#3c3836" "#3c3836"
                        FACE mode-line-inactive :foreground "#a89984" "#a89984"
                        FACE mode-line-inactive :background "#3c3836" "#3c3836"
                        GRUVBOX-THEME-PAGE-DONE 1/3
                        --
                        GRUVBOX-THEME-PAGE 2/3
                        FACE region :foreground unspecified "#ebdbb2"
                        FACE region :background "#504945" "#504945"
                        FACE hl-line :foreground unspecified "#ebdbb2"
                        FACE hl-line :background "#3c3836" "#3c3836"
                        FACE cursor :background "#ebdbb2" "#ebdbb2"
                        FACE orderless-0 :foreground "#66999D" "#66999D"
                        FACE orderless-0 :weight bold bold
                        FACE orderless-1 :foreground "#d65d0e" "#d65d0e"
                        FACE orderless-1 :weight bold bold
                        FACE orderless-2 :foreground "#8ec07c" "#8ec07c"
                        FACE orderless-2 :weight bold bold
                        FACE orderless-3 :foreground "#d79921" "#d79921"
                        FACE orderless-3 :weight bold bold
                        VAR ANSI-BOUND t
                        VAR ANSI 0 "#3c3836"
                        VAR ANSI 1 "#fb4933"
                        VAR ANSI 2 "#b8bb26"
                        VAR ANSI 3 "#fabd2f"
                        GRUVBOX-THEME-PAGE-DONE 2/3
                        --
                        GRUVBOX-THEME-PAGE 3/3
                        VAR ANSI 4 "#83a598"
                        VAR ANSI 5 "#d3869b"
                        VAR ANSI 6 "#8ec07c"
                        VAR ANSI 7 "#ebdbb2"
                        VAR PDF-BOUND t
                        VAR PDF-LIGHT "#fdf4c1"
                        VAR PDF-DARK "#282828"
                        GRUVBOX-THEME-PAGE-DONE 3/3
                        GRUVBOX-THEME-READY"##]],
                },
                mismatches,
            );
            invoke_both(pair, "gt357-show-bold-cycle", "GRUVBOX-BOLD-READY");
            record_pair(
                pair,
                "truecolor bold reload",
                expect![[r#"
                BOLD PLAIN (normal normal)
                BOLD BEFORE-RELOAD (normal normal)
                BOLD RELOADED (bold bold)
                BOLD PLAIN-AGAIN (normal normal)
                BOLD ORG-RUN ("TODO" (org-todo org-level-1))
                GRUVBOX-BOLD-READY"#]],
                mismatches,
            );
        },
    )
}

fn color256(packages: &PreparedPackageSet) -> Result<(), String> {
    run_profile(
        "gruvbox-theme-color256",
        packages,
        DisplayEnvOverride::Remove { key: "COLORTERM" },
        |pair, mismatches| {
            record_pair(
                pair,
                "256-color capability",
                expect![[r#"
                    CAP TERM "dumb"
                    CAP COLORTERM nil
                    CAP CELLS 256
                    CAP VISUAL static-color
                    CAP DISPLAY color
                    CAP GRAPHIC nil
                    CAP TRUECOLOR nil
                    CAP COLOR256 t
                    CAP ORG-COMPILED t
                    CAP GNUS-BEFORE nil
                    CAP LOAD-SUFFIXES (".el")
                    THEMES-KNOWN t
                    GRUVBOX-TUI-BOOT"#]],
                mismatches,
            );
            invoke_both(pair, "gt357-configure-core-org", "GRUVBOX-CORE-ORG-READY");
            record_pair(
                pair,
                "256-color core Org configuration",
                expect![[r#"
                    CORE-ORG BEFORE-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                    CORE-ORG BEFORE-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                    CORE-ORG AFTER nil
                    CORE-ORG GNUS (nil nil nil)
                    GRUVBOX-CORE-ORG-READY"#]],
                mismatches,
            );
            initialize_consumer(
                pair,
                "256-color core",
                expect![[r#"
                CONSUMER MODULES-1 nil
                CONSUMER MODULES-2 nil
                CONSUMER BEFORE (nil nil nil)
                CONSUMER THEME (gruvbox-dark-medium)
                CONSUMER OUTCOME (:value returned)
                CONSUMER SOURCE nil nil
                CONSUMER AFTER (nil nil nil)
                CONSUMER INHERIT nil
                CONSUMER SUFFIXES (".el")
                GRUVBOX-CONSUMER-READY"#]],
                mismatches,
            );
            record_orderless_completion(
                pair,
                expect![[r#"
                [0;1;38;5;73;48;5;235malpha[0;38;5;223;48;5;235m [0;1;38;5;166;48;5;235mb[0;38;5;223;48;5;235meta [0;1;38;5;108;48;5;235mgam[0;38;5;223;48;5;235mma [0;1;38;5;214;48;5;235mdel[0;38;5;223;48;5;235mta[0m
            "#]],
                expect![[r##"
                ORDERLESS CHOICE "alpha beta gamma delta"
                ORDERLESS FINAL-INPUT "alpha beta gamma delta"
                ORDERLESS HISTORY-HEAD "alpha beta gamma delta"
                ORDERLESS MINIBUFFER nil
                ORDERLESS RUN ("alpha" orderless-match-face-0)
                ORDERLESS RUN (" " nil)
                ORDERLESS RUN ("b" orderless-match-face-1)
                ORDERLESS RUN ("eta " nil)
                ORDERLESS RUN ("gam" orderless-match-face-2)
                ORDERLESS RUN ("ma " nil)
                ORDERLESS RUN ("del" orderless-match-face-3)
                ORDERLESS RUN ("ta" nil)
                ORDERLESS FACE 0 "#5fafaf" "#5fafaf" bold bold
                ORDERLESS FACE 1 "#d75f00" "#d75f00" bold bold
                ORDERLESS FACE 2 "#87af87" "#87af87" bold bold
                ORDERLESS FACE 3 "#ffaf00" "#ffaf00" bold bold
                GRUVBOX-ORDERLESS-READY"##]],
                mismatches,
            );
            assert_matrix(
                pair,
                "256-color seven-theme matrix",
                expect![[r##"
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox
                    ENABLED (gruvbox)
                    MODE dark
                    FACE default :foreground "#ffdfaf" "#ffdfaf"
                    FACE default :background "#262626" "#262626"
                    FACE keyword :foreground "#d75f5f" "#d75f5f"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#afaf00" "#afaf00"
                    FACE org-link :foreground "#5f8787" "#5f8787"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#afaf00" "#afaf00"
                    FACE diff-added :background unspecified "#262626"
                    FACE diff-removed :foreground "#d75f5f" "#d75f5f"
                    FACE diff-removed :background unspecified "#262626"
                    FACE diff-context :foreground "#ffdfaf" "#ffdfaf"
                    FACE diff-context :background "#3a3a3a" "#3a3a3a"
                    FACE mode-line-inactive :foreground "#949494" "#949494"
                    FACE mode-line-inactive :background "#3a3a3a" "#3a3a3a"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ffdfaf"
                    FACE region :background "#4e4e4e" "#4e4e4e"
                    FACE hl-line :foreground unspecified "#ffdfaf"
                    FACE hl-line :background "#3a3a3a" "#3a3a3a"
                    FACE cursor :background "#ffdfaf" "#ffdfaf"
                    FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d75f00" "#d75f00"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#87af87" "#87af87"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#af8700" "#af8700"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4934"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "gruvbox-tui-baseline-light"
                    VAR PDF-DARK "gruvbox-tui-baseline-dark"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-dark-hard
                    ENABLED (gruvbox-dark-hard)
                    MODE dark
                    FACE default :foreground "#ffdfaf" "#ffdfaf"
                    FACE default :background "#1c1c1c" "#1c1c1c"
                    FACE keyword :foreground "#d75f5f" "#d75f5f"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#afaf00" "#afaf00"
                    FACE org-link :foreground "#87af87" "#87af87"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#afaf00" "#afaf00"
                    FACE diff-added :background unspecified "#1c1c1c"
                    FACE diff-removed :foreground "#d75f5f" "#d75f5f"
                    FACE diff-removed :background unspecified "#1c1c1c"
                    FACE diff-context :foreground "#ffdfaf" "#ffdfaf"
                    FACE diff-context :background "#3a3a3a" "#3a3a3a"
                    FACE mode-line-inactive :foreground "#949494" "#949494"
                    FACE mode-line-inactive :background "#3a3a3a" "#3a3a3a"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ffdfaf"
                    FACE region :background "#4e4e4e" "#4e4e4e"
                    FACE hl-line :foreground unspecified "#ffdfaf"
                    FACE hl-line :background "#3a3a3a" "#3a3a3a"
                    FACE cursor :background "#ffdfaf" "#ffdfaf"
                    FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d75f00" "#d75f00"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#87af87" "#87af87"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4933"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#fdf4c1"
                    VAR PDF-DARK "#1d2021"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-dark-medium
                    ENABLED (gruvbox-dark-medium)
                    MODE dark
                    FACE default :foreground "#ffdfaf" "#ffdfaf"
                    FACE default :background "#262626" "#262626"
                    FACE keyword :foreground "#d75f5f" "#d75f5f"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#afaf00" "#afaf00"
                    FACE org-link :foreground "#87af87" "#87af87"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#afaf00" "#afaf00"
                    FACE diff-added :background unspecified "#262626"
                    FACE diff-removed :foreground "#d75f5f" "#d75f5f"
                    FACE diff-removed :background unspecified "#262626"
                    FACE diff-context :foreground "#ffdfaf" "#ffdfaf"
                    FACE diff-context :background "#3a3a3a" "#3a3a3a"
                    FACE mode-line-inactive :foreground "#949494" "#949494"
                    FACE mode-line-inactive :background "#3a3a3a" "#3a3a3a"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ffdfaf"
                    FACE region :background "#4e4e4e" "#4e4e4e"
                    FACE hl-line :foreground unspecified "#ffdfaf"
                    FACE hl-line :background "#3a3a3a" "#3a3a3a"
                    FACE cursor :background "#ffdfaf" "#ffdfaf"
                    FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d75f00" "#d75f00"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#87af87" "#87af87"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4933"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#fdf4c1"
                    VAR PDF-DARK "#282828"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-dark-soft
                    ENABLED (gruvbox-dark-soft)
                    MODE dark
                    FACE default :foreground "#ffdfaf" "#ffdfaf"
                    FACE default :background "#303030" "#303030"
                    FACE keyword :foreground "#d75f5f" "#d75f5f"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#afaf00" "#afaf00"
                    FACE org-link :foreground "#87af87" "#87af87"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#afaf00" "#afaf00"
                    FACE diff-added :background unspecified "#303030"
                    FACE diff-removed :foreground "#d75f5f" "#d75f5f"
                    FACE diff-removed :background unspecified "#303030"
                    FACE diff-context :foreground "#ffdfaf" "#ffdfaf"
                    FACE diff-context :background "#3a3a3a" "#3a3a3a"
                    FACE mode-line-inactive :foreground "#949494" "#949494"
                    FACE mode-line-inactive :background "#3a3a3a" "#3a3a3a"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#ffdfaf"
                    FACE region :background "#4e4e4e" "#4e4e4e"
                    FACE hl-line :foreground unspecified "#ffdfaf"
                    FACE hl-line :background "#3a3a3a" "#3a3a3a"
                    FACE cursor :background "#ffdfaf" "#ffdfaf"
                    FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d75f00" "#d75f00"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#87af87" "#87af87"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#3c3836"
                    VAR ANSI 1 "#fb4933"
                    VAR ANSI 2 "#b8bb26"
                    VAR ANSI 3 "#fabd2f"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#83a598"
                    VAR ANSI 5 "#d3869b"
                    VAR ANSI 6 "#8ec07c"
                    VAR ANSI 7 "#ebdbb2"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#fdf4c1"
                    VAR PDF-DARK "#32302f"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-light-hard
                    ENABLED (gruvbox-light-hard)
                    MODE light
                    FACE default :foreground "#3a3a3a" "#3a3a3a"
                    FACE default :background "#ffffd7" "#ffffd7"
                    FACE keyword :foreground "#870000" "#870000"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#878700" "#878700"
                    FACE org-link :foreground "#87af87" "#87af87"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#878700" "#878700"
                    FACE diff-added :background unspecified "#ffffd7"
                    FACE diff-removed :foreground "#870000" "#870000"
                    FACE diff-removed :background unspecified "#ffffd7"
                    FACE diff-context :foreground "#3a3a3a" "#3a3a3a"
                    FACE diff-context :background "#ffffaf" "#ffffaf"
                    FACE mode-line-inactive :foreground "#767676" "#767676"
                    FACE mode-line-inactive :background "#ffffaf" "#ffffaf"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#3a3a3a"
                    FACE region :background "#d7d6af" "#d7d6af"
                    FACE hl-line :foreground unspecified "#3a3a3a"
                    FACE hl-line :background "#ffffaf" "#ffffaf"
                    FACE cursor :background "#3a3a3a" "#3a3a3a"
                    FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d75f00" "#d75f00"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#87af87" "#87af87"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#ebdbb2"
                    VAR ANSI 1 "#cc241d"
                    VAR ANSI 2 "#98971a"
                    VAR ANSI 3 "#d79921"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#458588"
                    VAR ANSI 5 "#b16286"
                    VAR ANSI 6 "#689d6a"
                    VAR ANSI 7 "#3c3836"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#282828"
                    VAR PDF-DARK "#f9f5d7"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-light-medium
                    ENABLED (gruvbox-light-medium)
                    MODE light
                    FACE default :foreground "#3a3a3a" "#3a3a3a"
                    FACE default :background "#ffffd7" "#ffffd7"
                    FACE keyword :foreground "#870000" "#870000"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#878700" "#878700"
                    FACE org-link :foreground "#87af87" "#87af87"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#878700" "#878700"
                    FACE diff-added :background unspecified "#ffffd7"
                    FACE diff-removed :foreground "#870000" "#870000"
                    FACE diff-removed :background unspecified "#ffffd7"
                    FACE diff-context :foreground "#3a3a3a" "#3a3a3a"
                    FACE diff-context :background "#ffffaf" "#ffffaf"
                    FACE mode-line-inactive :foreground "#767676" "#767676"
                    FACE mode-line-inactive :background "#ffffaf" "#ffffaf"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#3a3a3a"
                    FACE region :background "#d7d6af" "#d7d6af"
                    FACE hl-line :foreground unspecified "#3a3a3a"
                    FACE hl-line :background "#ffffaf" "#ffffaf"
                    FACE cursor :background "#3a3a3a" "#3a3a3a"
                    FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d75f00" "#d75f00"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#87af87" "#87af87"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#ebdbb2"
                    VAR ANSI 1 "#cc241d"
                    VAR ANSI 2 "#98971a"
                    VAR ANSI 3 "#d79921"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#458588"
                    VAR ANSI 5 "#b16286"
                    VAR ANSI 6 "#689d6a"
                    VAR ANSI 7 "#3c3836"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#282828"
                    VAR PDF-DARK "#fbf1c7"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY
                    --
                    GRUVBOX-THEME-PAGE 1/3
                    THEME gruvbox-light-soft
                    ENABLED (gruvbox-light-soft)
                    MODE light
                    FACE default :foreground "#3a3a3a" "#3a3a3a"
                    FACE default :background "#ffffd7" "#ffffd7"
                    FACE keyword :foreground "#870000" "#870000"
                    FACE keyword :weight normal normal
                    FACE string :foreground "#878700" "#878700"
                    FACE org-link :foreground "#87af87" "#87af87"
                    FACE org-link :underline t t
                    FACE diff-added :foreground "#878700" "#878700"
                    FACE diff-added :background unspecified "#ffffd7"
                    FACE diff-removed :foreground "#870000" "#870000"
                    FACE diff-removed :background unspecified "#ffffd7"
                    FACE diff-context :foreground "#3a3a3a" "#3a3a3a"
                    FACE diff-context :background "#ffffaf" "#ffffaf"
                    FACE mode-line-inactive :foreground "#767676" "#767676"
                    FACE mode-line-inactive :background "#ffffaf" "#ffffaf"
                    GRUVBOX-THEME-PAGE-DONE 1/3
                    --
                    GRUVBOX-THEME-PAGE 2/3
                    FACE region :foreground unspecified "#3a3a3a"
                    FACE region :background "#d7d6af" "#d7d6af"
                    FACE hl-line :foreground unspecified "#3a3a3a"
                    FACE hl-line :background "#ffffaf" "#ffffaf"
                    FACE cursor :background "#3a3a3a" "#3a3a3a"
                    FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                    FACE orderless-0 :weight bold bold
                    FACE orderless-1 :foreground "#d75f00" "#d75f00"
                    FACE orderless-1 :weight bold bold
                    FACE orderless-2 :foreground "#87af87" "#87af87"
                    FACE orderless-2 :weight bold bold
                    FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                    FACE orderless-3 :weight bold bold
                    VAR ANSI-BOUND t
                    VAR ANSI 0 "#ebdbb2"
                    VAR ANSI 1 "#9d0006"
                    VAR ANSI 2 "#79740e"
                    VAR ANSI 3 "#b57614"
                    GRUVBOX-THEME-PAGE-DONE 2/3
                    --
                    GRUVBOX-THEME-PAGE 3/3
                    VAR ANSI 4 "#076678"
                    VAR ANSI 5 "#8f3f71"
                    VAR ANSI 6 "#427b58"
                    VAR ANSI 7 "#3c3836"
                    VAR PDF-BOUND t
                    VAR PDF-LIGHT "#282828"
                    VAR PDF-DARK "#f2e5bc"
                    GRUVBOX-THEME-PAGE-DONE 3/3
                    GRUVBOX-THEME-READY"##]],
                mismatches,
            );
            let _ordinary = exercise_rendering(
                pair,
                "256-color",
                RenderingExpectations {
                    dark_elisp: expect![[r#"
                        [0;38;5;243;48;5;235m; comment Ω[0m
                        [0;38;5;223;48;5;235m([0;38;5;167;48;5;235mdefun[0;38;5;223;48;5;235m [0;38;5;214;48;5;235mgreet[0;38;5;223;48;5;235m (name)[0m
                        [0;38;5;223;48;5;235m  [0;38;5;142;48;5;235m"Doc."[0m
                        [0;38;5;223;48;5;235m  ([0;38;5;167;48;5;235mif[0;38;5;223;48;5;235m name (message [0;38;5;142;48;5;235m"Hello %s"[0;38;5;223;48;5;235m name) nil))[0m
                    "#]],
                    dark_org: expect![[r#"
                        [0;38;5;243;48;5;235m#+title:[0;38;5;223;48;5;235m [0;38;5;109;48;5;235mPlan Ω[0m
                        [0;38;5;109;48;5;235m* [0;1;38;5;167;48;5;235mTODO[0;38;5;109;48;5;235m Ship release[0m
                        [0;38;5;214;48;5;235m** [0;1;38;5;108;48;5;235mDONE[0;38;5;214;48;5;235m [0;38;5;108;48;5;235mVerify rollback[0m
                        [0;38;5;223;48;5;235mA [0;4;38;5;108;48;5;235mlink[0;38;5;223;48;5;235m and [0;38;5;243;48;5;235m=code=[0;38;5;223;48;5;235m.[0m
                        [0;38;5;223;48;5;237m#+begin_src emacs-lisp[0m
                        [0;38;5;223;48;5;236m(message [0;38;5;142;48;5;236m"ship"[0;38;5;223;48;5;236m)[0m
                        [0;38;5;223;48;5;237m#+end_src[0m
                    "#]],
                    dark_diff: expect![[r#"
                        [0;38;5;223;48;5;237mdiff --git a/a.el b/a.el[0m
                        [0;38;5;223;48;5;237m--- [0;38;5;223;48;5;239ma/a.el[0m
                        [0;38;5;223;48;5;237m+++ [0;38;5;223;48;5;239mb/a.el[0m
                        [0;38;5;223;48;5;239m@@ -1 +1 @@[0m
                        [0;38;5;167;48;5;235m-([0;38;5;223;48;5;167mold[0;38;5;167;48;5;235m)[0m
                        [0;38;5;142;48;5;235m+([0;38;5;223;48;5;142mnew[0;38;5;142;48;5;235m)[0m
                    "#]],
                    dark_properties: expect![[r##"
                        PROPERTIES gruvbox-dark-medium E PAGE 1/1
                        PROPERTY-COUNT E 13
                        RUN E ("; " font-lock-comment-delimiter-face)
                        RUN E ("comment Ω\n" font-lock-comment-face)
                        RUN E ("(" nil)
                        RUN E ("defun" font-lock-keyword-face)
                        RUN E (" " nil)
                        RUN E ("greet" font-lock-function-name-face)
                        RUN E (" (name)\n  " nil)
                        RUN E ("\"Doc.\"" font-lock-doc-face)
                        RUN E ("\n  (" nil)
                        RUN E ("if" font-lock-keyword-face)
                        RUN E (" name (message " nil)
                        RUN E ("\"Hello %s\"" font-lock-string-face)
                        RUN E (" name) nil))\n" nil)
                        GRUVBOX-PROPERTIES-E-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-E-READY
                        --
                        PROPERTIES gruvbox-dark-medium O PAGE 1/2
                        PROPERTY-COUNT O 21
                        RUN O ("#+title:" org-document-info-keyword)
                        RUN O (" " nil)
                        RUN O ("Plan Ω\n" org-document-title)
                        RUN O ("* " org-level-1)
                        RUN O ("TODO" (org-todo org-level-1))
                        RUN O (" Ship release" org-level-1)
                        RUN O ("\n" nil)
                        RUN O ("** " org-level-2)
                        RUN O ("DONE" (org-done org-level-2))
                        RUN O (" " org-level-2)
                        RUN O ("Verify rollback" (org-headline-done org-level-2))
                        RUN O ("\nA " nil)
                        RUN O ("[[https://example.invalid][link]]" org-link)
                        RUN O (" and " nil)
                        RUN O ("=code=" (org-verbatim))
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 1/2
                        --
                        PROPERTIES gruvbox-dark-medium O PAGE 2/2
                        PROPERTY-COUNT O 21
                        RUN O (".\n" nil)
                        RUN O ("#+begin_src emacs-lisp\n" org-block-begin-line)
                        RUN O ("(message " (org-block))
                        RUN O ("\"ship\"" (font-lock-string-face org-block))
                        RUN O (")\n" (org-block))
                        RUN O ("#+end_src\n" org-block-end-line)
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 2/2
                        GRUVBOX-PROPERTIES-O-READY
                        --
                        PROPERTIES gruvbox-dark-medium D PAGE 1/1
                        PROPERTY-COUNT D 12
                        RUN D ("diff --git a/a.el b/a.el\n--- " diff-header)
                        RUN D ("a/a.el" (diff-file-header diff-header))
                        RUN D ("\n+++ " diff-header)
                        RUN D ("b/a.el" (diff-file-header diff-header))
                        RUN D ("\n" diff-header)
                        RUN D ("@@ -1 +1 @@" diff-hunk-header)
                        RUN D ("\n" nil)
                        RUN D ("-" diff-indicator-removed)
                        RUN D ("(old)\n" diff-removed)
                        RUN D ("+" diff-indicator-added)
                        RUN D ("(new)\n" diff-added)
                        RUN D (" context\n" diff-context)
                        GRUVBOX-PROPERTIES-D-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-D-READY"##]],
                    dark_state: expect![[r##"
                        GRUVBOX-THEME-PAGE 1/3
                        THEME gruvbox-dark-medium
                        ENABLED (gruvbox-dark-medium)
                        MODE dark
                        FACE default :foreground "#ffdfaf" "#ffdfaf"
                        FACE default :background "#262626" "#262626"
                        FACE keyword :foreground "#d75f5f" "#d75f5f"
                        FACE keyword :weight normal normal
                        FACE string :foreground "#afaf00" "#afaf00"
                        FACE org-link :foreground "#87af87" "#87af87"
                        FACE org-link :underline t t
                        FACE diff-added :foreground "#afaf00" "#afaf00"
                        FACE diff-added :background unspecified "#262626"
                        FACE diff-removed :foreground "#d75f5f" "#d75f5f"
                        FACE diff-removed :background unspecified "#262626"
                        FACE diff-context :foreground "#ffdfaf" "#ffdfaf"
                        FACE diff-context :background "#3a3a3a" "#3a3a3a"
                        FACE mode-line-inactive :foreground "#949494" "#949494"
                        FACE mode-line-inactive :background "#3a3a3a" "#3a3a3a"
                        GRUVBOX-THEME-PAGE-DONE 1/3
                        --
                        GRUVBOX-THEME-PAGE 2/3
                        FACE region :foreground unspecified "#ffdfaf"
                        FACE region :background "#4e4e4e" "#4e4e4e"
                        FACE hl-line :foreground unspecified "#ffdfaf"
                        FACE hl-line :background "#3a3a3a" "#3a3a3a"
                        FACE cursor :background "#ffdfaf" "#ffdfaf"
                        FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                        FACE orderless-0 :weight bold bold
                        FACE orderless-1 :foreground "#d75f00" "#d75f00"
                        FACE orderless-1 :weight bold bold
                        FACE orderless-2 :foreground "#87af87" "#87af87"
                        FACE orderless-2 :weight bold bold
                        FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                        FACE orderless-3 :weight bold bold
                        VAR ANSI-BOUND t
                        VAR ANSI 0 "#3c3836"
                        VAR ANSI 1 "#fb4933"
                        VAR ANSI 2 "#b8bb26"
                        VAR ANSI 3 "#fabd2f"
                        GRUVBOX-THEME-PAGE-DONE 2/3
                        --
                        GRUVBOX-THEME-PAGE 3/3
                        VAR ANSI 4 "#83a598"
                        VAR ANSI 5 "#d3869b"
                        VAR ANSI 6 "#8ec07c"
                        VAR ANSI 7 "#ebdbb2"
                        VAR PDF-BOUND t
                        VAR PDF-LIGHT "#fdf4c1"
                        VAR PDF-DARK "#282828"
                        GRUVBOX-THEME-PAGE-DONE 3/3
                        GRUVBOX-THEME-READY"##]],
                    light_elisp: expect![[r#"
                        [0;38;5;145;48;5;230m; comment Ω[0m
                        [0;38;5;237;48;5;230m([0;38;5;88;48;5;230mdefun[0;38;5;237;48;5;230m [0;38;5;136;48;5;230mgreet[0;38;5;237;48;5;230m (name)[0m
                        [0;38;5;237;48;5;230m  [0;38;5;100;48;5;230m"Doc."[0m
                        [0;38;5;237;48;5;230m  ([0;38;5;88;48;5;230mif[0;38;5;237;48;5;230m name (message [0;38;5;100;48;5;230m"Hello %s"[0;38;5;237;48;5;230m name) nil))[0m
                    "#]],
                    light_org: expect![[r#"
                        [0;38;5;145;48;5;230m#+title:[0;38;5;237;48;5;230m [0;38;5;109;48;5;230mPlan Ω[0m
                        [0;38;5;24;48;5;230m* [0;1;38;5;88;48;5;230mTODO[0;38;5;24;48;5;230m Ship release[0m
                        [0;38;5;136;48;5;230m** [0;1;38;5;66;48;5;230mDONE[0;38;5;136;48;5;230m [0;38;5;66;48;5;230mVerify rollback[0m
                        [0;38;5;237;48;5;230mA [0;4;38;5;108;48;5;230mlink[0;38;5;237;48;5;230m and [0;38;5;145;48;5;230m=code=[0;38;5;237;48;5;230m.[0m
                        [0;38;5;237;48;5;229m#+begin_src emacs-lisp[0m
                        [0;38;5;237;48;5;230m(message [0;38;5;100;48;5;230m"ship"[0;38;5;237;48;5;230m)[0m
                        [0;38;5;237;48;5;229m#+end_src[0m
                    "#]],
                    light_diff: expect![[r#"
                        [0;38;5;237;48;5;229mdiff --git a/a.el b/a.el[0m
                        [0;38;5;237;48;5;229m--- [0;38;5;237;48;5;187ma/a.el[0m
                        [0;38;5;237;48;5;229m+++ [0;38;5;237;48;5;187mb/a.el[0m
                        [0;38;5;237;48;5;187m@@ -1 +1 @@[0m
                        [0;38;5;88;48;5;230m-([0;38;5;237;48;5;167mold[0;38;5;88;48;5;230m)[0m
                        [0;38;5;100;48;5;230m+([0;38;5;237;48;5;142mnew[0;38;5;100;48;5;230m)[0m
                    "#]],
                    light_properties: expect![[r##"
                        PROPERTIES gruvbox-light-medium E PAGE 1/1
                        PROPERTY-COUNT E 13
                        RUN E ("; " font-lock-comment-delimiter-face)
                        RUN E ("comment Ω\n" font-lock-comment-face)
                        RUN E ("(" nil)
                        RUN E ("defun" font-lock-keyword-face)
                        RUN E (" " nil)
                        RUN E ("greet" font-lock-function-name-face)
                        RUN E (" (name)\n  " nil)
                        RUN E ("\"Doc.\"" font-lock-doc-face)
                        RUN E ("\n  (" nil)
                        RUN E ("if" font-lock-keyword-face)
                        RUN E (" name (message " nil)
                        RUN E ("\"Hello %s\"" font-lock-string-face)
                        RUN E (" name) nil))\n" nil)
                        GRUVBOX-PROPERTIES-E-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-E-READY
                        --
                        PROPERTIES gruvbox-light-medium O PAGE 1/2
                        PROPERTY-COUNT O 21
                        RUN O ("#+title:" org-document-info-keyword)
                        RUN O (" " nil)
                        RUN O ("Plan Ω\n" org-document-title)
                        RUN O ("* " org-level-1)
                        RUN O ("TODO" (org-todo org-level-1))
                        RUN O (" Ship release" org-level-1)
                        RUN O ("\n" nil)
                        RUN O ("** " org-level-2)
                        RUN O ("DONE" (org-done org-level-2))
                        RUN O (" " org-level-2)
                        RUN O ("Verify rollback" (org-headline-done org-level-2))
                        RUN O ("\nA " nil)
                        RUN O ("[[https://example.invalid][link]]" org-link)
                        RUN O (" and " nil)
                        RUN O ("=code=" (org-verbatim))
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 1/2
                        --
                        PROPERTIES gruvbox-light-medium O PAGE 2/2
                        PROPERTY-COUNT O 21
                        RUN O (".\n" nil)
                        RUN O ("#+begin_src emacs-lisp\n" org-block-begin-line)
                        RUN O ("(message " (org-block))
                        RUN O ("\"ship\"" (font-lock-string-face org-block))
                        RUN O (")\n" (org-block))
                        RUN O ("#+end_src\n" org-block-end-line)
                        GRUVBOX-PROPERTIES-O-PAGE-DONE 2/2
                        GRUVBOX-PROPERTIES-O-READY
                        --
                        PROPERTIES gruvbox-light-medium D PAGE 1/1
                        PROPERTY-COUNT D 12
                        RUN D ("diff --git a/a.el b/a.el\n--- " diff-header)
                        RUN D ("a/a.el" (diff-file-header diff-header))
                        RUN D ("\n+++ " diff-header)
                        RUN D ("b/a.el" (diff-file-header diff-header))
                        RUN D ("\n" diff-header)
                        RUN D ("@@ -1 +1 @@" diff-hunk-header)
                        RUN D ("\n" nil)
                        RUN D ("-" diff-indicator-removed)
                        RUN D ("(old)\n" diff-removed)
                        RUN D ("+" diff-indicator-added)
                        RUN D ("(new)\n" diff-added)
                        RUN D (" context\n" diff-context)
                        GRUVBOX-PROPERTIES-D-PAGE-DONE 1/1
                        GRUVBOX-PROPERTIES-D-READY"##]],
                    light_state: expect![[r##"
                        GRUVBOX-THEME-PAGE 1/3
                        THEME gruvbox-light-medium
                        ENABLED (gruvbox-light-medium)
                        MODE light
                        FACE default :foreground "#3a3a3a" "#3a3a3a"
                        FACE default :background "#ffffd7" "#ffffd7"
                        FACE keyword :foreground "#870000" "#870000"
                        FACE keyword :weight normal normal
                        FACE string :foreground "#878700" "#878700"
                        FACE org-link :foreground "#87af87" "#87af87"
                        FACE org-link :underline t t
                        FACE diff-added :foreground "#878700" "#878700"
                        FACE diff-added :background unspecified "#ffffd7"
                        FACE diff-removed :foreground "#870000" "#870000"
                        FACE diff-removed :background unspecified "#ffffd7"
                        FACE diff-context :foreground "#3a3a3a" "#3a3a3a"
                        FACE diff-context :background "#ffffaf" "#ffffaf"
                        FACE mode-line-inactive :foreground "#767676" "#767676"
                        FACE mode-line-inactive :background "#ffffaf" "#ffffaf"
                        GRUVBOX-THEME-PAGE-DONE 1/3
                        --
                        GRUVBOX-THEME-PAGE 2/3
                        FACE region :foreground unspecified "#3a3a3a"
                        FACE region :background "#d7d6af" "#d7d6af"
                        FACE hl-line :foreground unspecified "#3a3a3a"
                        FACE hl-line :background "#ffffaf" "#ffffaf"
                        FACE cursor :background "#3a3a3a" "#3a3a3a"
                        FACE orderless-0 :foreground "#5fafaf" "#5fafaf"
                        FACE orderless-0 :weight bold bold
                        FACE orderless-1 :foreground "#d75f00" "#d75f00"
                        FACE orderless-1 :weight bold bold
                        FACE orderless-2 :foreground "#87af87" "#87af87"
                        FACE orderless-2 :weight bold bold
                        FACE orderless-3 :foreground "#ffaf00" "#ffaf00"
                        FACE orderless-3 :weight bold bold
                        VAR ANSI-BOUND t
                        VAR ANSI 0 "#ebdbb2"
                        VAR ANSI 1 "#cc241d"
                        VAR ANSI 2 "#98971a"
                        VAR ANSI 3 "#d79921"
                        GRUVBOX-THEME-PAGE-DONE 2/3
                        --
                        GRUVBOX-THEME-PAGE 3/3
                        VAR ANSI 4 "#458588"
                        VAR ANSI 5 "#b16286"
                        VAR ANSI 6 "#689d6a"
                        VAR ANSI 7 "#3c3836"
                        VAR PDF-BOUND t
                        VAR PDF-LIGHT "#282828"
                        VAR PDF-DARK "#fbf1c7"
                        GRUVBOX-THEME-PAGE-DONE 3/3
                        GRUVBOX-THEME-READY"##]],
                },
                mismatches,
            );
        },
    )
}

#[test]
fn gruvbox_theme_real_terminal_profiles_match_gnu() {
    let oracle = oracle();
    let default_org = catch_phase("default Org consumer profile", || {
        default_org_consumer(oracle.prepared_packages())
    })
    .and_then(|result| result);
    let truecolor = catch_phase("truecolor profile", || {
        truecolor(oracle.prepared_packages())
    })
    .and_then(|result| result);
    let color256 = catch_phase("256-color profile", || color256(oracle.prepared_packages()))
        .and_then(|result| result);
    let failures = [default_org.err(), truecolor.err(), color256.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "Gruvbox real terminal profiles failed:\n{}",
        failures.join("\n\n")
    );
}
