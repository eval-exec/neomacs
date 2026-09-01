use expect_test::expect;

use super::ParityBatchCase;

fn atom_dark_theme_basic_face_specs_match_every_selector_color_font_and_attribute()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_basic_face_specs_match_every_selector_color_font_and_attribute",
        r##"(atom-dark-test-theme-specs
         '(button
           cursor
           default
           escape-glyph
           fixed-pitch
           header-line
           highlight
           lazy-highlight
           link
           link-visited
           match
           minibuffer-prompt
           next-error
           query-replace
           region
           secondary-selection
           shadow
           tooltip
           trailing-whitespace
           variable-pitch))"##,
        expect![[
            r##"OK ((button ((t (:inherit (link))))) (cursor ((((background light)) (:background "black")) (((background dark)) (:background "white")))) (default ((t (:foreground "#c5c8c6" :background "#1d1f21" :weight normal :slant normal :underline nil :overline nil :strike-through nil :box nil :inverse-video nil :stipple nil :inherit nil)))) (escape-glyph ((t (:foreground "#FF8000")))) (fixed-pitch ((t (:family "Monospace")))) (header-line ((t (:foreground "grey90" :background "grey20")))) (highlight ((t (:background "#444")))) (lazy-highlight ((((class color) (min-colors 88) (background light)) (:background "paleturquoise")) (((class color) (min-colors 88) (background dark)) (:background "paleturquoise4")) (((class color) (min-colors 16)) (:background "turquoise3")) (((class color) (min-colors 8)) (:background "turquoise3")) (t (:underline (:color foreground-color :style line))))) (link ((t (:inherit font-lock-keyword-face :underline t)))) (link-visited ((default (:inherit (link))) (((class color) (background light)) (:foreground "magenta4")) (((class color) (background dark)) (:foreground "violet")))) (match ((((class color) (min-colors 88) (background light)) (:background "yellow1")) (((class color) (min-colors 88) (background dark)) (:background "RoyalBlue3")) (((class color) (min-colors 8) (background light)) (:foreground "black" :background "yellow")) (((class color) (min-colors 8) (background dark)) (:foreground "white" :background "blue")) (((type tty) (class mono)) (:inverse-video t)) (t (:background "gray")))) (minibuffer-prompt ((t (:foreground "#FF8000")))) (next-error ((t (:inherit (region))))) (query-replace ((t (:inherit (isearch))))) (region ((t (:background "grey70")))) (secondary-selection ((t (:background "#262626")))) (shadow ((t (:foreground "#7c7c7c")))) (tooltip ((t (:inherit variable-pitch :background "#fff" :foreground "#333")))) (trailing-whitespace ((t (:background "#562d56" :foreground "#FD5FF1")))) (variable-pitch ((t (:family "Sans Serif")))))"##
        ]],
    )
}

fn atom_dark_theme_font_lock_specs_match_every_literal_and_inheritance_edge() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_font_lock_specs_match_every_literal_and_inheritance_edge",
        r##"(atom-dark-test-theme-specs
         '(font-lock-builtin-face
           font-lock-comment-delimiter-face
           font-lock-comment-face
           font-lock-constant-face
           font-lock-doc-face
           font-lock-function-name-face
           font-lock-keyword-face
           font-lock-preprocessor-face
           font-lock-regexp-grouping-backslash
           font-lock-regexp-grouping-construct
           font-lock-string-face
           font-lock-type-face
           font-lock-variable-name-face
           font-lock-warning-face))"##,
        expect![[
            r##"OK ((font-lock-builtin-face ((t (:foreground "#DAD085")))) (font-lock-comment-delimiter-face ((default (:inherit (font-lock-comment-face))))) (font-lock-comment-face ((t (:foreground "#7C7C7C")))) (font-lock-constant-face ((t (:foreground "#99CC99")))) (font-lock-doc-face ((t (:inherit (font-lock-string-face))))) (font-lock-function-name-face ((t (:foreground "#FFD2A7")))) (font-lock-keyword-face ((t (:foreground "#96CBFE")))) (font-lock-preprocessor-face ((t (:foreground "#8996A8")))) (font-lock-regexp-grouping-backslash ((t (:inherit font-lock-string-face)))) (font-lock-regexp-grouping-construct ((t (:foreground "#C6A24F")))) (font-lock-string-face ((t (:foreground "#8AE234")))) (font-lock-type-face ((t (:foreground "#CFCB90")))) (font-lock-variable-name-face ((t (:inherit (default))))) (font-lock-warning-face ((t (:foreground "#ff982d" :weight bold)))))"##
        ]],
    )
}

fn atom_dark_theme_mode_line_search_and_ido_specs_match_all_display_fallbacks() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_mode_line_search_and_ido_specs_match_all_display_fallbacks",
        r##"(atom-dark-test-theme-specs
         '(mode-line
           mode-line-buffer-id
           mode-line-emphasis
           mode-line-highlight
           mode-line-inactive
           isearch
           isearch-fail
           ido-first-match
           ido-only-match
           ido-subdir
           ido-virtual))"##,
        expect![[
            r##"OK ((mode-line ((t (:background "grey10" :foreground "#96CBFE")))) (mode-line-buffer-id ((t (:weight bold)))) (mode-line-emphasis ((t (:weight bold)))) (mode-line-highlight ((((class color) (min-colors 88)) (:box (:line-width 2 :color "#1d1f21" :style released-button))) (t (:inherit (highlight))))) (mode-line-inactive ((default (:inherit (mode-line))) (((class color) (min-colors 88) (background light)) (:background "#7c7c7c" :foreground "grey20" :box (:line-width -1 :color "grey75" :style nil) :weight light)) (((class color) (min-colors 88) (background dark)) (:background "grey30" :foreground "grey80" :box (:line-width -1 :color "grey40" :style nil) :weight light)))) (isearch ((((class color) (min-colors 88) (background light)) (:foreground "lightskyblue1" :background "magenta3")) (((class color) (min-colors 88) (background dark)) (:foreground "brown4" :background "palevioletred2")) (((class color) (min-colors 16)) (:foreground "cyan1" :background "magenta4")) (((class color) (min-colors 8)) (:foreground "cyan1" :background "magenta4")) (t (:inverse-video t)))) (isearch-fail ((((class color) (min-colors 88) (background light)) (:background "RosyBrown1")) (((class color) (min-colors 88) (background dark)) (:background "red4")) (((class color) (min-colors 16)) (:background "red")) (((class color) (min-colors 8)) (:background "red")) (((class color grayscale)) (:foreground "grey")) (t (:inverse-video t)))) (ido-first-match ((t (:foreground "violet" :weight bold)))) (ido-only-match ((t (:foreground "#ff982d" :weight bold)))) (ido-subdir ((t (:foreground "#8AE234")))) (ido-virtual ((t (:foreground "#7c7c7c")))))"##
        ]],
    )
}

fn atom_dark_theme_diff_dired_guide_key_and_flx_specs_match_optional_face_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_diff_dired_guide_key_and_flx_specs_match_optional_face_contracts",
        r##"(atom-dark-test-theme-specs
         '(diff-hl-change
           diff-hl-delete
           diff-hl-insert
           dired-directory
           dired-flagged
           dired-symlink
           guide-key/highlight-command-face
           guide-key/key-face
           guide-key/prefix-command-face
           flx-highlight-face))"##,
        expect![[
            r##"OK ((diff-hl-change ((t (:foreground "#E9C062" :background "#8b733a")))) (diff-hl-delete ((t (:foreground "#CC6666" :background "#7a3d3d")))) (diff-hl-insert ((t (:foreground "#A8FF60" :background "#547f30")))) (dired-directory ((t (:inherit (font-lock-keyword-face))))) (dired-flagged ((t (:inherit (diff-hl-delete))))) (dired-symlink ((t (:foreground "#FD5FF1")))) (guide-key/highlight-command-face ((t (:inherit (cursor))))) (guide-key/key-face ((t (:inherit (font-lock-warning-face))))) (guide-key/prefix-command-face ((t (:inherit (font-lock-keyword-face))))) (flx-highlight-face ((t (:inherit (link) :weight bold)))))"##
        ]],
    )
}

fn atom_dark_theme_markdown_and_js2_specs_match_practical_language_face_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_markdown_and_js2_specs_match_practical_language_face_contracts",
        r##"(atom-dark-test-theme-specs
         '(markdown-blockquote-face
           markdown-header-face
           markdown-header-delimiter-face
           markdown-header-rule-face
           js2-error
           js2-external-variable
           js2-function-param
           js2-jsdoc-html-tag-delimiter
           js2-jsdoc-html-tag-name
           js2-jsdoc-tag
           js2-jsdoc-type
           js2-jsdoc-value))"##,
        expect![[
            r##"OK ((markdown-blockquote-face ((t :foreground "#555"))) (markdown-header-face ((t :foreground "#eee"))) (markdown-header-delimiter-face ((t (:inherit (markdown-header-face))))) (markdown-header-rule-face ((t (:inherit (font-lock-comment-face))))) (js2-error ((t (:foreground "#c00")))) (js2-external-variable ((t (:inherit (font-lock-builtin-face))))) (js2-function-param ((t (:foreground "#C6C5FE")))) (js2-jsdoc-html-tag-delimiter ((t (:foreground "#96CBFE")))) (js2-jsdoc-html-tag-name ((t (:foreground "#96CBFE")))) (js2-jsdoc-tag ((t (:inherit (font-lock-doc-face) :weight bold)))) (js2-jsdoc-type ((t (:inherit (font-lock-type-face))))) (js2-jsdoc-value ((t (:inherit (js2-function-param))))))"##
        ]],
    )
}

fn atom_dark_theme_minimap_powerline_realgud_and_speedbar_specs_match_ui_integrations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_minimap_powerline_realgud_and_speedbar_specs_match_ui_integrations",
        r##"(atom-dark-test-theme-specs
         '(minimap-active-region-background
           powerline-active2
           realgud-overlay-arrow1
           realgud-overlay-arrow2
           realgud-overlay-arrow3
           speedbar-button-face
           speedbar-directory-face
           speedbar-file-face
           speedbar-highlight-face
           speedbar-selected-face
           speedbar-separator-face
           speedbar-tag-face))"##,
        expect![[
            r##"OK ((minimap-active-region-background ((t (:inherit (highlight))))) (powerline-active2 ((t (:background "grey10")))) (realgud-overlay-arrow1 ((t (:foreground "#7fff00")))) (realgud-overlay-arrow2 ((t (:foreground "#5FAF44")))) (realgud-overlay-arrow3 ((t (:foreground "#116600")))) (speedbar-button-face ((t (:foreground "#AAAAAA")))) (speedbar-directory-face ((t (:inherit (font-lock-keyword-face))))) (speedbar-file-face ((t (:inherit (default))))) (speedbar-highlight-face ((t (:inherit (highlight))))) (speedbar-selected-face ((t (:background "#4182C4" :foreground "#FFFFFF")))) (speedbar-separator-face ((t (:background "grey11" :foreground "#C5C8C6" :overline "#7C7C7C")))) (speedbar-tag-face ((t (:inherit (font-lock-function-name-face))))))"##
        ]],
    )
}

fn atom_dark_theme_whitespace_specs_match_empty_trailing_and_all_derived_faces() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_dark_theme_whitespace_specs_match_empty_trailing_and_all_derived_faces",
        r##"(atom-dark-test-theme-specs
         '(whitespace-empty
           whitespace-hspace
           whitespace-indentation
           whitespace-line
           whitespace-newline
           whitespace-space
           whitespace-space-after-tab
           whitespace-space-before-tab
           whitespace-tab
           whitespace-trailing))"##,
        expect![[
            r##"OK ((whitespace-empty ((t (:foreground "#333333")))) (whitespace-hspace ((t (:inherit (whitespace-empty))))) (whitespace-indentation ((t (:inherit (whitespace-empty))))) (whitespace-line ((t (:inherit (trailing-whitespace))))) (whitespace-newline ((t (:inherit (whitespace-empty))))) (whitespace-space ((t (:inherit (whitespace-empty))))) (whitespace-space-after-tab ((t (:inherit (whitespace-empty))))) (whitespace-space-before-tab ((t (:inherit (whitespace-empty))))) (whitespace-tab ((t (:inherit (whitespace-empty))))) (whitespace-trailing ((t (:inherit (trailing-whitespace))))))"##
        ]],
    )
}

fn atom_dark_theme_company_specs_match_preview_scrollbar_tooltip_and_selection_edges()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_company_specs_match_preview_scrollbar_tooltip_and_selection_edges",
        r##"(atom-dark-test-theme-specs
         '(company-preview
           company-preview-common
           company-preview-search
           company-scrollbar-bg
           company-scrollbar-fg
           company-tooltip
           company-tooltip-common
           company-tooltip-common-selection
           company-tooltip-selection))"##,
        expect![[
            r##"OK ((company-preview ((t (:foreground "#96CBFE")))) (company-preview-common ((t (:inherit company-preview :underline "#96CBFE")))) (company-preview-search ((t (:inherit company-preview)))) (company-scrollbar-bg ((t (:inherit company-tooltip :background "dim grey")))) (company-scrollbar-fg ((t (:background "black")))) (company-tooltip ((t (:background "#c5c8c6" :foreground "#1d1f21")))) (company-tooltip-common ((t (:inherit company-tooltip :foreground "red4")))) (company-tooltip-common-selection ((t (:inherit company-tooltip-selection :background "#96CBFE")))) (company-tooltip-selection ((t (:inherit company-tooltip :background "#96CBFE")))))"##
        ]],
    )
}

fn atom_dark_theme_all_ninety_eight_specs_have_nonempty_selectors_and_stable_attribute_shapes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_all_ninety_eight_specs_have_nonempty_selectors_and_stable_attribute_shapes",
        r##"(let (observations)
         (dolist
             (setting
              (get 'atom-dark 'theme-settings))
           (let* ((face
                   (cadr setting))
                  (spec
                   (nth 3 setting))
                  (selectors
                   (mapcar #'car spec))
                  (attributes
                   (mapcar
                    (lambda (entry)
                      (let ((tail
                             (cdr entry)))
                        (if
                            (and
                             (= 1
                                (length tail))
                             (listp
                              (car tail)))
                            (length
                             (car tail))
                          (length tail))))
                    spec)))
             (push
              (list
               face
               (length spec)
               selectors
               attributes)
              observations)))
         (nreverse observations))"##,
        expect![
            "OK ((company-tooltip-selection 1 (t) (4)) (company-tooltip-common-selection 1 (t) (4)) (company-tooltip-common 1 (t) (4)) (company-tooltip 1 (t) (4)) (company-scrollbar-fg 1 (t) (2)) (company-scrollbar-bg 1 (t) (4)) (company-preview-search 1 (t) (2)) (company-preview-common 1 (t) (4)) (company-preview 1 (t) (2)) (whitespace-trailing 1 (t) (2)) (whitespace-tab 1 (t) (2)) (whitespace-space-before-tab 1 (t) (2)) (whitespace-space-after-tab 1 (t) (2)) (whitespace-space 1 (t) (2)) (whitespace-newline 1 (t) (2)) (whitespace-line 1 (t) (2)) (whitespace-indentation 1 (t) (2)) (whitespace-hspace 1 (t) (2)) (whitespace-empty 1 (t) (2)) (speedbar-tag-face 1 (t) (2)) (speedbar-separator-face 1 (t) (6)) (speedbar-selected-face 1 (t) (4)) (speedbar-highlight-face 1 (t) (2)) (speedbar-file-face 1 (t) (2)) (speedbar-directory-face 1 (t) (2)) (speedbar-button-face 1 (t) (2)) (realgud-overlay-arrow3 1 (t) (2)) (realgud-overlay-arrow2 1 (t) (2)) (realgud-overlay-arrow1 1 (t) (2)) (powerline-active2 1 (t) (2)) (minimap-active-region-background 1 (t) (2)) (js2-jsdoc-value 1 (t) (2)) (js2-jsdoc-type 1 (t) (2)) (js2-jsdoc-tag 1 (t) (4)) (js2-jsdoc-html-tag-name 1 (t) (2)) (js2-jsdoc-html-tag-delimiter 1 (t) (2)) (js2-function-param 1 (t) (2)) (js2-external-variable 1 (t) (2)) (js2-error 1 (t) (2)) (markdown-header-rule-face 1 (t) (2)) (markdown-header-delimiter-face 1 (t) (2)) (markdown-header-face 1 (t) (2)) (markdown-blockquote-face 1 (t) (2)) (flx-highlight-face 1 (t) (4)) (guide-key/prefix-command-face 1 (t) (2)) (guide-key/key-face 1 (t) (2)) (guide-key/highlight-command-face 1 (t) (2)) (dired-symlink 1 (t) (2)) (dired-flagged 1 (t) (2)) (dired-directory 1 (t) (2)) (diff-hl-insert 1 (t) (4)) (diff-hl-delete 1 (t) (4)) (diff-hl-change 1 (t) (4)) (ido-virtual 1 (t) (2)) (ido-subdir 1 (t) (2)) (ido-only-match 1 (t) (4)) (ido-first-match 1 (t) (4)) (isearch-fail 6 (((class color) (min-colors 88) (background light)) ((class color) (min-colors 88) (background dark)) ((class color) (min-colors 16)) ((class color) (min-colors 8)) ((class color grayscale)) t) (2 2 2 2 2 2)) (isearch 5 (((class color) (min-colors 88) (background light)) ((class color) (min-colors 88) (background dark)) ((class color) (min-colors 16)) ((class color) (min-colors 8)) t) (4 4 4 4 2)) (mode-line-inactive 3 (default ((class color) (min-colors 88) (background light)) ((class color) (min-colors 88) (background dark))) (2 8 8)) (mode-line-highlight 2 (((class color) (min-colors 88)) t) (2 2)) (mode-line-emphasis 1 (t) (2)) (mode-line-buffer-id 1 (t) (2)) (mode-line 1 (t) (4)) (font-lock-warning-face 1 (t) (4)) (font-lock-variable-name-face 1 (t) (2)) (font-lock-type-face 1 (t) (2)) (font-lock-string-face 1 (t) (2)) (font-lock-regexp-grouping-construct 1 (t) (2)) (font-lock-regexp-grouping-backslash 1 (t) (2)) (font-lock-preprocessor-face 1 (t) (2)) (font-lock-keyword-face 1 (t) (2)) (font-lock-function-name-face 1 (t) (2)) (font-lock-doc-face 1 (t) (2)) (font-lock-constant-face 1 (t) (2)) (font-lock-comment-face 1 (t) (2)) (font-lock-comment-delimiter-face 1 (default) (2)) (font-lock-builtin-face 1 (t) (2)) (variable-pitch 1 (t) (2)) (trailing-whitespace 1 (t) (4)) (tooltip 1 (t) (6)) (shadow 1 (t) (2)) (secondary-selection 1 (t) (2)) (region 1 (t) (2)) (query-replace 1 (t) (2)) (next-error 1 (t) (2)) (minibuffer-prompt 1 (t) (2)) (match 6 (((class color) (min-colors 88) (background light)) ((class color) (min-colors 88) (background dark)) ((class color) (min-colors 8) (background light)) ((class color) (min-colors 8) (background dark)) ((type tty) (class mono)) t) (2 2 4 4 2 2)) (link-visited 3 (default ((class color) (background light)) ((class color) (background dark))) (2 2 2)) (link 1 (t) (4)) (lazy-highlight 5 (((class color) (min-colors 88) (background light)) ((class color) (min-colors 88) (background dark)) ((class color) (min-colors 16)) ((class color) (min-colors 8)) t) (2 2 2 2 2)) (highlight 1 (t) (2)) (header-line 1 (t) (4)) (fixed-pitch 1 (t) (2)) (escape-glyph 1 (t) (2)) (default 1 (t) (22)) (cursor 2 (((background light)) ((background dark))) (2 2)) (button 1 (t) (2)))"
        ],
    )
}

fn atom_dark_theme_palette_literals_and_inheritance_targets_are_complete_and_stable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_palette_literals_and_inheritance_targets_are_complete_and_stable",
        r##"(let (colors inherits)
         (dolist
             (setting
              (get 'atom-dark 'theme-settings))
           (dolist
               (entry
                (nth 3 setting))
             (let ((attributes
                    (if
                        (and
                         (= 1
                            (length
                             (cdr entry)))
                         (listp
                          (cadr entry)))
                        (cadr entry)
                      (cdr entry))))
               (while attributes
                 (let ((key
                        (pop attributes))
                       (value
                        (pop attributes)))
                   (cond
                    ((memq key
                           '(:foreground
                             :background
                             :color))
                     (when
                         (stringp value)
                       (push value colors)))
                    ((eq key :inherit)
                     (push value inherits))))))))
         (list
          (sort
           (delete-dups colors)
           #'string<)
          (sort
           (delete-dups
            (mapcar #'prin1-to-string
                    inherits))
           #'string<)))"##,
        expect![[
            r##"OK (("#116600" "#1d1f21" "#262626" "#333" "#333333" "#4182C4" "#444" "#547f30" "#555" "#562d56" "#5FAF44" "#7C7C7C" "#7a3d3d" "#7c7c7c" "#7fff00" "#8996A8" "#8AE234" "#8b733a" "#96CBFE" "#99CC99" "#A8FF60" "#AAAAAA" "#C5C8C6" "#C6A24F" "#C6C5FE" "#CC6666" "#CFCB90" "#DAD085" "#E9C062" "#FD5FF1" "#FF8000" "#FFD2A7" "#FFFFFF" "#c00" "#c5c8c6" "#eee" "#ff982d" "#fff" "RosyBrown1" "RoyalBlue3" "black" "blue" "brown4" "cyan1" "dim grey" "gray" "grey" "grey10" "grey11" "grey20" "grey30" "grey70" "grey80" "grey90" "lightskyblue1" "magenta3" "magenta4" "paleturquoise" "paleturquoise4" "palevioletred2" "red" "red4" "turquoise3" "violet" "white" "yellow" "yellow1") ("(cursor)" "(default)" "(diff-hl-delete)" "(font-lock-builtin-face)" "(font-lock-comment-face)" "(font-lock-doc-face)" "(font-lock-function-name-face)" "(font-lock-keyword-face)" "(font-lock-string-face)" "(font-lock-type-face)" "(font-lock-warning-face)" "(highlight)" "(isearch)" "(js2-function-param)" "(link)" "(markdown-header-face)" "(mode-line)" "(region)" "(trailing-whitespace)" "(whitespace-empty)" "company-preview" "company-tooltip" "company-tooltip-selection" "font-lock-keyword-face" "font-lock-string-face" "nil" "variable-pitch"))"##
        ]],
    )
}

pub(super) fn faces_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_dark_theme_basic_face_specs_match_every_selector_color_font_and_attribute(),
        atom_dark_theme_font_lock_specs_match_every_literal_and_inheritance_edge(),
        atom_dark_theme_mode_line_search_and_ido_specs_match_all_display_fallbacks(),
        atom_dark_theme_diff_dired_guide_key_and_flx_specs_match_optional_face_contracts(),
        atom_dark_theme_markdown_and_js2_specs_match_practical_language_face_contracts(),
        atom_dark_theme_minimap_powerline_realgud_and_speedbar_specs_match_ui_integrations(),
        atom_dark_theme_whitespace_specs_match_empty_trailing_and_all_derived_faces(),
        atom_dark_theme_company_specs_match_preview_scrollbar_tooltip_and_selection_edges(),
        atom_dark_theme_all_ninety_eight_specs_have_nonempty_selectors_and_stable_attribute_shapes(
        ),
        atom_dark_theme_palette_literals_and_inheritance_targets_are_complete_and_stable(),
    ]
}
