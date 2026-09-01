use expect_test::expect;

use super::ParityBatchCase;

fn loading_the_theme_repaints_a_real_elisp_editing_session() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "zen-and-art-elisp-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "invoice-review.el" root))
       (default-directory root)
       (faces
        '(default cursor fringe region hl-line highlight italic underline
          minibuffer-prompt))
       buffer before after)
  (unwind-protect
      (save-window-excursion
        (progn
        (neomacs-zen-and-art-test-cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           ";;; invoice-review.el --- Review overdue invoices\n\n"
           ";; Keep disputed invoices visible to the accounting team.\n\n"
           "(defconst invoice-review-limit 25\n"
           "  \"Maximum invoices reviewed in one batch.\")\n\n"
           "(defun invoice-review-overdue-p (invoice)\n"
           "  \"Return non-nil when INVOICE needs review.\"\n"
           "  (if (null invoice)\n"
           "      (error \"Missing invoice\")\n"
           "    (message \"reviewing %s\" invoice)\n"
           "    :overdue))\n"))
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (emacs-lisp-mode)
        (font-lock-ensure)
        (setq before
              (list
               :themes (copy-sequence custom-enabled-themes)
               :default
               (list
                (face-attribute 'default :foreground nil t)
                (face-attribute 'default :background nil t))
               :hl-line-defined (and (facep 'hl-line) t)))
        (load-theme 'zen-and-art t)
        (font-lock-flush)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "(message")
        (setq after
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :themes (copy-sequence custom-enabled-themes)
               :point (point)
               :line (line-number-at-pos)
               :modified (buffer-modified-p)
               :content (buffer-substring-no-properties (point-min) (point-max))
               :faces (neomacs-zen-and-art-test-face-state faces)
               :tokens
               (neomacs-zen-and-art-test-token-state
                '(";;; invoice-review.el"
                  "Review overdue invoices"
                  "Keep disputed invoices"
                  "defconst"
                  "invoice-review-limit"
                  "25"
                  "Maximum invoices"
                  "defun"
                  "invoice-review-overdue-p"
                  "Return non-nil"
                  "if"
                  "null"
                  "error"
                  "\"Missing invoice\""
                  "message"
                  ":overdue"))))))
    (neomacs-zen-and-art-test-cleanup root))
  (list :before before :after after))
"####;
    let expect = expect![[
        r##"OK (:before (:themes nil :default ("unspecified-fg" "unspecified-bg") :hl-line-defined nil) :after (:file "invoice-review.el" :mode emacs-lisp-mode :themes (zen-and-art) :point 344 :line 12 :modified nil :content ";;; invoice-review.el --- Review overdue invoices\n\n;; Keep disputed invoices visible to the accounting team.\n\n(defconst invoice-review-limit 25\n  \"Maximum invoices reviewed in one batch.\")\n\n(defun invoice-review-overdue-p (invoice)\n  \"Return non-nil when INVOICE needs review.\"\n  (if (null invoice)\n      (error \"Missing invoice\")\n    (message \"reviewing %s\" invoice)\n    :overdue))\n" :faces ((:face default :defined t :foreground "#d2dec4" :background "#191717" :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight normal :slant normal :underline nil :inherit nil) (:face cursor :defined t :foreground "#a7a7a7" :background unspecified :resolved-foreground "#a7a7a7" :resolved-background "#191717" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#252323" :resolved-foreground "#d2dec4" :resolved-background "#252323" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#999966" :resolved-foreground "#d2dec4" :resolved-background "#999966" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face hl-line :defined nil) (:face highlight :defined t :foreground unspecified :background "darkolivegreen" :resolved-foreground "#d2dec4" :resolved-background "darkolivegreen" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight unspecified :slant italic :underline unspecified :inherit unspecified) (:face underline :defined t :foreground unspecified :background unspecified :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight unspecified :slant unspecified :underline t :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#ff6600" :background unspecified :resolved-foreground "#ff6600" :resolved-background "#191717" :weight bold :slant unspecified :underline unspecified :inherit unspecified)) :tokens ((:token ";;; invoice-review.el" :face font-lock-comment-delimiter-face :font-lock-face nil :foreground "#4C565D" :background unspecified :resolved-foreground "#4C565D" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "Review overdue invoices" :face font-lock-comment-face :font-lock-face nil :foreground "#333B40" :background unspecified :resolved-foreground "#333B40" :resolved-background "#191717" :weight unspecified :slant italic) (:token "Keep disputed invoices" :face font-lock-comment-face :font-lock-face nil :foreground "#333B40" :background unspecified :resolved-foreground "#333B40" :resolved-background "#191717" :weight unspecified :slant italic) (:token "defconst" :face font-lock-keyword-face :font-lock-face nil :foreground "#AE5825" :background unspecified :resolved-foreground "#AE5825" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "invoice-review-limit" :face font-lock-variable-name-face :font-lock-face nil :foreground "#46657B" :background unspecified :resolved-foreground "#46657B" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "25" :face nil :font-lock-face nil :foreground nil :background nil :resolved-foreground nil :resolved-background nil :weight nil :slant nil) (:token "Maximum invoices" :face font-lock-doc-face :font-lock-face nil :foreground "#DDFFD1" :background unspecified :resolved-foreground "#DDFFD1" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "defun" :face font-lock-keyword-face :font-lock-face nil :foreground "#AE5825" :background unspecified :resolved-foreground "#AE5825" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "invoice-review-overdue-p" :face font-lock-function-name-face :font-lock-face nil :foreground "#C6B032" :background unspecified :resolved-foreground "#C6B032" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "Return non-nil" :face font-lock-doc-face :font-lock-face nil :foreground "#DDFFD1" :background unspecified :resolved-foreground "#DDFFD1" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "if" :face font-lock-keyword-face :font-lock-face nil :foreground "#AE5825" :background unspecified :resolved-foreground "#AE5825" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "null" :face nil :font-lock-face nil :foreground nil :background nil :resolved-foreground nil :resolved-background nil :weight nil :slant nil) (:token "error" :face font-lock-warning-face :font-lock-face nil :foreground "Pink" :background unspecified :resolved-foreground "Pink" :resolved-background "#191717" :weight bold :slant unspecified) (:token "\"Missing invoice\"" :face font-lock-string-face :font-lock-face nil :foreground "#5A7644" :background unspecified :resolved-foreground "#5A7644" :resolved-background "#191717" :weight unspecified :slant unspecified) (:token "message" :face nil :font-lock-face nil :foreground nil :background nil :resolved-foreground nil :resolved-background nil :weight nil :slant nil) (:token ":overdue" :face font-lock-builtin-face :font-lock-face nil :foreground "#86453A" :background unspecified :resolved-foreground "#86453A" :resolved-background "#191717" :weight unspecified :slant unspecified))))"##
    ]];
    ParityBatchCase::value(
        "loading_the_theme_repaints_a_real_elisp_editing_session",
        elisp_form,
        expect,
    )
}

fn selecting_and_searching_real_text_uses_the_ui_palette_without_editing_it() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "zen-and-art-review-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (notes (expand-file-name "release-notes.txt" root))
       (default-directory root)
       (transient-mark-mode t)
       buffer selection result)
  (unwind-protect
      (save-window-excursion
        (unwind-protect
            (progn
              (neomacs-zen-and-art-test-cleanup root)
              (make-directory root t)
              (with-temp-file notes
                (insert
                 "Release review\n"
                 "Invoice Δ-42 is ready for review.\n"
                 "Invoice Δ-43 still needs evidence.\n"))
              (setq buffer (find-file-noselect notes))
              (switch-to-buffer buffer)
              (text-mode)
              (load-theme 'zen-and-art t)
              (hl-line-mode 1)
              (goto-char (point-min))
              (search-forward "Invoice Δ-42")
              (set-mark (match-beginning 0))
              (search-forward "ready for review")
              (activate-mark)
              (setq selection
                    (list
                     :point (point)
                     :mark (mark)
                     :mark-active mark-active
                     :selected
                     (buffer-substring-no-properties
                      (region-beginning) (region-end))))
              (deactivate-mark)
              (goto-char (point-min))
              (isearch-mode t)
              (isearch-yank-string "ready for review")
              (setq result
                    (list
                     :file (file-relative-name buffer-file-name root)
                     :mode major-mode
                     :content
                     (buffer-substring-no-properties (point-min) (point-max))
                     :selection selection
                     :search
                     (list
                      :string isearch-string
                      :success isearch-success
                      :point (point)
                      :overlay-text
                      (buffer-substring-no-properties
                       (overlay-start isearch-overlay)
                       (overlay-end isearch-overlay))
                      :overlay-face (overlay-get isearch-overlay 'face))
                     :modified (buffer-modified-p)
                     :themes (copy-sequence custom-enabled-themes)
                     :faces
                     (neomacs-zen-and-art-test-face-state
                      '(default region hl-line highlight isearch
                        secondary-selection minibuffer-prompt
                        italic underline))))
              (isearch-done))
          (when (and (boundp 'isearch-mode) isearch-mode)
            (isearch-done))))
    (neomacs-zen-and-art-test-cleanup root))
  result)
"####;
    let expect = expect![[
        r##"OK (:file "release-notes.txt" :mode text-mode :content "Release review\nInvoice Δ-42 is ready for review.\nInvoice Δ-43 still needs evidence.\n" :selection (:point 48 :mark 16 :mark-active t :selected "Invoice Δ-42 is ready for review") :search (:string "ready for review" :success 48 :point 48 :overlay-text "ready for review" :overlay-face isearch) :modified nil :themes (zen-and-art) :faces ((:face default :defined t :foreground "#d2dec4" :background "#191717" :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight normal :slant normal :underline nil :inherit nil) (:face region :defined t :foreground unspecified :background "#999966" :resolved-foreground "#d2dec4" :resolved-background "#999966" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#252323" :resolved-foreground "#d2dec4" :resolved-background "#252323" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face highlight :defined t :foreground unspecified :background "darkolivegreen" :resolved-foreground "#d2dec4" :resolved-background "darkolivegreen" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face isearch :defined t :foreground unspecified :background "#555555" :resolved-foreground "#d2dec4" :resolved-background "#555555" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face secondary-selection :defined t :foreground unspecified :background "#545459" :resolved-foreground "#d2dec4" :resolved-background "#545459" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#ff6600" :background unspecified :resolved-foreground "#ff6600" :resolved-background "#191717" :weight bold :slant unspecified :underline unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight unspecified :slant italic :underline unspecified :inherit unspecified) (:face underline :defined t :foreground unspecified :background unspecified :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight unspecified :slant unspecified :underline t :inherit unspecified)))"##
    ]];
    ParityBatchCase::value(
        "selecting_and_searching_real_text_uses_the_ui_palette_without_editing_it",
        elisp_form,
        expect,
    )
}

fn records_the_complete_palette_and_preserves_legacy_face_contracts() -> ParityBatchCase {
    let elisp_form = r####"
(let ((legacy
       '(border-color cursor-color highlight-current-line-face
         paren-face-match-light modeline modeline-buffer-id
         modeline-mousable modeline-mousable-minor-mode primary-selection
         zmacs-region flymake-errline flymake-warnline)))
  (list
   :known (and (custom-theme-p 'zen-and-art) t)
   :documentation (get 'zen-and-art 'theme-documentation)
   :feature (get 'zen-and-art 'theme-feature)
   :provided (featurep 'zen-and-art-theme)
   :enabled (and (custom-theme-enabled-p 'zen-and-art) t)
   :settings-count (length (get 'zen-and-art 'theme-settings))
   :settings (neomacs-zen-and-art-test-recorded-face-settings)
   :legacy-defined
   (mapcar (lambda (face) (list face (and (facep face) t))) legacy)
   :legacy-recorded
   (mapcar
    (lambda (face)
      (list face
            (and (assq face
                       (neomacs-zen-and-art-test-recorded-face-settings))
                 t)))
    legacy)))
"####;
    let expect = expect![[
        r##"OK (:known t :documentation "zen-and-art color theme" :feature zen-and-art-theme :provided t :enabled nil :settings-count 36 :settings ((border-color ((t (:background "#000000")))) (cursor ((t (:foreground "#a7a7a7")))) (cursor-color ((t (:background "#A7A7A7")))) (default ((t (:background "#191717" :foreground "#d2dec4")))) (flymake-errline ((t (:background "LightSalmon" :foreground "#000000")))) (flymake-warnline ((t (:background "LightSteelBlue" :foreground "#000000")))) (font-lock-builtin-face ((t (:foreground "#86453A")))) (font-lock-comment-delimiter-face ((t (:foreground "#4C565D")))) (font-lock-comment-face ((t (:italic t :foreground "#333B40")))) (font-lock-constant-face ((t (:foreground "#86453A")))) (font-lock-doc-face ((t (:foreground "#DDFFD1")))) (font-lock-function-name-face ((t (:foreground "#C6B032")))) (font-lock-keyword-face ((t (:foreground "#AE5825")))) (font-lock-preprocessor-face ((t (:foreground "#007575")))) (font-lock-reference-face ((t (:foreground "#0055FF")))) (font-lock-string-face ((t (:foreground "#5A7644")))) (font-lock-type-face ((t (:italic t :foreground "#C6B032")))) (font-lock-variable-name-face ((t (:foreground "#46657B")))) (font-lock-warning-face ((t (:bold t :foreground "Pink")))) (fringe ((t (:background "#252323")))) (highlight ((t (:background "darkolivegreen")))) (highlight-current-line-face ((t (:background "#252323")))) (hl-line ((t (:background "#252323")))) (isearch ((t (:background "#555555")))) (italic ((t (:italic t)))) (minibuffer-prompt ((t (:bold t :foreground "#ff6600")))) (modeline ((t (:background "#3F3B3B" :foreground "white")))) (modeline-buffer-id ((t (:background "#3F3B3B" :foreground "white")))) (modeline-mousable ((t (:background "#a5baf1" :foreground "black")))) (modeline-mousable-minor-mode ((t (:background "#a5baf1" :foreground "#000000")))) (paren-face-match-light ((t (:background "#252323")))) (primary-selection ((t (:background "#3B3B3F")))) (region ((t (:background "#999966")))) (secondary-selection ((t (:background "#545459")))) (underline ((t (:underline t)))) (zmacs-region ((t (:background "#555577"))))) :legacy-defined ((border-color nil) (cursor-color nil) (highlight-current-line-face nil) (paren-face-match-light nil) (modeline nil) (modeline-buffer-id nil) (modeline-mousable nil) (modeline-mousable-minor-mode nil) (primary-selection nil) (zmacs-region nil) (flymake-errline nil) (flymake-warnline nil)) :legacy-recorded ((border-color t) (cursor-color t) (highlight-current-line-face t) (paren-face-match-light t) (modeline t) (modeline-buffer-id t) (modeline-mousable t) (modeline-mousable-minor-mode t) (primary-selection t) (zmacs-region t) (flymake-errline t) (flymake-warnline t)))"##
    ]];
    ParityBatchCase::value(
        "records_the_complete_palette_and_preserves_legacy_face_contracts",
        elisp_form,
        expect,
    )
}

fn source_loading_registers_one_exact_directory_and_the_nil_branch_registers_none()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((source (getenv "NEOMACS_PACKAGE_SOURCE"))
       (directory
        (file-name-as-directory (file-name-directory source)))
       (exact-count
        (lambda ()
          (cl-count directory custom-theme-load-path :test #'equal)))
       initial after-first after-second nil-load)
  (unwind-protect
      (progn
        (setq initial
              (list
               :exact-directory t
               :present (and (member directory custom-theme-load-path) t)
               :count (funcall exact-count)
               :enabled (and (custom-theme-enabled-p 'zen-and-art) t)))
        (load-theme 'zen-and-art t)
        (setq after-first
              (list
               :count (funcall exact-count)
               :enabled (and (custom-theme-enabled-p 'zen-and-art) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'zen-and-art 'theme-settings))))
        (load-theme 'zen-and-art t)
        (setq after-second
              (list
               :count (funcall exact-count)
               :enabled (and (custom-theme-enabled-p 'zen-and-art) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'zen-and-art 'theme-settings))))
        (disable-theme 'zen-and-art)
        (let* ((without-directory
                (delete directory (copy-sequence custom-theme-load-path)))
               (custom-theme-load-path (copy-sequence without-directory))
               (load-file-name nil))
          (with-temp-buffer
            (insert-file-contents source)
            (eval-buffer))
          (setq nil-load
                (list
                 :load-file-name load-file-name
                 :unchanged (equal custom-theme-load-path without-directory)
                 :directory-added
                 (and (member directory custom-theme-load-path) t)
                 :provided (featurep 'zen-and-art-theme)
                 :settings (length (get 'zen-and-art 'theme-settings))))))
    (when (custom-theme-enabled-p 'zen-and-art)
      (disable-theme 'zen-and-art)))
  (list
   :initial initial
   :after-first after-first
   :after-second after-second
   :reload-deduplicated (equal after-first after-second)
   :nil-load nil-load))
"####;
    let expect = expect![[
        r##"OK (:initial (:exact-directory t :present t :count 1 :enabled nil) :after-first (:count 1 :enabled t :themes (zen-and-art) :settings 36) :after-second (:count 1 :enabled t :themes (zen-and-art) :settings 36) :reload-deduplicated t :nil-load (:load-file-name nil :unchanged t :directory-added nil :provided t :settings 72))"##
    ]];
    ParityBatchCase::value(
        "source_loading_registers_one_exact_directory_and_the_nil_branch_registers_none",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn stacking_the_theme_and_disabling_it_restores_the_users_previous_theme() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((faces
        '(default cursor region hl-line isearch mode-line
          font-lock-keyword-face))
       baseline stacked restored)
  (unwind-protect
      (progn
        (require 'hl-line)
        (eval
         '(deftheme neomacs-zen-and-art-baseline
            "Theme already used before zen-and-art."))
        (custom-theme-set-faces
         'neomacs-zen-and-art-baseline
         '(default ((t (:foreground "#eceff4" :background "#20242b"))))
         '(cursor ((t (:foreground "#20242b" :background "#e5b567"))))
         '(region ((t (:background "#41536b"))))
         '(fringe ((t (:background "#292e38"))))
         '(hl-line ((t (:background "#303641"))))
         '(highlight ((t (:background "#506070"))))
         '(isearch ((t (:foreground "#20242b" :background "#e5b567"))))
         '(secondary-selection ((t (:background "#4a5668"))))
         '(minibuffer-prompt ((t (:foreground "#8fc7ff" :weight bold))))
         '(italic ((t (:slant italic))))
         '(underline ((t (:underline t))))
         '(font-lock-keyword-face ((t (:foreground "#c792ea"))))
         '(font-lock-string-face ((t (:foreground "#addb67"))))
         '(font-lock-comment-face ((t (:foreground "#7f8c98" :slant italic))))
         '(font-lock-warning-face ((t (:foreground "#ff5370" :weight bold)))))
        (provide-theme 'neomacs-zen-and-art-baseline)
        (enable-theme 'neomacs-zen-and-art-baseline)
        (setq baseline
              (list :themes (copy-sequence custom-enabled-themes)
                    :faces (neomacs-zen-and-art-test-face-state faces)))
        (load-theme 'zen-and-art t)
        (setq stacked
              (list :themes (copy-sequence custom-enabled-themes)
                    :zen-enabled (and (custom-theme-enabled-p 'zen-and-art) t)
                    :baseline-enabled
                    (and (custom-theme-enabled-p
                          'neomacs-zen-and-art-baseline)
                         t)
                    :faces (neomacs-zen-and-art-test-face-state faces)))
        (disable-theme 'zen-and-art)
        (setq restored
              (list :themes (copy-sequence custom-enabled-themes)
                    :zen-enabled (and (custom-theme-enabled-p 'zen-and-art) t)
                    :zen-known (and (custom-theme-p 'zen-and-art) t)
                    :faces (neomacs-zen-and-art-test-face-state faces))))
    (dolist (theme '(zen-and-art neomacs-zen-and-art-baseline))
      (when (custom-theme-enabled-p theme)
        (disable-theme theme))))
  (list
   :baseline baseline
   :stacked stacked
   :restored restored
   :restored-matches-baseline
   (equal (plist-get baseline :faces) (plist-get restored :faces))
   :zen-changed-appearance
   (not (equal (plist-get baseline :faces) (plist-get stacked :faces)))))
"####;
    let expect = expect![[
        r##"OK (:baseline (:themes (neomacs-zen-and-art-baseline) :faces ((:face default :defined t :foreground "#eceff4" :background "#20242b" :resolved-foreground "#eceff4" :resolved-background "#20242b" :weight normal :slant normal :underline nil :inherit nil) (:face cursor :defined t :foreground "#20242b" :background "#e5b567" :resolved-foreground "#20242b" :resolved-background "#e5b567" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#41536b" :resolved-foreground "#eceff4" :resolved-background "#41536b" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#303641" :resolved-foreground "#eceff4" :resolved-background "#303641" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face isearch :defined t :foreground "#20242b" :background "#e5b567" :resolved-foreground "#20242b" :resolved-background "#e5b567" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background unspecified :resolved-foreground "#eceff4" :resolved-background "#20242b" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#c792ea" :background unspecified :resolved-foreground "#c792ea" :resolved-background "#20242b" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified))) :stacked (:themes (zen-and-art neomacs-zen-and-art-baseline) :zen-enabled t :baseline-enabled t :faces ((:face default :defined t :foreground "#d2dec4" :background "#191717" :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight normal :slant normal :underline nil :inherit nil) (:face cursor :defined t :foreground "#a7a7a7" :background "#e5b567" :resolved-foreground "#a7a7a7" :resolved-background "#e5b567" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#999966" :resolved-foreground "#d2dec4" :resolved-background "#999966" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#252323" :resolved-foreground "#d2dec4" :resolved-background "#252323" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face isearch :defined t :foreground "#20242b" :background "#555555" :resolved-foreground "#20242b" :resolved-background "#555555" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background unspecified :resolved-foreground "#d2dec4" :resolved-background "#191717" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#AE5825" :background unspecified :resolved-foreground "#AE5825" :resolved-background "#191717" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified))) :restored (:themes (neomacs-zen-and-art-baseline) :zen-enabled nil :zen-known t :faces ((:face default :defined t :foreground "#eceff4" :background "#20242b" :resolved-foreground "#eceff4" :resolved-background "#20242b" :weight normal :slant normal :underline nil :inherit nil) (:face cursor :defined t :foreground "#20242b" :background "#e5b567" :resolved-foreground "#20242b" :resolved-background "#e5b567" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#41536b" :resolved-foreground "#eceff4" :resolved-background "#41536b" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#303641" :resolved-foreground "#eceff4" :resolved-background "#303641" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face isearch :defined t :foreground "#20242b" :background "#e5b567" :resolved-foreground "#20242b" :resolved-background "#e5b567" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background unspecified :resolved-foreground "#eceff4" :resolved-background "#20242b" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#c792ea" :background unspecified :resolved-foreground "#c792ea" :resolved-background "#20242b" :weight unspecified :slant unspecified :underline unspecified :inherit unspecified))) :restored-matches-baseline t :zen-changed-appearance t)"##
    ]];
    ParityBatchCase::value(
        "stacking_the_theme_and_disabling_it_restores_the_users_previous_theme",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_the_theme_repaints_a_real_elisp_editing_session(),
        selecting_and_searching_real_text_uses_the_ui_palette_without_editing_it(),
        records_the_complete_palette_and_preserves_legacy_face_contracts(),
        source_loading_registers_one_exact_directory_and_the_nil_branch_registers_none(),
        stacking_the_theme_and_disabling_it_restores_the_users_previous_theme(),
    ]
}
