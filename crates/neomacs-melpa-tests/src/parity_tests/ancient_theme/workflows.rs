use expect_test::expect;

use super::ParityBatchCase;

fn loading_the_theme_enables_it_and_disabling_it_puts_the_editor_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_the_theme_enables_it_and_disabling_it_puts_the_editor_back",
        r##"
(let ((before (list :enabled (copy-sequence custom-enabled-themes)
                    :custom-theme-p (and (custom-theme-p 'ancient) t)
                    :available (and (memq 'ancient (custom-available-themes)) t)
                    :documentation (get 'ancient 'theme-documentation)
                    :default (anc-test-appearance 'default)
                    :string (anc-test-appearance 'font-lock-string-face))))
  (load-theme 'ancient t)
  (let ((loaded (list :enabled (copy-sequence custom-enabled-themes)
                      :settings (length (anc-test-settings))
                      :distinct-faces
                      (length (delete-dups (anc-test-face-names)))
                      :variables-themed
                      (length (seq-filter
                               (lambda (setting) (eq (car setting) 'theme-value))
                               (get 'ancient 'theme-settings)))
                      :default (anc-test-appearance 'default)
                      :string (anc-test-appearance 'font-lock-string-face))))
    (disable-theme 'ancient)
    (let ((disabled (list :enabled (copy-sequence custom-enabled-themes)
                          :custom-theme-p (and (custom-theme-p 'ancient) t)
                          :theme-face-property (get 'default 'theme-face)
                          :default (anc-test-appearance 'default)
                          :string
                          (anc-test-appearance 'font-lock-string-face))))
      (enable-theme 'ancient)
      (list :before-loading before
            :while-loaded loaded
            :after-disabling disabled
            :after-re-enabling
            (list :enabled (copy-sequence custom-enabled-themes)
                  :settings (length (anc-test-settings))
                  :default (anc-test-appearance 'default)
                  :string (anc-test-appearance 'font-lock-string-face))))))
"##,
        expect![[
            r##"OK (:before-loading (:enabled nil :custom-theme-p t :available t :documentation "A theme about ruins." :default (default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :slant italic)) :while-loaded (:enabled (ancient) :settings 236 :distinct-faces 236 :variables-themed 0 :default (default :foreground "#e8dcc8" :background "#1a1710" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :foreground "#c8a05a")) :after-disabling (:enabled nil :custom-theme-p t :theme-face-property nil :default (default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :slant italic)) :after-re-enabling (:enabled (ancient) :settings 236 :default (default :foreground "#e8dcc8" :background "#1a1710" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :foreground "#c8a05a")))"##
        ]],
    )
}

fn every_one_of_the_two_hundred_and_thirty_six_specs_applies_on_a_monochrome_display()
-> ParityBatchCase {
    ParityBatchCase::value(
        "every_one_of_the_two_hundred_and_thirty_six_specs_applies_on_a_monochrome_display",
        r##"
(progn
  (load-theme 'ancient t)
  (list
   :display (anc-test-display)
   ;; No spec in this theme carries a display clause, so the census is a
   ;; single entry.  That is what lets a 0-colour frame show its colours.
   :clause-census (anc-test-clause-census)
   :themed-faces-that-exist (anc-test-existing-faces)
   :appearance (mapcar #'anc-test-appearance (anc-test-existing-faces))
   ;; The same faces with the theme off, so the snapshot shows exactly what
   ;; the theme changed rather than asserting it changed something.
   :appearance-without-the-theme
   (progn (disable-theme 'ancient)
          (mapcar #'anc-test-appearance (anc-test-existing-faces)))))
"##,
        expect![[
            r##"OK (:display (:color-cells 0 :visual-class static-gray :color-p nil :graphic-p nil :ungated-clause-matches t :eighty-nine-colour-clause-matches nil) :clause-census ((t . 236)) :themed-faces-that-exist (default tab-bar-tab-inactive tab-bar-tab tab-bar header-line-highlight header-line completions-first-difference completions-common-part show-paren-match-expression show-paren-mismatch show-paren-match font-lock-misc-punctuation-face font-lock-escape-face font-lock-bracket-face font-lock-delimiter-face font-lock-property-use-face font-lock-property-name-face font-lock-operator-face font-lock-number-face font-lock-regexp-grouping-construct font-lock-regexp-grouping-backslash font-lock-negation-char-face font-lock-warning-face font-lock-preprocessor-face font-lock-constant-face font-lock-type-face font-lock-variable-use-face font-lock-variable-name-face font-lock-function-call-face font-lock-function-name-face font-lock-builtin-face font-lock-keyword-face font-lock-string-face font-lock-doc-markup-face font-lock-doc-face font-lock-comment-delimiter-face font-lock-comment-face query-replace match lazy-highlight isearch-group-2 isearch-group-1 isearch-fail isearch mode-line-highlight mode-line-emphasis mode-line-buffer-id mode-line-inactive mode-line line-number-minor-tick line-number-major-tick line-number-current-line line-number shadow nobreak-space homoglyph escape-glyph success error warning button link-visited link minibuffer-prompt window-divider-last-pixel window-divider-first-pixel window-divider vertical-border fringe secondary-selection highlight region cursor) :appearance ((default :foreground "#e8dcc8" :background "#1a1710" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) (tab-bar-tab-inactive :foreground "#665a48" :background "#0e0c09" :box (:line-width 1 :color "#2d2820")) (tab-bar-tab :foreground "#e8dcc8" :background "#2d2820" :box (:line-width 1 :color "#4a4234")) (tab-bar :foreground "#665a48" :background "#0e0c09") (header-line-highlight :foreground "#e8dcc8") (header-line :foreground "#8a7a64" :background "#0e0c09" :box (:line-width 1 :color "#2d2820")) (completions-first-difference :foreground "#f0e8d4") (completions-common-part :foreground "#3d8a6e") (show-paren-match-expression :background "#2d2820") (show-paren-mismatch :foreground "#e08c68" :background "#4c1c10") (show-paren-match :foreground "#f0e8d4" :background "#2d6652") (font-lock-misc-punctuation-face :foreground "#8a7a64") (font-lock-escape-face :foreground "#e08c68") (font-lock-bracket-face :foreground "#8a7a64") (font-lock-delimiter-face :foreground "#8a7a64") (font-lock-property-use-face :foreground "#c8b89a") (font-lock-property-name-face :foreground "#e8dcc8") (font-lock-operator-face :foreground "#8a7a64") (font-lock-number-face :foreground "#7aacc0") (font-lock-regexp-grouping-construct :foreground "#c8a05a") (font-lock-regexp-grouping-backslash :foreground "#e8cc90") (font-lock-negation-char-face :foreground "#a84428") (font-lock-warning-face :foreground "#a84428" :weight normal) (font-lock-preprocessor-face :foreground "#a84428") (font-lock-constant-face :foreground "#e08c68") (font-lock-type-face :foreground "#e8cc90") (font-lock-variable-use-face :foreground "#c8b89a") (font-lock-variable-name-face :foreground "#e8dcc8") (font-lock-function-call-face :foreground "#e8dcc8") (font-lock-function-name-face :foreground "#f0e8d4" :weight normal) (font-lock-builtin-face :foreground "#7ecfb4") (font-lock-keyword-face :foreground "#3d8a6e") (font-lock-string-face :foreground "#c8a05a") (font-lock-doc-markup-face :foreground "#c8a05a") (font-lock-doc-face :foreground "#5a4422" :slant italic) (font-lock-comment-delimiter-face :foreground "#665a48") (font-lock-comment-face :foreground "#665a48" :slant italic) (query-replace :foreground "#e8cc90" :background "#5a4422") (match :foreground "#f0e8d4" :background "#2d6652") (lazy-highlight :foreground "#8a7a64" :background "#4a4234") (isearch-group-2 :foreground "#c09080" :background "#4a2830") (isearch-group-1 :foreground "#7aacc0" :background "#2a4858") (isearch-fail :foreground "#e08c68" :background "#4c1c10") (isearch :foreground "#f0e8d4" :background "#2d6652") (mode-line-highlight :foreground "#7ecfb4") (mode-line-emphasis :foreground "#3d8a6e") (mode-line-buffer-id :foreground "#c8a05a" :weight normal) (mode-line-inactive :foreground "#665a48" :background "#1a1710" :box (:line-width 1 :color "#2d2820")) (mode-line :foreground "#8a7a64" :background "#2d2820" :box (:line-width 1 :color "#4a4234")) (line-number-minor-tick :foreground "#4a4234" :background "#1a1710") (line-number-major-tick :foreground "#8a7a64" :background "#1a1710") (line-number-current-line :foreground "#665a48" :background "#1a1710") (line-number :foreground "#4a4234" :background "#1a1710") (shadow :foreground "#665a48") (nobreak-space :foreground "#a84428" :underline t) (homoglyph :foreground "#e8cc90") (escape-glyph :foreground "#e08c68") (success :foreground "#7ecfb4") (error :foreground "#e08c68") (warning :foreground "#c8a05a") (button :foreground "#7aacc0" :underline t) (link-visited :foreground "#c8a05a" :underline t) (link :foreground "#7ecfb4" :underline t) (minibuffer-prompt :foreground "#3d8a6e" :weight normal) (window-divider-last-pixel :foreground "#0e0c09") (window-divider-first-pixel :foreground "#2d2820") (window-divider :foreground "#4a4234") (vertical-border :foreground "#4a4234") (fringe :foreground "#665a48" :background "#1a1710") (secondary-selection :background "#4a4234") (highlight :background "#2d2820") (region :background "#4a4234") (cursor :background "#3d8a6e")) :appearance-without-the-theme ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) (tab-bar-tab-inactive :background "grey" :inherit tab-bar-tab) (tab-bar-tab :background "grey" :inherit tab-bar) (tab-bar :background "grey") (header-line-highlight :inherit mode-line-highlight) (header-line :underline t :inherit mode-line) (completions-first-difference :weight bold :inherit bold) (completions-common-part) (show-paren-match-expression :underline t :inherit show-paren-match) (show-paren-mismatch) (show-paren-match :underline t :inherit underline) (font-lock-misc-punctuation-face :inherit font-lock-punctuation-face) (font-lock-escape-face :weight bold :inherit font-lock-regexp-grouping-backslash) (font-lock-bracket-face :inherit font-lock-punctuation-face) (font-lock-delimiter-face :inherit font-lock-punctuation-face) (font-lock-property-use-face :weight bold :slant italic :inherit font-lock-property-name-face) (font-lock-property-name-face :weight bold :slant italic :inherit font-lock-variable-name-face) (font-lock-operator-face) (font-lock-number-face) (font-lock-regexp-grouping-construct :weight bold :inherit bold) (font-lock-regexp-grouping-backslash :weight bold :inherit bold) (font-lock-negation-char-face) (font-lock-warning-face :weight bold :inherit error) (font-lock-preprocessor-face :weight bold :inherit font-lock-builtin-face) (font-lock-constant-face :weight bold :underline t) (font-lock-type-face :weight bold :underline t) (font-lock-variable-use-face :weight bold :slant italic :inherit font-lock-variable-name-face) (font-lock-variable-name-face :weight bold :slant italic) (font-lock-function-call-face :weight bold :inherit font-lock-function-name-face) (font-lock-function-name-face :weight bold) (font-lock-builtin-face :weight bold) (font-lock-keyword-face :weight bold) (font-lock-string-face :slant italic) (font-lock-doc-markup-face :weight bold :underline t :inherit font-lock-constant-face) (font-lock-doc-face :slant italic :inherit font-lock-string-face) (font-lock-comment-delimiter-face :weight bold :slant italic :inherit font-lock-comment-face) (font-lock-comment-face :weight bold :slant italic) (query-replace :inherit isearch) (match) (lazy-highlight :underline t) (isearch-group-2 :inherit isearch) (isearch-group-1 :inherit isearch) (isearch-fail) (isearch) (mode-line-highlight :inherit highlight) (mode-line-emphasis :weight bold) (mode-line-buffer-id :weight bold) (mode-line-inactive :inherit mode-line) (mode-line) (line-number-minor-tick :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit line-number :height 1) (line-number-major-tick :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit line-number :height 1) (line-number-current-line :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit line-number :height 1) (line-number :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit (shadow default) :height 1) (shadow) (nobreak-space) (homoglyph :foreground "cyan") (escape-glyph :foreground "cyan") (success :weight bold) (error :weight bold) (warning :weight bold) (button :underline t :inherit link) (link-visited :underline t :inherit link) (link :underline t :inherit underline) (minibuffer-prompt :foreground "cyan") (window-divider-last-pixel :foreground "gray40") (window-divider-first-pixel :foreground "gray80") (window-divider :foreground "gray60") (vertical-border :inherit mode-line-inactive) (fringe :background "gray") (secondary-selection) (highlight) (region) (cursor :background "white")))"##
        ]],
    )
}

fn the_theme_registers_two_hundred_and_thirty_six_specs_over_its_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_theme_registers_two_hundred_and_thirty_six_specs_over_its_palette",
        r##"
(progn
  (load-theme 'ancient t)
  (list
   :palette (anc-test-palette-counts)
   ;; Five colours are written as literals in the face list rather than
   ;; named in the palette `let*', and each is written twice -- once in the
   ;; `diff-' block and once in the `magit-' block.  The two blocks agree
   ;; only because both literals say the same thing, so pin the pairs.
   :literals-outside-the-palette
   (mapcar (lambda (color) (assoc color (anc-test-palette)))
           '("#122a20" "#2a2410" "#1e4434" "#7a2e18" "#4a3c10"))
   :specs (anc-test-settings)))
"##,
        expect![[
            r##"OK (:palette (("#1a1710" . 7) ("#e8dcc8" . 14) ("#e08c68" . 23) ("#4c1c10" . 6) ("#c09080" . 3) ("#e8cc90" . 19) ("#7aacc0" . 14) ("#7ecfb4" . 20) ("#8a4858" . 2) ("#c8a05a" . 22) ("#4a7a94" . 5) ("#3d8a6e" . 15) ("#8a7a64" . 19) ("#2d6652" . 9) ("#f0e8d4" . 10) ("#665a48" . 34) ("#2d2820" . 24) ("#0e0c09" . 8) ("#4a4234" . 18) ("#a84428" . 8) ("#4a3c10" . 2) ("#2a2410" . 2) ("#7a2e18" . 2) ("#1e4434" . 2) ("#122a20" . 2) ("#c8b89a" . 6) ("#5a4422" . 3) ("#4a2830" . 1) ("#2a4858" . 1)) :literals-outside-the-palette (("#122a20" magit-diff-added diff-added) ("#2a2410" magit-diff-base diff-changed) ("#1e4434" magit-diff-added-highlight diff-refine-added) ("#7a2e18" magit-diff-removed-highlight diff-refine-removed) ("#4a3c10" magit-diff-base-highlight diff-refine-changed)) :specs ((default (t (:background "#1a1710" :foreground "#e8dcc8"))) (rainbow-delimiters-unmatched-face (t (:foreground "#e08c68" :background "#4c1c10"))) (rainbow-delimiters-depth-9-face (t (:foreground "#c09080"))) (rainbow-delimiters-depth-8-face (t (:foreground "#e8cc90"))) (rainbow-delimiters-depth-7-face (t (:foreground "#7aacc0"))) (rainbow-delimiters-depth-6-face (t (:foreground "#7ecfb4"))) (rainbow-delimiters-depth-5-face (t (:foreground "#8a4858"))) (rainbow-delimiters-depth-4-face (t (:foreground "#c8a05a"))) (rainbow-delimiters-depth-3-face (t (:foreground "#4a7a94"))) (rainbow-delimiters-depth-2-face (t (:foreground "#3d8a6e"))) (rainbow-delimiters-depth-1-face (t (:foreground "#8a7a64"))) (orderless-match-face-3 (t (:foreground "#c09080"))) (orderless-match-face-2 (t (:foreground "#7aacc0"))) (orderless-match-face-1 (t (:foreground "#e8cc90"))) (orderless-match-face-0 (t (:foreground "#7ecfb4"))) (consult-highlight-match (t (:background "#2d6652" :foreground "#f0e8d4"))) (consult-grep-context (t (:foreground "#665a48"))) (consult-file (t (:foreground "#e8dcc8"))) (consult-preview-line (t (:background "#2d2820"))) (consult-preview-cursor (t (:background "#2d6652"))) (tab-bar-tab-inactive (t (:background "#0e0c09" :foreground "#665a48" :box (:line-width 1 :color "#2d2820")))) (tab-bar-tab (t (:background "#2d2820" :foreground "#e8dcc8" :box (:line-width 1 :color "#4a4234")))) (tab-bar (t (:background "#0e0c09" :foreground "#665a48"))) (header-line-highlight (t (:foreground "#e8dcc8"))) (header-line (t (:background "#0e0c09" :foreground "#8a7a64" :box (:line-width 1 :color "#2d2820")))) (pulse-highlight-start-face (t (:background "#2d6652"))) (hl-line (t (:background "#2d2820"))) (eglot-diagnostic-tag-deprecated-face (t (:foreground "#665a48" :strike-through t))) (eglot-diagnostic-tag-unnecessary-face (t (:foreground "#665a48" :slant italic))) (eglot-highlight-symbol-face (t (:background "#2d2820"))) (flymake-note (t (:underline (:style wave :color "#7aacc0")))) (flymake-warning (t (:underline (:style wave :color "#c8a05a")))) (flymake-error (t (:underline (:style wave :color "#e08c68")))) (flycheck-fringe-info (t (:foreground "#7aacc0"))) (flycheck-fringe-warning (t (:foreground "#c8a05a"))) (flycheck-fringe-error (t (:foreground "#e08c68"))) (flycheck-info (t (:underline (:style wave :color "#7aacc0")))) (flycheck-warning (t (:underline (:style wave :color "#c8a05a")))) (flycheck-error (t (:underline (:style wave :color "#e08c68")))) (treemacs-git-untracked-face (t (:foreground "#7aacc0"))) (treemacs-git-ignored-face (t (:foreground "#665a48"))) (treemacs-git-deleted-face (t (:foreground "#e08c68"))) (treemacs-git-added-face (t (:foreground "#7ecfb4"))) (treemacs-git-modified-face (t (:foreground "#c8a05a"))) (treemacs-tags-face (t (:foreground "#8a7a64"))) (treemacs-file-face (t (:foreground "#e8dcc8"))) (treemacs-directory-face (t (:foreground "#3d8a6e"))) (treemacs-root-face (t (:foreground "#e8cc90" :weight normal))) (which-key-special-key-face (t (:foreground "#e08c68"))) (which-key-highlighted-command-face (t (:foreground "#e8cc90"))) (which-key-note-face (t (:foreground "#665a48" :slant italic))) (which-key-separator-face (t (:foreground "#665a48"))) (which-key-group-description-face (t (:foreground "#c8a05a"))) (which-key-command-description-face (t (:foreground "#e8dcc8"))) (which-key-key-face (t (:foreground "#7ecfb4"))) (dired-perm-write (t (:foreground "#a84428"))) (dired-ignored (t (:foreground "#665a48"))) (dired-header (t (:foreground "#7ecfb4" :weight normal))) (dired-mark (t (:foreground "#c8a05a"))) (dired-marked (t (:foreground "#c8a05a"))) (dired-flagged (t (:foreground "#e08c68" :strike-through t))) (dired-special (t (:foreground "#8a4858"))) (dired-broken-symlink (t (:foreground "#e08c68" :strike-through t))) (dired-symlink (t (:foreground "#7aacc0"))) (dired-directory (t (:foreground "#3d8a6e"))) (magit-blame-highlight (t (:background "#4a4234" :foreground "#8a7a64"))) (magit-blame-heading (t (:background "#2d2820" :foreground "#665a48"))) (magit-signature-untrusted (t (:foreground "#c8a05a"))) (magit-signature-bad (t (:foreground "#e08c68"))) (magit-signature-good (t (:foreground "#7ecfb4"))) (magit-process-ng (t (:foreground "#e08c68"))) (magit-process-ok (t (:foreground "#3d8a6e"))) (magit-log-graph (t (:foreground "#4a4234"))) (magit-log-date (t (:foreground "#665a48"))) (magit-log-author (t (:foreground "#c8a05a"))) (magit-filename (t (:foreground "#e8dcc8"))) (magit-dimmed (t (:foreground "#665a48"))) (magit-tag (t (:foreground "#c8a05a"))) (magit-branch-current (t (:foreground "#7ecfb4" :box (:line-width -1 :color "#3d8a6e")))) (magit-branch-remote (t (:foreground "#4a7a94"))) (magit-branch-local (t (:foreground "#3d8a6e"))) (magit-hash (t (:foreground "#665a48"))) (magit-diff-base-highlight (t (:background "#4a3c10" :foreground "#e8cc90"))) (magit-diff-base (t (:background "#2a2410" :foreground "#e8cc90"))) (magit-diff-hunk-heading-highlight (t (:background "#4a4234" :foreground "#8a7a64"))) (magit-diff-hunk-heading (t (:background "#2d2820" :foreground "#665a48"))) (magit-diff-context-highlight (t (:background "#2d2820" :foreground "#8a7a64"))) (magit-diff-context (t (:foreground "#665a48"))) (magit-diff-removed-highlight (t (:background "#7a2e18" :foreground "#e08c68"))) (magit-diff-removed (t (:background "#4c1c10" :foreground "#e08c68"))) (magit-diff-added-highlight (t (:background "#1e4434" :foreground "#7ecfb4"))) (magit-diff-added (t (:background "#122a20" :foreground "#7ecfb4"))) (magit-section-highlight (t (:background "#2d2820"))) (magit-section-heading-selection (t (:foreground "#e8cc90" :weight normal))) (magit-section-heading (t (:foreground "#c8a05a" :weight normal))) (org-special-keyword (t (:foreground "#665a48"))) (org-priority (t (:foreground "#a84428"))) (org-checkbox (t (:foreground "#c8a05a"))) (org-formula (t (:foreground "#7aacc0"))) (org-table (t (:foreground "#c8b89a"))) (org-footnote (t (:foreground "#8a7a64"))) (org-link (t (:foreground "#7ecfb4" :underline t))) (org-warning (t (:foreground "#e08c68"))) (org-upcoming-deadline (t (:foreground "#c8a05a"))) (org-scheduled-previously (t (:foreground "#a84428"))) (org-scheduled-today (t (:foreground "#7ecfb4"))) (org-scheduled (t (:foreground "#3d8a6e"))) (org-agenda-structure (t (:foreground "#8a7a64"))) (org-agenda-date-weekend (t (:foreground "#4a7a94"))) (org-agenda-date-today (t (:foreground "#e8cc90" :weight normal))) (org-agenda-date (t (:foreground "#7aacc0"))) (org-tag (t (:foreground "#665a48" :weight normal))) (org-document-info-keyword (t (:foreground "#665a48"))) (org-document-info (t (:foreground "#8a7a64"))) (org-document-title (t (:foreground "#e8cc90" :weight normal :height 1.3))) (org-meta-line (t (:foreground "#665a48"))) (org-block-end-line (t (:foreground "#665a48" :background "#0e0c09"))) (org-block-begin-line (t (:foreground "#665a48" :background "#0e0c09"))) (org-block (t (:background "#0e0c09" :foreground "#c8b89a"))) (org-verbatim (t (:foreground "#e8cc90" :background "#2d2820"))) (org-code (t (:foreground "#7ecfb4" :background "#2d2820"))) (org-date-selected (t (:background "#5a4422" :foreground "#e8cc90"))) (org-date (t (:foreground "#c8a05a"))) (org-headline-done (t (:foreground "#665a48"))) (org-done (t (:foreground "#665a48"))) (org-todo (t (:foreground "#a84428" :weight normal))) (org-level-8 (t (:foreground "#c8b89a"))) (org-level-7 (t (:foreground "#e8dcc8"))) (org-level-6 (t (:foreground "#4a7a94" :weight normal))) (org-level-5 (t (:foreground "#7aacc0" :weight normal))) (org-level-4 (t (:foreground "#3d8a6e" :weight normal))) (org-level-3 (t (:foreground "#7ecfb4" :weight normal))) (org-level-2 (t (:foreground "#c8a05a" :weight normal))) (org-level-1 (t (:foreground "#e8cc90" :weight normal :height 1.15))) (evil-ex-lazy-highlight (t (:background "#4a4234" :foreground "#8a7a64"))) (evil-ex-substitute-replacement (t (:background "#2d6652" :foreground "#7ecfb4"))) (evil-ex-substitute-matches (t (:background "#4c1c10" :foreground "#e08c68"))) (evil-ex-info (t (:foreground "#e08c68"))) (completions-first-difference (t (:foreground "#f0e8d4"))) (completions-common-part (t (:foreground "#3d8a6e"))) (marginalia-file-priv-exec (t (:foreground "#3d8a6e"))) (marginalia-file-priv-dir (t (:foreground "#4a7a94"))) (marginalia-type (t (:foreground "#e8cc90"))) (marginalia-documentation (t (:foreground "#665a48" :slant italic))) (vertico-current (t (:background "#4a4234" :foreground "#f0e8d4"))) (corfu-popupinfo (t (:background "#0e0c09" :foreground "#c8b89a"))) (corfu-annotations (t (:foreground "#665a48"))) (corfu-border (t (:background "#4a4234"))) (corfu-bar (t (:background "#2d6652"))) (corfu-current (t (:background "#4a4234" :foreground "#f0e8d4"))) (corfu-default (t (:background "#2d2820" :foreground "#e8dcc8"))) (company-preview (t (:background "#2d2820"))) (company-preview-common (t (:foreground "#665a48"))) (company-scrollbar-fg (t (:background "#4a4234"))) (company-scrollbar-bg (t (:background "#2d2820"))) (company-tooltip-search (t (:background "#2d6652" :foreground "#f0e8d4"))) (company-tooltip-annotation-selection (t (:foreground "#8a7a64"))) (company-tooltip-annotation (t (:foreground "#665a48"))) (company-tooltip-common (t (:foreground "#3d8a6e"))) (company-tooltip-selection (t (:background "#4a4234" :foreground "#f0e8d4"))) (company-tooltip (t (:background "#2d2820" :foreground "#e8dcc8"))) (diff-context (t (:foreground "#665a48"))) (diff-hunk-header (t (:background "#2d2820" :foreground "#7aacc0"))) (diff-file-header (t (:background "#2d2820" :foreground "#e8dcc8"))) (diff-header (t (:background "#2d2820" :foreground "#8a7a64"))) (diff-refine-changed (t (:background "#4a3c10" :foreground "#e8cc90"))) (diff-refine-removed (t (:background "#7a2e18" :foreground "#e08c68"))) (diff-refine-added (t (:background "#1e4434" :foreground "#7ecfb4"))) (diff-changed (t (:background "#2a2410" :foreground "#e8cc90"))) (diff-removed (t (:background "#4c1c10" :foreground "#e08c68"))) (diff-added (t (:background "#122a20" :foreground "#7ecfb4"))) (show-paren-match-expression (t (:background "#2d2820"))) (show-paren-mismatch (t (:background "#4c1c10" :foreground "#e08c68"))) (show-paren-match (t (:background "#2d6652" :foreground "#f0e8d4"))) (font-lock-misc-punctuation-face (t (:foreground "#8a7a64"))) (font-lock-escape-face (t (:foreground "#e08c68"))) (font-lock-bracket-face (t (:foreground "#8a7a64"))) (font-lock-delimiter-face (t (:foreground "#8a7a64"))) (font-lock-property-use-face (t (:foreground "#c8b89a"))) (font-lock-property-name-face (t (:foreground "#e8dcc8"))) (font-lock-operator-face (t (:foreground "#8a7a64"))) (font-lock-number-face (t (:foreground "#7aacc0"))) (font-lock-regexp-grouping-construct (t (:foreground "#c8a05a"))) (font-lock-regexp-grouping-backslash (t (:foreground "#e8cc90"))) (font-lock-negation-char-face (t (:foreground "#a84428"))) (font-lock-warning-face (t (:foreground "#a84428" :weight normal))) (font-lock-preprocessor-face (t (:foreground "#a84428"))) (font-lock-constant-face (t (:foreground "#e08c68"))) (font-lock-type-face (t (:foreground "#e8cc90"))) (font-lock-variable-use-face (t (:foreground "#c8b89a"))) (font-lock-variable-name-face (t (:foreground "#e8dcc8"))) (font-lock-function-call-face (t (:foreground "#e8dcc8"))) (font-lock-function-name-face (t (:foreground "#f0e8d4" :weight normal))) (font-lock-builtin-face (t (:foreground "#7ecfb4"))) (font-lock-keyword-face (t (:foreground "#3d8a6e"))) (font-lock-string-face (t (:foreground "#c8a05a"))) (font-lock-doc-markup-face (t (:foreground "#c8a05a"))) (font-lock-doc-face (t (:foreground "#5a4422" :slant italic))) (font-lock-comment-delimiter-face (t (:foreground "#665a48"))) (font-lock-comment-face (t (:foreground "#665a48" :slant italic))) (query-replace (t (:background "#5a4422" :foreground "#e8cc90"))) (match (t (:background "#2d6652" :foreground "#f0e8d4"))) (lazy-highlight (t (:background "#4a4234" :foreground "#8a7a64"))) (isearch-group-2 (t (:background "#4a2830" :foreground "#c09080"))) (isearch-group-1 (t (:background "#2a4858" :foreground "#7aacc0"))) (isearch-fail (t (:background "#4c1c10" :foreground "#e08c68"))) (isearch (t (:background "#2d6652" :foreground "#f0e8d4"))) (mode-line-highlight (t (:foreground "#7ecfb4"))) (mode-line-emphasis (t (:foreground "#3d8a6e"))) (mode-line-buffer-id (t (:foreground "#c8a05a" :weight normal))) (mode-line-inactive (t (:background "#1a1710" :foreground "#665a48" :box (:line-width 1 :color "#2d2820")))) (mode-line (t (:background "#2d2820" :foreground "#8a7a64" :box (:line-width 1 :color "#4a4234")))) (line-number-minor-tick (t (:background "#1a1710" :foreground "#4a4234"))) (line-number-major-tick (t (:background "#1a1710" :foreground "#8a7a64"))) (line-number-current-line (t (:background "#1a1710" :foreground "#665a48"))) (line-number (t (:background "#1a1710" :foreground "#4a4234"))) (shadow (t (:foreground "#665a48"))) (nobreak-space (t (:foreground "#a84428" :underline t))) (homoglyph (t (:foreground "#e8cc90"))) (escape-glyph (t (:foreground "#e08c68"))) (success (t (:foreground "#7ecfb4"))) (error (t (:foreground "#e08c68"))) (warning (t (:foreground "#c8a05a"))) (button (t (:foreground "#7aacc0" :underline t))) (link-visited (t (:foreground "#c8a05a" :underline t))) (link (t (:foreground "#7ecfb4" :underline t))) (minibuffer-prompt (t (:foreground "#3d8a6e" :weight normal))) (window-divider-last-pixel (t (:foreground "#0e0c09"))) (window-divider-first-pixel (t (:foreground "#2d2820"))) (window-divider (t (:foreground "#4a4234"))) (vertical-border (t (:foreground "#4a4234"))) (fringe (t (:background "#1a1710" :foreground "#665a48"))) (secondary-selection (t (:background "#4a4234"))) (highlight (t (:background "#2d2820"))) (region (t (:background "#4a4234"))) (cursor (t (:background "#3d8a6e")))))"##
        ]],
    )
}

fn loading_dired_org_flymake_and_pulse_brings_in_the_faces_the_theme_already_styles()
-> ParityBatchCase {
    ParityBatchCase::value(
        "loading_dired_org_flymake_and_pulse_brings_in_the_faces_the_theme_already_styles",
        r##"
(progn
  (load-theme 'ancient t)
  (let ((faces '(dired-directory dired-broken-symlink dired-flagged
                 org-level-1 org-document-title org-block org-code
                 flymake-error flymake-warning flymake-note
                 pulse-highlight-start-face)))
    (let ((before (mapcar (lambda (face) (list face (and (facep face) t)))
                          faces)))
      ;; Every one of these ships with Emacs, so this is a user opening a
      ;; Dired buffer, an Org file and a Flymake session -- not a stub.
      (dolist (library '(dired org flymake pulse))
        (require library))
      (let ((after (mapcar #'anc-test-appearance faces)))
        (disable-theme 'ancient)
        (let ((without (mapcar #'anc-test-appearance faces)))
          (enable-theme 'ancient)
          (list :before-the-libraries-were-loaded before
                ;; These four carry the attribute kinds nothing above uses:
                ;; a struck-through face, three wave underlines that name
                ;; their own colour, and two scaled headings.
                :once-loaded-with-the-theme-on after
                :with-the-theme-off without
                :theme-back-on (mapcar #'anc-test-appearance faces)))))))
"##,
        expect![[
            r##"OK (:before-the-libraries-were-loaded ((dired-directory nil) (dired-broken-symlink nil) (dired-flagged nil) (org-level-1 nil) (org-document-title nil) (org-block nil) (org-code nil) (flymake-error nil) (flymake-warning nil) (flymake-note nil) (pulse-highlight-start-face nil)) :once-loaded-with-the-theme-on ((dired-directory :foreground "#3d8a6e") (dired-broken-symlink :foreground "#e08c68" :strike-through t) (dired-flagged :foreground "#e08c68" :strike-through t) (org-level-1 :foreground "#e8cc90" :weight normal :height 1.15) (org-document-title :foreground "#e8cc90" :weight normal :height 1.3) (org-block :foreground "#c8b89a" :background "#0e0c09") (org-code :foreground "#7ecfb4" :background "#2d2820") (flymake-error :underline (:style wave :color "#e08c68")) (flymake-warning :underline (:style wave :color "#c8a05a")) (flymake-note :underline (:style wave :color "#7aacc0")) (pulse-highlight-start-face :background "#2d6652")) :with-the-theme-off ((dired-directory :weight bold :inherit font-lock-function-name-face) (dired-broken-symlink :weight bold :slant italic :underline t) (dired-flagged :weight bold :inherit error) (org-level-1 :weight bold :inherit outline-1) (org-document-title :weight bold) (org-block :inherit shadow) (org-code :inherit shadow) (flymake-error :weight bold :inherit error) (flymake-warning :weight bold :inherit warning) (flymake-note :weight bold :inherit warning) (pulse-highlight-start-face)) :theme-back-on ((dired-directory :foreground "#3d8a6e") (dired-broken-symlink :foreground "#e08c68" :strike-through t) (dired-flagged :foreground "#e08c68" :strike-through t) (org-level-1 :foreground "#e8cc90" :weight normal :height 1.15) (org-document-title :foreground "#e8cc90" :weight normal :height 1.3) (org-block :foreground "#c8b89a" :background "#0e0c09") (org-code :foreground "#7ecfb4" :background "#2d2820") (flymake-error :underline (:style wave :color "#e08c68")) (flymake-warning :underline (:style wave :color "#c8a05a")) (flymake-note :underline (:style wave :color "#7aacc0")) (pulse-highlight-start-face :background "#2d6652")))"##
        ]],
    )
}

fn fontifying_a_real_emacs_lisp_buffer_shows_the_themes_own_colours() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifying_a_real_emacs_lisp_buffer_shows_the_themes_own_colours",
        r##"
(progn
  (load-theme 'ancient t)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert ";;; settle.el --- pay an invoice -*- lexical-binding: t; -*-\n"
            "(defconst settle-currency \"EUR\"\n"
            "  \"The currency invoices are settled in.\")\n"
            "(defun settle-invoice (invoice retries)\n"
            "  \"Return the payment state of INVOICE.\"\n"
            "  (let ((state :unpaid))\n"
            "    (when (and (bufferp invoice) (> retries 0))\n"
            "      (setq state :paid))\n"
            "    state))\n")
    (font-lock-ensure)
    (let (tokens)
      (dolist (token '("defconst" "settle-currency" "\"EUR\"" "defun"
                       "settle-invoice" "let" ":unpaid" "and" "0" "setq"
                       "pay an invoice"))
        (goto-char (point-min))
        (search-forward token)
        (let* ((start (- (point) (length token)))
               (face (get-text-property start 'face))
               (name (if (consp face) (car face) face)))
          (setq tokens
                (append tokens
                        (list (list token
                                    :face face
                                    :looks-like
                                    (and name (anc-test-appearance name))))))))
      (list :buffer (buffer-substring-no-properties (point-min) (point-max))
            :tokens tokens))))
"##,
        expect![[
            r##"OK (:buffer ";;; settle.el --- pay an invoice -*- lexical-binding: t; -*-\n(defconst settle-currency \"EUR\"\n  \"The currency invoices are settled in.\")\n(defun settle-invoice (invoice retries)\n  \"Return the payment state of INVOICE.\"\n  (let ((state :unpaid))\n    (when (and (bufferp invoice) (> retries 0))\n      (setq state :paid))\n    state))\n" :tokens (("defconst" :face font-lock-keyword-face :looks-like (font-lock-keyword-face :foreground "#3d8a6e")) ("settle-currency" :face font-lock-variable-name-face :looks-like (font-lock-variable-name-face :foreground "#e8dcc8")) ("\"EUR\"" :face font-lock-string-face :looks-like (font-lock-string-face :foreground "#c8a05a")) ("defun" :face font-lock-keyword-face :looks-like (font-lock-keyword-face :foreground "#3d8a6e")) ("settle-invoice" :face font-lock-function-name-face :looks-like (font-lock-function-name-face :foreground "#f0e8d4" :weight normal)) ("let" :face font-lock-keyword-face :looks-like (font-lock-keyword-face :foreground "#3d8a6e")) (":unpaid" :face font-lock-builtin-face :looks-like (font-lock-builtin-face :foreground "#7ecfb4")) ("and" :face font-lock-keyword-face :looks-like (font-lock-keyword-face :foreground "#3d8a6e")) ("0" :face nil :looks-like nil) ("setq" :face font-lock-keyword-face :looks-like (font-lock-keyword-face :foreground "#3d8a6e")) ("pay an invoice" :face font-lock-comment-face :looks-like (font-lock-comment-face :foreground "#665a48" :slant italic))))"##
        ]],
    )
}

fn layering_another_theme_on_top_only_changes_what_its_display_clause_allows() -> ParityBatchCase {
    ParityBatchCase::value(
        "layering_another_theme_on_top_only_changes_what_its_display_clause_allows",
        r##"
(progn
  (load-theme 'ancient t)
  (let ((alone (list :enabled (copy-sequence custom-enabled-themes)
                     :stack (anc-test-stack 'default)
                     :default (anc-test-appearance 'default)
                     :string (anc-test-appearance 'font-lock-string-face))))
    ;; wombat's specs are gated on 89 colours, which this display does not
    ;; have.  It goes on top of the stack and changes nothing.
    (load-theme 'wombat t)
    (let ((gated (list :enabled (copy-sequence custom-enabled-themes)
                       :stack (anc-test-stack 'default)
                       :wombat-clause (anc-test-clause-of 'wombat 'default)
                       :ancient-clause (anc-test-clause-of 'ancient 'default)
                       :default (anc-test-appearance 'default)
                       :string (anc-test-appearance 'font-lock-string-face))))
      (disable-theme 'wombat)
      ;; manoj-dark writes its specs `((t ...))' the way ancient does, so it
      ;; is the control: put it on top and it really does take over.
      (load-theme 'manoj-dark t)
      (let ((ungated (list :enabled (copy-sequence custom-enabled-themes)
                           :stack (anc-test-stack 'default)
                           :manoj-clause
                           (anc-test-clause-of 'manoj-dark 'default)
                           :default (anc-test-appearance 'default)
                           :string
                           (anc-test-appearance 'font-lock-string-face))))
        (disable-theme 'manoj-dark)
        (list :ancient-alone alone
              :with-a-gated-theme-on-top gated
              :with-an-ungated-theme-on-top ungated
              :after-removing-both
              (list :enabled (copy-sequence custom-enabled-themes)
                    :stack (anc-test-stack 'default)
                    :default (anc-test-appearance 'default)
                    :string
                    (anc-test-appearance 'font-lock-string-face)))))))
"##,
        expect![[
            r##"OK (:ancient-alone (:enabled (ancient) :stack (ancient) :default (default :foreground "#e8dcc8" :background "#1a1710" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :foreground "#c8a05a")) :with-a-gated-theme-on-top (:enabled (wombat ancient) :stack (wombat ancient) :wombat-clause ((class color) (min-colors 89)) :ancient-clause t :default (default :foreground "#e8dcc8" :background "#1a1710" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :foreground "#c8a05a")) :with-an-ungated-theme-on-top (:enabled (manoj-dark ancient) :stack (manoj-dark ancient) :manoj-clause t :default (default :foreground "WhiteSmoke" :background "black" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :foreground "RosyBrown1")) :after-removing-both (:enabled (ancient) :stack (ancient) :default (default :foreground "#e8dcc8" :background "#1a1710" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inherit nil :height 1) :string (font-lock-string-face :foreground "#c8a05a")))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_the_theme_enables_it_and_disabling_it_puts_the_editor_back(),
        every_one_of_the_two_hundred_and_thirty_six_specs_applies_on_a_monochrome_display(),
        the_theme_registers_two_hundred_and_thirty_six_specs_over_its_palette(),
        loading_dired_org_flymake_and_pulse_brings_in_the_faces_the_theme_already_styles(),
        fontifying_a_real_emacs_lisp_buffer_shows_the_themes_own_colours(),
        layering_another_theme_on_top_only_changes_what_its_display_clause_allows(),
    ]
}
