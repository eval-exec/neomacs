pub(super) const GRUVBOX_TUI_PRELUDE: &str = r####"
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
