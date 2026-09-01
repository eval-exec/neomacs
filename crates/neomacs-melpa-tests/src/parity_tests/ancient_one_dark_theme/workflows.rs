use expect_test::expect;

use super::ParityBatchCase;

fn loading_the_theme_enables_it_and_disabling_it_puts_the_editor_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_the_theme_enables_it_and_disabling_it_puts_the_editor_back",
        r##"
(let ((before (list :enabled custom-enabled-themes
                    :known (and (memq 'ancient-one-dark custom-known-themes) t)
                    :available
                    (and (memq 'ancient-one-dark (custom-available-themes)) t)
                    :custom-theme-p (and (custom-theme-p 'ancient-one-dark) t)
                    :string-face (aod-test-appearance 'font-lock-string-face)
                    :line-number (aod-test-appearance 'line-number))))
  (load-theme 'ancient-one-dark t)
  (let ((loaded (list :enabled custom-enabled-themes
                      :custom-theme-p (and (custom-theme-p 'ancient-one-dark) t)
                      :settings (length (aod-test-settings))
                      :distinct-faces
                      (length (delete-dups (aod-test-face-names)))
                      :variables-themed
                      (length (seq-filter
                               (lambda (setting) (eq (car setting) 'theme-value))
                               (get 'ancient-one-dark 'theme-settings)))
                      :string-face (aod-test-appearance 'font-lock-string-face)
                      :line-number (aod-test-appearance 'line-number))))
    (disable-theme 'ancient-one-dark)
    (let ((disabled (list :enabled custom-enabled-themes
                          :known
                          (and (memq 'ancient-one-dark custom-known-themes) t)
                          :custom-theme-p
                          (and (custom-theme-p 'ancient-one-dark) t)
                          :theme-face-property (get 'line-number 'theme-face)
                          :string-face
                          (aod-test-appearance 'font-lock-string-face)
                          :line-number (aod-test-appearance 'line-number))))
      (enable-theme 'ancient-one-dark)
      (list :before-loading before
            :while-loaded loaded
            :after-disabling disabled
            :after-re-enabling
            (list :enabled custom-enabled-themes
                  :settings (length (aod-test-settings))
                  :line-number (aod-test-appearance 'line-number))))))
"##,
        expect![[
            r#"OK (:before-loading (:enabled nil :known t :available t :custom-theme-p t :string-face (font-lock-string-face :slant italic) :line-number (line-number :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box nil :inherit (shadow default) :height 1)) :while-loaded (:enabled (ancient-one-dark) :custom-theme-p t :settings 202 :distinct-faces 199 :variables-themed 0 :string-face (font-lock-string-face :slant italic) :line-number (line-number :background "gray" :inherit fringe)) :after-disabling (:enabled nil :known t :custom-theme-p t :theme-face-property nil :string-face (font-lock-string-face :slant italic) :line-number (line-number :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box nil :inherit (shadow default) :height 1)) :after-re-enabling (:enabled (ancient-one-dark) :settings 202 :line-number (line-number :background "gray" :inherit fringe)))"#
        ]],
    )
}

fn on_a_display_below_eighty_nine_colours_only_the_ungated_faces_reach_the_user() -> ParityBatchCase
{
    ParityBatchCase::value(
        "on_a_display_below_eighty_nine_colours_only_the_ungated_faces_reach_the_user",
        r##"
(progn
  (load-theme 'ancient-one-dark t)
  (list
   :display (aod-test-display)
   :clauses (aod-test-clause-kinds)
   ;; Every themed face a stock editor already has, as it now looks.  All of
   ;; them are gated on 89 colours, so each keeps whatever its own `defface'
   ;; gives a monochrome terminal -- except the four line-number settings,
   ;; which are written `((t ...))' and do apply.
   :themed-faces-that-exist (aod-test-existing-faces)
   :appearance
   (mapcar #'aod-test-appearance (aod-test-existing-faces))
   ;; The same faces with the theme switched off, so the snapshot shows
   ;; exactly which of them the theme changed on this display.
   :appearance-without-the-theme
   (progn (disable-theme 'ancient-one-dark)
          (mapcar #'aod-test-appearance (aod-test-existing-faces)))))
"##,
        expect![[
            r#"OK (:display (:color-cells 0 :visual-class static-gray :color-p nil :graphic-p nil :gated-clause-matches nil :ungated-clause-matches t) :clauses (:gated-on-89-colors 191 :ungated (line-number-current-line line-number line-number-current-line line-number jde-java-font-lock-number-face jde-jave-font-lock-protected-face jde-java-font-lock-modifier-face jde-java-font-lock-constant-face jde-java-font-lock-private-face jde-java-font-lock-public-face jde-java-font-lock-package-face) :other-clauses nil) :themed-faces-that-exist (default tab-line line-number-current-line line-number lazy-highlight trailing-whitespace warning link minibuffer-prompt vertical-border mode-line-emphasis mode-line-highlight mode-line-buffer-id mode-line-inactive mode-line isearch cursor fringe highlight region font-lock-warning-face font-lock-variable-name-face font-lock-type-face font-lock-string-face font-lock-keyword-face font-lock-function-name-face font-lock-doc-face font-lock-constant-face font-lock-negation-char-face font-lock-comment-face font-lock-builtin-face) :appearance ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil :height 1) (tab-line :background "grey") (line-number-current-line :foreground "white" :background "gray" :weight bold :inherit fringe) (line-number :background "gray" :inherit fringe) (lazy-highlight :underline t) (trailing-whitespace) (warning :weight bold) (link :underline t :inherit underline) (minibuffer-prompt :foreground "cyan") (vertical-border :inherit mode-line-inactive) (mode-line-emphasis :weight bold) (mode-line-highlight :inherit highlight) (mode-line-buffer-id :weight bold) (mode-line-inactive :inherit mode-line) (mode-line) (isearch) (cursor :background "white") (fringe :background "gray") (highlight) (region) (font-lock-warning-face :weight bold :inherit error) (font-lock-variable-name-face :weight bold :slant italic) (font-lock-type-face :weight bold :underline t) (font-lock-string-face :slant italic) (font-lock-keyword-face :weight bold) (font-lock-function-name-face :weight bold) (font-lock-doc-face :slant italic :inherit font-lock-string-face) (font-lock-constant-face :weight bold :underline t) (font-lock-negation-char-face) (font-lock-comment-face :weight bold :slant italic) (font-lock-builtin-face :weight bold)) :appearance-without-the-theme ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil :height 1) (tab-line :background "grey") (line-number-current-line :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box nil :inherit line-number :height 1) (line-number :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box nil :inherit (shadow default) :height 1) (lazy-highlight :underline t) (trailing-whitespace) (warning :weight bold) (link :underline t :inherit underline) (minibuffer-prompt :foreground "cyan") (vertical-border :inherit mode-line-inactive) (mode-line-emphasis :weight bold) (mode-line-highlight :inherit highlight) (mode-line-buffer-id :weight bold) (mode-line-inactive :inherit mode-line) (mode-line) (isearch) (cursor :background "white") (fringe :background "gray") (highlight) (region) (font-lock-warning-face :weight bold :inherit error) (font-lock-variable-name-face :weight bold :slant italic) (font-lock-type-face :weight bold :underline t) (font-lock-string-face :slant italic) (font-lock-keyword-face :weight bold) (font-lock-function-name-face :weight bold) (font-lock-doc-face :slant italic :inherit font-lock-string-face) (font-lock-constant-face :weight bold :underline t) (font-lock-negation-char-face) (font-lock-comment-face :weight bold :slant italic) (font-lock-builtin-face :weight bold)))"#
        ]],
    )
}

fn the_theme_registers_two_hundred_and_two_face_specs_and_fifteen_colours() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_theme_registers_two_hundred_and_two_face_specs_and_fifteen_colours",
        r##"
(progn
  (load-theme 'ancient-one-dark t)
  (list :palette (aod-test-palette-counts)
        :specs (aod-test-settings)))
"##,
        expect![[
            r##"OK (:palette (("#312843" . 37) ("#d1cad5" . 33) ("#c0bac4" . 16) ("#625c70" . 4) ("#736a8c" . 3) ("#413952" . 11) ("#767278" . 12) ("white" . 1) ("#8b76bc" . 22) ("#b273b1" . 40) ("#8e7ed9" . 12) ("#f3cb89" . 9) ("#524a61" . 16) ("#b0aab3" . 12) ("#fad13d" . 11)) :specs ((default (((class color) (min-colors 89)) (:background "#312843" :foreground "#d1cad5"))) (tab-line-highlight (((class color) (min-colors 89)) (:background "#312843" :foreground "#c0bac4" :box (:line-width 4 :color "#312843")))) (tab-line-tab-current (((class color) (min-colors 89)) (:background "#625c70" :foreground "#d1cad5" :box (:line-width 4 :color "#625c70")))) (tab-line-tab-inactive (((class color) (min-colors 89)) (:inherit tab-line :foreground "#736a8c"))) (tab-line-tab (((class color) (min-colors 89)) (:inherit tab-line))) (tab-line (((class color) (min-colors 89)) (:inherit fringe :box (:line-width 5 :color "#312843")))) (line-number-current-line (t (:background "#413952" :foreground "#d1cad5"))) (line-number (t (:background "#413952" :foreground "#767278"))) (line-number-current-line (t (:inherit fringe :foreground "white" :weight bold))) (line-number (t (:inherit fringe))) (jde-java-font-lock-number-face (t (:foreground "#d1cad5"))) (jde-jave-font-lock-protected-face (t (:foreground "#8b76bc"))) (jde-java-font-lock-modifier-face (t (:foreground "#c0bac4"))) (jde-java-font-lock-constant-face (t (:foreground "#b273b1"))) (jde-java-font-lock-private-face (t (:foreground "#8b76bc"))) (jde-java-font-lock-public-face (t (:foreground "#8b76bc"))) (jde-java-font-lock-package-face (t (:foreground "#d1cad5"))) (web-mode-html-tag-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (web-mode-warning-face (((class color) (min-colors 89)) (:inherit font-lock-warning-face))) (web-mode-html-attr-value-face (((class color) (min-colors 89)) (:foreground "#8b76bc"))) (web-mode-html-attr-name-face (((class color) (min-colors 89)) (:foreground "#8e7ed9"))) (web-mode-type-face (((class color) (min-colors 89)) (:inherit font-lock-type-face))) (web-mode-string-face (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (web-mode-function-name-face (((class color) (min-colors 89)) (:inherit font-lock-function-name-face))) (web-mode-doctype-face (((class color) (min-colors 89)) (:inherit font-lock-comment-face))) (web-mode-keyword-face (((class color) (min-colors 89)) (:foreground "#8b76bc"))) (web-mode-constant-face (((class color) (min-colors 89)) (:inherit font-lock-constant-face))) (web-mode-comment-face (((class color) (min-colors 89)) (:inherit font-lock-comment-face))) (web-mode-builtin-face (((class color) (min-colors 89)) (:inherit font-lock-builtin-face))) (company-template-field (((class color) (min-colors 89)) (:inherit region))) (company-tooltip-selection (((class color) (min-colors 89)) (:background "#524a61" :foreground "#b0aab3"))) (company-tooltip-mouse (((class color) (min-colors 89)) (:inherit highlight))) (company-tooltip-common-selection (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (company-tooltip-common (((class color) (min-colors 89)) (:foreground "#b0aab3"))) (company-tooltop-annotation (((class color) (min-colors 89)) (:foreground "#b273b1"))) (company-tooltip (((class color) (min-colors 89)) (:foreground "#c0bac4" :background "#312843" :bold t))) (company-scrollbar-fg (((class color) (min-colors 89)) (:foreground "#8b76bc"))) (company-scrollbar-bg (((class color) (min-colors 89)) (:background "#524a61"))) (company-preview-search (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#312843"))) (company-preview-common (((class color) (min-colors 89)) (:foreground "#413952" :foreground "#b0aab3"))) (company-preview (((class color) (min-colors 89)) (:background "#312843" :foreground "#d1cad5"))) (company-echo-common (((class color) (min-colors 89)) (:foreground "#312843" :background "#d1cad5"))) (helm-bookmark-w3m (((class color) (min-colors 89)) (:foreground "#b273b1"))) (helm-source-go-package-godoc-description (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (helm-moccur-buffer (((class color) (min-colors 89)) (:foreground "#8e7ed9" :background "#312843"))) (helm-grep-running (((class color) (min-colors 89)) (:foreground "#8e7ed9" :background "#312843"))) (helm-grep-match (((class color) (min-colors 89)) (:foreground nil :background nil :inherit helm-match))) (helm-grep-lineno (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843"))) (helm-grep-finish (((class color) (min-colors 89)) (:foreground "#c0bac4" :background "#312843"))) (helm-grep-file (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843"))) (helm-grep-cmd-line (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843"))) (helm-ff-prefix (((class color) (min-colors 89)) (:foreground "#312843" :background "#8b76bc" :weight normal))) (helm-ff-symlink (((class color) (min-colors 89)) (:foreground "#8b76bc" :background "#312843" :weight bold))) (helm-ff-invalid-symlink (((class color) (min-colors 89)) (:foreground "#fad13d" :background "#312843" :weight bold))) (helm-ff-executable (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843" :weight normal))) (helm-ff-file (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843" :weight normal))) (helm-ff-directory (((class color) (min-colors 89)) (:foreground "#8e7ed9" :background "#312843" :weight bold))) (helm-buffer-size (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843"))) (helm-buffer-saved-out (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843"))) (helm-buffer-process (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#312843"))) (helm-buffer-not-saved (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#312843"))) (helm-time-zone-home (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#312843"))) (helm-time-zone-current (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#312843"))) (helm-separator (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#312843"))) (helm-candidate-number (((class color) (min-colors 89)) (:foreground "#312843" :background "#d1cad5"))) (helm-visible-mark (((class color) (min-colors 89)) (:foreground "#312843" :background "#524a61"))) (helm-selection-line (((class color) (min-colors 89)) (:background "#413952"))) (helm-selection (((class color) (min-colors 89)) (:background "#413952" :underline nil))) (helm-source-header (((class color) (min-colors 89)) (:foreground "#8b76bc" :background "#312843" :underline nil :weight bold))) (helm-header (((class color) (min-colors 89)) (:foreground "#c0bac4" :background "#312843" :underline nil :box nil))) (rainbow-delimiters-unmatched-face (((class color) (min-colors 89)) :foreground "#fad13d")) (term-color-white (((class color) (min-colors 89)) (:foreground "#c0bac4" :background "#c0bac4"))) (term-color-cyan (((class color) (min-colors 89)) (:foreground "#f3cb89" :background "#f3cb89"))) (term-color-magenta (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#b273b1"))) (term-color-yellow (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#d1cad5"))) (term-color-green (((class color) (min-colors 89)) (:foreground "#b273b1" :background "#524a61"))) (term-color-red (((class color) (min-colors 89)) (:foreground "#8b76bc" :background "#524a61"))) (term-color-blue (((class color) (min-colors 89)) (:foreground "#8e7ed9" :background "#8e7ed9"))) (term-color-black (((class color) (min-colors 89)) (:foreground "#524a61" :background "#524a61"))) (term (((class color) (min-colors 89)) (:foreground "#d1cad5" :background "#312843"))) (lazy-highlight (((class color) (min-colors 89)) (:foreground "#c0bac4" :background "#524a61"))) (magit-diff-file-header (((class color) (min-colors 89)) (:foreground "#c0bac4" :background "#524a61"))) (magit-hash (((class color) (min-colors 89)) (:foreground "#c0bac4"))) (magit-log-author (((class color) (min-colors 89)) (:foreground "#b0aab3"))) (magit-branch (((class color) (min-colors 89)) (:foreground "#b273b1" :weight bold))) (magit-process-ng (((class color) (min-colors 89)) (:foreground "#fad13d" :weight bold))) (magit-process-ok (((class color) (min-colors 89)) (:foreground "#8e7ed9" :weight bold))) (magit-diffstat-removed (((class color) (min-colors 89)) (:foreground "#d1cad5"))) (magit-diffstat-added (((class color) (min-colors 89)) (:foreground "#b273b1"))) (magit-diff-context-highlight (((class color) (min-colors 89)) (:background "#524a61" :foreground "#b0aab3"))) (magit-hunk-heading-highlight (((class color) (min-colors 89)) (:background "#524a61"))) (magit-section-highlight (((class color) (min-colors 89)) (:background "#413952"))) (magit-hunk-heading (((class color) (min-colors 89)) (:background "#524a61"))) (magit-section-heading (((class color) (min-colors 89)) (:foreground "#8b76bc" :weight bold))) (magit-item-highlight (((class color) (min-colors 89)) :background "#524a61")) (rainbow-delimiters-depth-8-face (((class color) (min-colors 89)) :foreground "#d1cad5")) (rainbow-delimiters-depth-7-face (((class color) (min-colors 89)) :foreground "#b273b1")) (rainbow-delimiters-depth-6-face (((class color) (min-colors 89)) :foreground "#d1cad5")) (rainbow-delimiters-depth-5-face (((class color) (min-colors 89)) :foreground "#8b76bc")) (rainbow-delimiters-depth-4-face (((class color) (min-colors 89)) :foreground "#b273b1")) (rainbow-delimiters-depth-3-face (((class color) (min-colors 89)) :foreground "#d1cad5")) (rainbow-delimiters-depth-2-face (((class color) (min-colors 89)) :foreground "#b273b1")) (rainbow-delimiters-depth-1-face (((class color) (min-colors 89)) :foreground "#d1cad5")) (trailing-whitespace (((class color) (min-colors 89)) :foreground nil :background "#fad13d")) (slime-repl-inputed-output-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (undo-tree-visualizer-register-face (((class color) (min-colors 89)) :foreground "#b273b1")) (undo-tree-visualizer-unmodified-face (((class color) (min-colors 89)) :foreground "#d1cad5")) (undo-tree-visualizer-default-face (((class color) (min-colors 89)) :foreground "#c0bac4")) (undo-tree-visualizer-current-face (((class color) (min-colors 89)) :foreground "#b273b1")) (icompletep-determined (((class color) (min-colors 89)) :foreground "#b273b1")) (info-string (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (info-quoted-name (((class color) (min-colors 89)) (:foreground "#b273b1"))) (ac-completion-face (((class color) (min-colors 89)) (:underline t :foreground "#8b76bc"))) (warning (((class color) (min-colors 89)) (:foreground "#fad13d"))) (js3-instance-member-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (js3-jsdoc-tag-face (((class color) (min-colors 89)) (:foreground "#8b76bc"))) (js3-function-param-face (((class color) (min-colors 89)) (:foreground "#c0bac4"))) (js3-external-variable-face (((class color) (min-colors 89)) (:foreground "#d1cad5"))) (js3-error-face (((class color) (min-colors 89)) (:underline "#fad13d"))) (js3-warning-face (((class color) (min-colors 89)) (:underline "#8b76bc"))) (js2-private-member (((class color) (min-colors 89)) (:foreground "#b0aab3"))) (js2-jsdoc-value (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (js2-function-param (((class color) (min-colors 89)) (:foreground "#b273b1"))) (js2-external-variable (((class color) (min-colors 89)) (:foreground "#b273b1"))) (js2-jsdoc-html-tag-name (((class color) (min-colors 89)) (:foreground "#d1cad5"))) (js2-jsdoc-html-tag-delimiter (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (js2-private-function-call (((class color) (min-colors 89)) (:foreground "#b273b1"))) (ffap (((class color) (min-colors 89)) (:foreground "#767278"))) (mu4e-header-marks-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (mu4e-cited-7-face (((class color) (min-colors 89)) (:foreground "#b0aab3"))) (mu4e-cited-1-face (((class color) (min-colors 89)) (:foreground "#c0bac4"))) (mu4e-view-url-number-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (gnus-header-subject (((class color) (min-colors 89)) (:foreground "#8e7ed9" :bold t))) (gnus-header-name (((class color) (min-colors 89)) (:foreground "#b273b1"))) (gnus-header-from (((class color) (min-colors 89)) (:foreground "#d1cad5"))) (gnus-header-content (((class color) (min-colors 89)) (:foreground "#8b76bc"))) (ivy-current-match (((class color) (min-colors 89)) (:foreground "#b0aab3" :inherit highlight :underline t))) (ido-first-match (((class color) (min-colors 89)) (:foreground "#8b76bc" :bold t))) (org-sexp-date (((class color) (min-colors 89)) (:foreground "#767278"))) (ido-only-match (((class color) (min-colors 89)) (:foreground "#fad13d"))) (font-latex-match-variable-keywords (((class color) (min-colors 89)) (:foreground "#d1cad5"))) (font-latex-match-reference-keywords (((class color) (min-colors 89)) (:foreground "#b273b1"))) (font-latex-string-face (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (font-latex-italic-face (((class color) (min-colors 89)) (:foreground "#d1cad5" :italic t))) (font-latex-bold-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (org-document-info-keyword (((class color) (min-colors 89)) (:foreground "#8e7ed9"))) (org-verbatim (((class color) (min-colors 89)) (:foreground "#767278"))) (org-ellipsis (((class color) (min-colors 89)) (:foreground "#b273b1"))) (org-scheduled-today (((class color) (min-colors 89)) (:foreground "#8e7ed9" :weight bold :height 1.2))) (org-scheduled (((class color) (min-colors 89)) (:foreground "#b273b1"))) (org-agenda-done (((class color) (min-colors 89)) (:foreground "#625c70"))) (org-agenda-date-today (((class color) (min-colors 89)) (:weight bold :foreground "#8b76bc" :height 1.4))) (org-agenda-date-weekend (((class color) (min-colors 89)) (:weight normal :foreground "#767278"))) (org-agenda-date (((class color) (min-colors 89)) (:foreground "#d1cad5" :height 1.1))) (org-agenda-structure (((class color) (min-colors 89)) (:weight bold :foreground "#b0aab3" :box (:color "#767278") :background "#524a61"))) (org-warning (((class color) (min-colors 89)) (:underline t :foreground "#fad13d"))) (org-date (((class color) (min-colors 89)) (:underline t :foreground "#d1cad5"))) (org-block (((class color) (min-colors 89)) (:foreground "#b0aab3"))) (org-done (((class color) (min-colors 89)) (:box (:line-width 1 :color "#312843") :bold t :foreground "#625c70"))) (org-todo (((class color) (min-colors 89)) (:box (:line-width 1 :color "#312843") :foreground "#8b76bc" :bold t))) (org-verse (((class color) (min-colors 89)) (:inherit org-block :slant italic))) (org-quote (((class color) (min-colors 89)) (:inherit org-block :slant italic))) (org-special-keyword (((class color) (min-colors 89)) (:foreground "#8e7ed9"))) (org-link (((class color) (min-colors 89)) (:underline t :foreground "#b273b1"))) (org-footnote (((class color) (min-colors 89)) (:underline t :foreground "#767278"))) (org-level-4 (((class color) (min-colors 89)) (:bold nil :foreground "#625c70"))) (org-level-3 (((class color) (min-colors 89)) (:bold t :foreground "#767278"))) (org-level-2 (((class color) (min-colors 89)) (:bold nil :foreground "#b0aab3"))) (org-level-1 (((class color) (min-colors 89)) (:bold t :foreground "#c0bac4" :height 1.1))) (org-hide (((class color) (min-colors 89)) (:foreground "#767278"))) (org-code (((class color) (min-colors 89)) (:foreground "#c0bac4"))) (link (((class color) (min-colors 89)) (:foreground "#b273b1" :underline t))) (default-italic (((class color) (min-colors 89)) (:italic t))) (minibuffer-prompt (((class color) (min-colors 89)) (:bold t :foreground "#8b76bc"))) (vertical-border (((class color) (min-colors 89)) (:foreground "#767278"))) (mode-line-emphasis (((class color) (min-colors 89)) (:foreground "#767278"))) (mode-line-highlight (((class color) (min-colors 89)) (:foreground "#8b76bc" :box nil :weight bold))) (mode-line-buffer-id (((class color) (min-colors 89)) (:bold t :foreground "#8e7ed9" :background nil))) (mode-line-inactive (((class color) (min-colors 89)) (:box (:line-width 1 :color "#413952" :style pressed-button) :foreground "#d1cad5" :background "#312843" :weight normal))) (mode-line (((class color) (min-colors 89)) (:box (:line-width 3 :color "#413952") :bold t :foreground "#c0bac4" :background "#413952"))) (isearch (((class color) (min-colors 89)) (:bold t :foreground "#fad13d" :background "#524a61"))) (show-paren-match-face (((class color) (min-colors 89)) (:background "#fad13d"))) (cursor (((class color) (min-colors 89)) (:background "#524a61"))) (fringe (((class color) (min-colors 89)) (:background "#312843" :foreground "#767278"))) (centaur-tabs-unselected (((class color) (min-colors 89)) (:background "#413952"))) (centaur-tabs-selected (((class color) (min-colors 89)) (:background "#312843"))) (hl-line (((class color) (min-colors 89)) (:background "#413952"))) (highlight (((class color) (min-colors 89)) (:foreground "#b0aab3" :background "#524a61"))) (region (((class color) (min-colors 89)) (:background "#d1cad5" :foreground "#312843"))) (term-color-black (((class color) (min-colors 89)) (:foreground "#c0bac4" :background nil))) (font-lock-warning-face (((class color) (min-colors 89)) (:foreground "#fad13d" :background "#413952"))) (font-lock-variable-name-face (((class color) (min-colors 89)) (:foreground "#d1cad5"))) (font-lock-type-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (font-lock-string-face (((class color) (min-colors 89)) (:foreground "#f3cb89"))) (font-lock-keyword-face (((class color) (min-colors 89)) (:bold ((class color) (min-colors 89)) :foreground "#8b76bc"))) (font-lock-function-name-face (((class color) (min-colors 89)) (:foreground "#8e7ed9"))) (font-lock-doc-face (((class color) (min-colors 89)) (:foreground "#736a8c"))) (font-lock-constant-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (font-lock-reference-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (font-lock-negation-char-face (((class color) (min-colors 89)) (:foreground "#b273b1"))) (font-lock-comment-face (((class color) (min-colors 89)) (:foreground "#736a8c"))) (font-lock-builtin-face (((class color) (min-colors 89)) (:foreground "#b273b1")))))"##
        ]],
    )
}

fn a_package_whose_faces_the_theme_styles_picks_them_up_when_it_is_loaded_later() -> ParityBatchCase
{
    ParityBatchCase::value(
        "a_package_whose_faces_the_theme_styles_picks_them_up_when_it_is_loaded_later",
        r##"
(progn
  (load-theme 'ancient-one-dark t)
  (let ((before (list :company-preview-common (facep 'company-preview-common)
                      :jde-package (facep 'jde-java-font-lock-package-face))))
    ;; What loading company and jde-mode does: define the faces the theme has
    ;; already been told about.  Custom applies the stored spec at `defface'
    ;; time, so the package's own colours are overridden immediately -- but
    ;; only where the theme's display clause matches this display.
    (eval '(defface company-preview-common
             '((t (:foreground "orange")))
             "A face this editor did not have when the theme was loaded.")
          t)
    (eval '(defface jde-java-font-lock-package-face
             '((t (:foreground "orange")))
             "A face this editor did not have when the theme was loaded.")
          t)
    (let ((after (list (aod-test-appearance 'company-preview-common)
                       (aod-test-appearance 'jde-java-font-lock-package-face))))
      (disable-theme 'ancient-one-dark)
      (let ((without (list (aod-test-appearance 'company-preview-common)
                           (aod-test-appearance
                            'jde-java-font-lock-package-face))))
        (enable-theme 'ancient-one-dark)
        (list :before-the-packages-defined-them before
              :once-defined-with-the-theme-on after
              :with-the-theme-off without
              :theme-back-on
              (list (aod-test-appearance 'company-preview-common)
                    (aod-test-appearance
                     'jde-java-font-lock-package-face)))))))
"##,
        expect![[
            r##"OK (:before-the-packages-defined-them (:company-preview-common nil :jde-package nil) :once-defined-with-the-theme-on ((company-preview-common :foreground "orange") (jde-java-font-lock-package-face :foreground "#d1cad5")) :with-the-theme-off ((company-preview-common :foreground "orange") (jde-java-font-lock-package-face :foreground "orange")) :theme-back-on ((company-preview-common :foreground "orange") (jde-java-font-lock-package-face :foreground "#d1cad5")))"##
        ]],
    )
}

fn fontifying_a_real_emacs_lisp_buffer_uses_faces_the_theme_has_a_colour_for() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifying_a_real_emacs_lisp_buffer_uses_faces_the_theme_has_a_colour_for",
        r##"
(progn
  (load-theme 'ancient-one-dark t)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert ";;; settle.el --- pay an invoice -*- lexical-binding: t; -*-\n"
            "(defconst settle-currency \"EUR\"\n"
            "  \"The currency invoices are settled in.\")\n"
            "(defun settle-invoice (invoice)\n"
            "  \"Return the payment state of INVOICE.\"\n"
            "  (let ((state :unpaid))\n"
            "    (when (bufferp invoice)\n"
            "      (setq state :paid))\n"
            "    state))\n")
    (font-lock-ensure)
    (let (tokens)
      (dolist (token '("defconst" "settle-currency" "\"EUR\"" "defun"
                       "settle-invoice" "invoice)" "let" ":unpaid" "bufferp"
                       "setq" "pay an invoice"))
        (goto-char (point-min))
        (search-forward token)
        (let* ((start (- (point) (length token)))
               (face (get-text-property start 'face))
               (name (if (consp face) (car face) face)))
          (setq tokens
                (append tokens
                        (list (list token
                                    :face face
                                    :themed
                                    (aod-test-plain
                                     (cadr (assq 'ancient-one-dark
                                                 (and name
                                                      (get name 'theme-face)))))
                                    :looks-like
                                    (and name (aod-test-appearance name))))))))
      (list :buffer (buffer-substring-no-properties (point-min) (point-max))
            :tokens tokens))))
"##,
        expect![[
            r##"OK (:buffer ";;; settle.el --- pay an invoice -*- lexical-binding: t; -*-\n(defconst settle-currency \"EUR\"\n  \"The currency invoices are settled in.\")\n(defun settle-invoice (invoice)\n  \"Return the payment state of INVOICE.\"\n  (let ((state :unpaid))\n    (when (bufferp invoice)\n      (setq state :paid))\n    state))\n" :tokens (("defconst" :face font-lock-keyword-face :themed ((((class color) (min-colors 89)) (:bold ((class color) (min-colors 89)) :foreground "#8b76bc"))) :looks-like (font-lock-keyword-face :weight bold)) ("settle-currency" :face font-lock-variable-name-face :themed ((((class color) (min-colors 89)) (:foreground "#d1cad5"))) :looks-like (font-lock-variable-name-face :weight bold :slant italic)) ("\"EUR\"" :face font-lock-string-face :themed ((((class color) (min-colors 89)) (:foreground "#f3cb89"))) :looks-like (font-lock-string-face :slant italic)) ("defun" :face font-lock-keyword-face :themed ((((class color) (min-colors 89)) (:bold ((class color) (min-colors 89)) :foreground "#8b76bc"))) :looks-like (font-lock-keyword-face :weight bold)) ("settle-invoice" :face font-lock-function-name-face :themed ((((class color) (min-colors 89)) (:foreground "#8e7ed9"))) :looks-like (font-lock-function-name-face :weight bold)) ("invoice)" :face nil :themed nil :looks-like nil) ("let" :face font-lock-keyword-face :themed ((((class color) (min-colors 89)) (:bold ((class color) (min-colors 89)) :foreground "#8b76bc"))) :looks-like (font-lock-keyword-face :weight bold)) (":unpaid" :face font-lock-builtin-face :themed ((((class color) (min-colors 89)) (:foreground "#b273b1"))) :looks-like (font-lock-builtin-face :weight bold)) ("bufferp" :face nil :themed nil :looks-like nil) ("setq" :face font-lock-keyword-face :themed ((((class color) (min-colors 89)) (:bold ((class color) (min-colors 89)) :foreground "#8b76bc"))) :looks-like (font-lock-keyword-face :weight bold)) ("pay an invoice" :face font-lock-comment-face :themed ((((class color) (min-colors 89)) (:foreground "#736a8c"))) :looks-like (font-lock-comment-face :weight bold :slant italic))))"##
        ]],
    )
}

fn five_specs_upstream_wrote_wrong_are_registered_exactly_as_written() -> ParityBatchCase {
    ParityBatchCase::value(
        "five_specs_upstream_wrote_wrong_are_registered_exactly_as_written",
        r##"
(progn
  (load-theme 'ancient-one-dark t)
  (list
   ;; Three faces are set twice.  Custom keeps the first spec it was given
   ;; for a theme, so the second is dead: the >=27 block's own line-number
   ;; colours never replace the >=26 block's `:inherit fringe', and the Term
   ;; section's black never replaces the one written near the top.
   :set-twice (aod-test-duplicated)
   ;; `:bold ,class' passes the display clause where `t' was meant.  Compare
   ;; `isearch', which the same file writes correctly.
   :bold-given-a-display-clause
   (list (assq 'font-lock-keyword-face (aod-test-settings))
         (assq 'isearch (aod-test-settings)))
   ;; `:foreground' twice in one plist; the second wins, so bg2 is dead and
   ;; the background this face was meant to get is never set.
   :foreground-written-twice
   (assq 'company-preview-common (aod-test-settings))
   ;; Eight specs use the older (DISPLAY . ATTRIBUTES) shape rather than
   ;; (DISPLAY ATTRIBUTES).  Custom accepts both.
   :flat-attribute-lists
   (mapcar #'car
           (seq-filter (lambda (setting)
                         (keywordp (car (cdr (car (cdr setting))))))
                       (aod-test-settings)))
   ;; Faces named for something that does not exist: two typos, an
   ;; obsolete alias, and two faces Emacs dropped.  The correctly spelled
   ;; counterparts of the typos are not styled at all, so a user of jde-mode
   ;; or company never sees those two colours.
   :faces-that-never-existed
   (mapcar (lambda (face)
             (list face
                   :exists (facep face)
                   :themed (and (memq face (aod-test-face-names)) t)))
           '(jde-jave-font-lock-protected-face
             jde-java-font-lock-protected-face
             company-tooltop-annotation
             company-tooltip-annotation
             show-paren-match-face
             font-lock-reference-face
             default-italic))))
"##,
        expect![[
            r##"OK (:set-twice ((line-number-current-line :written-in-file-order (((t (:inherit fringe :foreground "white" :weight bold))) ((t (:background "#413952" :foreground "#d1cad5")))) :registered ((t (:inherit fringe :foreground "white" :weight bold)))) (line-number :written-in-file-order (((t (:inherit fringe))) ((t (:background "#413952" :foreground "#767278")))) :registered ((t (:inherit fringe)))) (term-color-black :written-in-file-order (((((class color) (min-colors 89)) (:foreground "#c0bac4" :background nil))) ((((class color) (min-colors 89)) (:foreground "#524a61" :background "#524a61")))) :registered ((((class color) (min-colors 89)) (:foreground "#c0bac4" :background nil))))) :bold-given-a-display-clause ((font-lock-keyword-face (((class color) (min-colors 89)) (:bold ((class color) (min-colors 89)) :foreground "#8b76bc"))) (isearch (((class color) (min-colors 89)) (:bold t :foreground "#fad13d" :background "#524a61")))) :foreground-written-twice (company-preview-common (((class color) (min-colors 89)) (:foreground "#413952" :foreground "#b0aab3"))) :flat-attribute-lists (rainbow-delimiters-unmatched-face magit-item-highlight rainbow-delimiters-depth-8-face rainbow-delimiters-depth-7-face rainbow-delimiters-depth-6-face rainbow-delimiters-depth-5-face rainbow-delimiters-depth-4-face rainbow-delimiters-depth-3-face rainbow-delimiters-depth-2-face rainbow-delimiters-depth-1-face trailing-whitespace undo-tree-visualizer-register-face undo-tree-visualizer-unmodified-face undo-tree-visualizer-default-face undo-tree-visualizer-current-face icompletep-determined) :faces-that-never-existed ((jde-jave-font-lock-protected-face :exists nil :themed t) (jde-java-font-lock-protected-face :exists nil :themed nil) (company-tooltop-annotation :exists nil :themed t) (company-tooltip-annotation :exists nil :themed nil) (show-paren-match-face :exists nil :themed t) (font-lock-reference-face :exists nil :themed t) (default-italic :exists nil :themed t)))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_the_theme_enables_it_and_disabling_it_puts_the_editor_back(),
        on_a_display_below_eighty_nine_colours_only_the_ungated_faces_reach_the_user(),
        the_theme_registers_two_hundred_and_two_face_specs_and_fifteen_colours(),
        a_package_whose_faces_the_theme_styles_picks_them_up_when_it_is_loaded_later(),
        fontifying_a_real_emacs_lisp_buffer_uses_faces_the_theme_has_a_colour_for(),
        five_specs_upstream_wrote_wrong_are_registered_exactly_as_written(),
    ]
}
