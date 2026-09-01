use expect_test::expect;

use super::ParityBatchCase;

/// Enabling the theme: `load-theme' finds it on the package's own
/// `custom-theme-load-path' entry, registers 520 face settings and nothing
/// else, and each face an editing session shows carries the exact palette
/// entry the theme documents -- a wrong colour, a wrong display clause or a
/// dropped face all fail here.  The display facts are pinned too: a batch
/// frame is a 0-colour `mono' display, so the theme's `((class color)
/// (min-colors 256))' clause matches nothing and the registered palette is not
/// realised on this display in either editor.
fn loading_the_theme_registers_its_documented_palette_for_every_probed_face() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_the_theme_registers_its_documented_palette_for_every_probed_face",
        r##"(let ((before (list :themes (copy-sequence custom-enabled-themes)
                    :registered (adwaita-test-registered
                                 '(default mode-line font-lock-keyword-face))
                    :resolved (adwaita-test-resolved '(default mode-line)))))
  (unwind-protect
      (progn
        (adwaita-test-reset)
        (list :before before
              :load (list (load-theme 'adwaita-dark t)
                          (copy-sequence custom-enabled-themes)
                          (and (custom-theme-p 'adwaita-dark) t)
                          (and (custom-theme-enabled-p 'adwaita-dark) t)
                          (length (get 'adwaita-dark 'theme-settings))
                          (let (kinds)
                            (dolist (setting (get 'adwaita-dark 'theme-settings))
                              (cl-pushnew (car setting) kinds))
                            kinds))
              :display (adwaita-test-display-facts)
              :registered (adwaita-test-registered adwaita-test-probed-faces)
              :resolved (adwaita-test-resolved
                         '(default mode-line font-lock-keyword-face))))
    (adwaita-test-reset)))"##,
        expect![[
            r#"OK (:before (:themes nil :registered ((default :not-registered) (mode-line :not-registered) (font-lock-keyword-face :not-registered)) :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified))) :load (t (adwaita-dark) t t 520 (theme-face)) :display (:graphic nil :daemon nil :display-type mono :color-cells 0 :matches-theme-clause nil :matches-any-display t) :registered ((default #1=((class color) (min-colors 256)) (:background "gray11" :foreground "gray86")) (cursor #1# (:background "gray86")) (region #1# (:background "gray27" :distant-foreground "gray86")) (highlight #1# (:background "steelblue2" :foreground "gray13" :distant-foreground "gray87")) (fringe #1# (:inherit default :foreground "gray27")) (mode-line #1# (:background "gray19" :foreground "gray86" :box nil)) (mode-line-inactive #1# (:background "gray14" :foreground "gray40" :box nil)) (mode-line-buffer-id #1# (:foreground "gray87" :weight bold)) (font-lock-keyword-face #1# (:foreground "orange2" :weight bold)) (font-lock-string-face #1# (:foreground "mediumaquamarine")) (font-lock-comment-face #1# (:foreground "gray40")) (font-lock-function-name-face #1# nil) (font-lock-variable-name-face #1# nil) (font-lock-constant-face #1# (:foreground "mediumpurple3")) (font-lock-type-face #1# (:foreground "mediumaquamarine" :weight bold)) (font-lock-builtin-face #1# (:foreground "mediumpurple3")) (font-lock-warning-face #1# (:inherit warning)) (error #1# (:foreground "indianred2")) (warning #1# (:foreground "gold2")) (success #1# (:foreground "seagreen3")) (isearch #1# (:inherit highlight)) (lazy-highlight #1# (:inherit highlight)) (link #1# (:foreground "steelblue2" :underline t :weight bold)) (line-number #1# (:inherit default :foreground "gray40")) (line-number-current-line #1# (:inherit (hl-line default) :foreground "gray65")) (minibuffer-prompt #1# (:foreground "gray65")) (vertical-border #1# (:background "gray14" :foreground "gray14")) (show-paren-match #1# (:foreground "gray87" :weight ultra-bold))) :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified)))"#
        ]],
    )
}

fn each_documented_toggle_changes_the_registered_appearance() -> ParityBatchCase {
    ParityBatchCase::value(
        "each_documented_toggle_changes_the_registered_appearance",
        r##"(unwind-protect
    (list
     :mode-line-default
     (progn (adwaita-test-reset)
            (load-theme 'adwaita-dark t)
            (adwaita-test-registered '(mode-line mode-line-inactive)))
     :mode-line-padded
     (adwaita-test-registered-with 'adwaita-dark-theme-pad-mode-line t
                                   '(mode-line mode-line-inactive))
     :outlines-default
     (progn (adwaita-test-reset)
            (load-theme 'adwaita-dark t)
            (adwaita-test-registered '(outline-1 outline-2 outline-3)))
     :outlines-gray
     (adwaita-test-registered-with 'adwaita-dark-theme-gray-outlines t
                                   '(outline-1 outline-2 outline-3))
     :delimiters-default
     (progn (adwaita-test-reset)
            (load-theme 'adwaita-dark t)
            (adwaita-test-registered '(rainbow-delimiters-depth-1-face
                                       rainbow-delimiters-depth-2-face)))
     :delimiters-gray
     (adwaita-test-registered-with 'adwaita-dark-theme-gray-rainbow-delimiters t
                                   '(rainbow-delimiters-depth-1-face
                                     rainbow-delimiters-depth-2-face))
     :vertico-default
     (progn (adwaita-test-reset)
            (load-theme 'adwaita-dark t)
            (adwaita-test-registered '(vertico-current)))
     :vertico-bold
     (adwaita-test-registered-with 'adwaita-dark-theme-bold-vertico-current t
                                   '(vertico-current))
     :first-difference-default
     (progn (adwaita-test-reset)
            (load-theme 'adwaita-dark t)
            (adwaita-test-registered '(completions-first-difference)))
     :first-difference-off
     (adwaita-test-registered-with
      'adwaita-dark-theme-no-completions-first-difference t
      '(completions-first-difference)))
  (adwaita-test-reset))"##,
        expect![[
            r#"OK (:mode-line-default ((mode-line #1=((class color) (min-colors 256)) (:background "gray19" :foreground "gray86" :box nil)) (mode-line-inactive #1# (:background "gray14" :foreground "gray40" :box nil))) :mode-line-padded ((mode-line #2=((class color) (min-colors 256)) (:background "gray19" :foreground "gray86" :box (:line-width 10 :color "gray19"))) (mode-line-inactive #2# (:background "gray14" :foreground "gray40" :box (:line-width 10 :color "gray14")))) :outlines-default ((outline-1 #3=((class color) (min-colors 256)) (:foreground "steelblue2" :weight bold)) (outline-2 #3# (:foreground "orchid3" :weight bold)) (outline-3 #3# (:foreground "seagreen3" :weight bold))) :outlines-gray ((outline-1 #4=((class color) (min-colors 256)) (:foreground "gray48" :weight bold)) (outline-2 #4# (:foreground "gray65" :weight bold)) (outline-3 #4# (:foreground "gray48" :weight bold))) :delimiters-default ((rainbow-delimiters-depth-1-face #5=((class color) (min-colors 256)) (:foreground "steelblue2")) (rainbow-delimiters-depth-2-face #5# (:foreground "orchid3"))) :delimiters-gray ((rainbow-delimiters-depth-1-face #6=((class color) (min-colors 256)) (:foreground "gray65")) (rainbow-delimiters-depth-2-face #6# (:foreground "gray65"))) :vertico-default ((vertico-current ((class color) (min-colors 256)) (:background "gray19" :bold nil))) :vertico-bold ((vertico-current ((class color) (min-colors 256)) (:background "gray19" :bold bold))) :first-difference-default ((completions-first-difference ((class color) (min-colors 256)) (:weight bold))) :first-difference-off ((completions-first-difference ((class color) (min-colors 256)) nil)))"#
        ]],
    )
}

fn disabling_the_theme_restores_the_captured_baseline_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_the_theme_restores_the_captured_baseline_exactly",
        r##"(unwind-protect
    (let ((baseline (list :themes (copy-sequence custom-enabled-themes)
                          :resolved (adwaita-test-resolved
                                     adwaita-test-probed-faces)
                          :registered (adwaita-test-registered
                                       '(default mode-line region)))))
      (load-theme 'adwaita-dark t)
      (let ((enabled (list :themes (copy-sequence custom-enabled-themes)
                           :enabled-p (and (custom-theme-enabled-p 'adwaita-dark) t)
                           :resolved (adwaita-test-resolved
                                      adwaita-test-probed-faces))))
        (disable-theme 'adwaita-dark)
        (let ((disabled (list :themes (copy-sequence custom-enabled-themes)
                              :enabled-p (and (custom-theme-enabled-p 'adwaita-dark) t)
                              :loaded-p (and (custom-theme-p 'adwaita-dark) t)
                              :settings (length (get 'adwaita-dark 'theme-settings))
                              :resolved (adwaita-test-resolved
                                         adwaita-test-probed-faces)
                              :registered (adwaita-test-registered
                                           '(default mode-line region)))))
          (list :baseline-restored (equal (plist-get baseline :resolved)
                                          (plist-get disabled :resolved))
                :registration-removed (equal (plist-get baseline :registered)
                                             (plist-get disabled :registered))
                :baseline baseline
                :enabled enabled
                :disabled disabled
                :re-enabled (progn (enable-theme 'adwaita-dark)
                                   (list (copy-sequence custom-enabled-themes)
                                         (adwaita-test-registered '(default))))))))
  (adwaita-test-reset))"##,
        expect![[
            r#"OK (:baseline-restored t :registration-removed t :baseline (:themes nil :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (cursor :foreground unspecified :background "white" :weight unspecified :box unspecified) (region :foreground unspecified :background unspecified :weight unspecified :box unspecified) (highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (fringe :foreground unspecified :background "gray" :weight unspecified :box unspecified) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-inactive :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-buffer-id :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-string-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-comment-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-function-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-variable-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-constant-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-type-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-builtin-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-warning-face :foreground unspecified :background unspecified :weight bold :box unspecified) (error :foreground unspecified :background unspecified :weight bold :box unspecified) (warning :foreground unspecified :background unspecified :weight bold :box unspecified) (success :foreground unspecified :background unspecified :weight bold :box unspecified) (isearch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (lazy-highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link :foreground unspecified :background unspecified :weight unspecified :box unspecified) (line-number :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (line-number-current-line :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (minibuffer-prompt :foreground "cyan" :background unspecified :weight unspecified :box unspecified) (vertical-border :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-match :foreground unspecified :background unspecified :weight unspecified :box unspecified)) :registered ((default :not-registered) (mode-line :not-registered) (region :not-registered))) :enabled (:themes (adwaita-dark) :enabled-p t :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (cursor :foreground unspecified :background "white" :weight unspecified :box unspecified) (region :foreground unspecified :background unspecified :weight unspecified :box unspecified) (highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (fringe :foreground unspecified :background "gray" :weight unspecified :box unspecified) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-inactive :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-buffer-id :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-string-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-comment-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-function-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-variable-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-constant-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-type-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-builtin-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-warning-face :foreground unspecified :background unspecified :weight bold :box unspecified) (error :foreground unspecified :background unspecified :weight bold :box unspecified) (warning :foreground unspecified :background unspecified :weight bold :box unspecified) (success :foreground unspecified :background unspecified :weight bold :box unspecified) (isearch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (lazy-highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link :foreground unspecified :background unspecified :weight unspecified :box unspecified) (line-number :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (line-number-current-line :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (minibuffer-prompt :foreground "cyan" :background unspecified :weight unspecified :box unspecified) (vertical-border :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-match :foreground unspecified :background unspecified :weight unspecified :box unspecified))) :disabled (:themes nil :enabled-p nil :loaded-p t :settings 520 :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (cursor :foreground unspecified :background "white" :weight unspecified :box unspecified) (region :foreground unspecified :background unspecified :weight unspecified :box unspecified) (highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (fringe :foreground unspecified :background "gray" :weight unspecified :box unspecified) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-inactive :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-buffer-id :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-string-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-comment-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-function-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-variable-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-constant-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-type-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-builtin-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-warning-face :foreground unspecified :background unspecified :weight bold :box unspecified) (error :foreground unspecified :background unspecified :weight bold :box unspecified) (warning :foreground unspecified :background unspecified :weight bold :box unspecified) (success :foreground unspecified :background unspecified :weight bold :box unspecified) (isearch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (lazy-highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link :foreground unspecified :background unspecified :weight unspecified :box unspecified) (line-number :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (line-number-current-line :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (minibuffer-prompt :foreground "cyan" :background unspecified :weight unspecified :box unspecified) (vertical-border :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-match :foreground unspecified :background unspecified :weight unspecified :box unspecified)) :registered ((default :not-registered) (mode-line :not-registered) (region :not-registered))) :re-enabled ((adwaita-dark) ((default ((class color) (min-colors 256)) (:background "gray11" :foreground "gray86")))))"#
        ]],
    )
}

fn a_second_theme_loaded_on_top_takes_precedence_and_disabling_it_hands_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_second_theme_loaded_on_top_takes_precedence_and_disabling_it_hands_back",
        r##"(unwind-protect
    (progn
      (adwaita-test-reset)
      (load-theme 'adwaita-dark t)
      (let ((adwaita-only (list :themes (copy-sequence custom-enabled-themes)
                                :registered (adwaita-test-registered
                                             '(default region font-lock-string-face)))))
        (eval '(deftheme adwaita-test-overlay "A personal overlay theme.") t)
        (custom-theme-set-faces 'adwaita-test-overlay
                                '(default ((((class color) (min-colors 256))
                                            (:background "#101010"))))
                                '(region ((((class color) (min-colors 256))
                                           (:background "#3584e4")))))
        (enable-theme 'adwaita-test-overlay)
        (let ((with-overlay
               (list :themes (copy-sequence custom-enabled-themes)
                     :default-registration (copy-tree (get 'default 'theme-face))
                     :region-registration (copy-tree (get 'region 'theme-face))
                     :string-registration (copy-tree (get 'font-lock-string-face
                                                          'theme-face)))))
          (disable-theme 'adwaita-test-overlay)
          (list :adwaita-only adwaita-only
                :with-overlay with-overlay
                :after-removing-overlay
                (list :themes (copy-sequence custom-enabled-themes)
                      :registered (adwaita-test-registered '(default region)))))))
  (adwaita-test-reset))"##,
        expect![[
            r##"OK (:adwaita-only (:themes (adwaita-dark) :registered ((default #1=((class color) (min-colors 256)) #2=(:background "gray11" :foreground "gray86")) (region #1# #3=(:background "gray27" :distant-foreground "gray86")) (font-lock-string-face #1# (:foreground "mediumaquamarine")))) :with-overlay (:themes (adwaita-test-overlay adwaita-dark) :default-registration ((adwaita-test-overlay ((((class color) (min-colors 256)) (:background "#101010")))) (adwaita-dark ((((class color) (min-colors 256)) (:background "gray11" :foreground "gray86"))))) :region-registration ((adwaita-test-overlay ((((class color) (min-colors 256)) (:background "#3584e4")))) (adwaita-dark ((((class color) (min-colors 256)) (:background "gray27" :distant-foreground "gray86"))))) :string-registration ((adwaita-dark ((((class color) (min-colors 256)) (:foreground "mediumaquamarine")))))) :after-removing-overlay (:themes (adwaita-dark) :registered ((default #1# #2#) (region #1# #3#))))"##
        ]],
    )
}

fn the_fringe_extras_install_the_themes_own_bitmaps() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_fringe_extras_install_the_themes_own_bitmaps",
        r##"(unwind-protect
    (progn
      (adwaita-test-reset)
      (load-theme 'adwaita-dark t)
      (let ((before (mapcar (lambda (bitmap) (cons bitmap (get bitmap 'fringe)))
                            '(right-arrow left-arrow
                              right-curly-arrow left-curly-arrow))))
        (list :before before
              :arrows (list (adwaita-dark-theme-arrow-fringe-bmp-enable)
                            (mapcar (lambda (bitmap)
                                      (cons bitmap (get bitmap 'fringe)))
                                    '(right-arrow left-arrow
                                      right-curly-arrow left-curly-arrow)))
              :flymake (progn (adwaita-dark-theme-flymake-fringe-bmp-enable)
                              (list (bound-and-true-p flymake-error-bitmap)
                                    (bound-and-true-p flymake-warning-bitmap)
                                    (bound-and-true-p flymake-note-bitmap)))
              :diff-hl (progn (adwaita-dark-theme-diff-hl-fringe-bmp-enable)
                              (list (bound-and-true-p diff-hl-fringe-bmp-function)
                                    (funcall (bound-and-true-p
                                              diff-hl-fringe-bmp-function)
                                             'insert 1)
                                    (funcall (bound-and-true-p
                                              diff-hl-fringe-bmp-function)
                                             'delete 2)))
              :bitmap-defined (and (get 'adwaita-dark-theme--diff-hl-bmp 'fringe) t))))
  (adwaita-test-reset))"##,
        expect![
            "OK (:before ((right-arrow . 4) (left-arrow . 3) (right-curly-arrow . 8) (left-curly-arrow . 7)) :arrows (left-curly-arrow ((right-arrow . 4) (left-arrow . 3) (right-curly-arrow . 8) (left-curly-arrow . 7))) :flymake ((adwaita-dark-theme--marker-bmp compilation-error) (adwaita-dark-theme--marker-bmp compilation-warning) (adwaita-dark-theme--marker-bmp compilation-info)) :diff-hl (adwaita-dark-theme--diff-hl-fringe-bmp-function adwaita-dark-theme--diff-hl-bmp adwaita-dark-theme--diff-hl-bmp) :bitmap-defined t)"
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_the_theme_registers_its_documented_palette_for_every_probed_face(),
        each_documented_toggle_changes_the_registered_appearance(),
        disabling_the_theme_restores_the_captured_baseline_exactly(),
        a_second_theme_loaded_on_top_takes_precedence_and_disabling_it_hands_back(),
        the_fringe_extras_install_the_themes_own_bitmaps(),
    ]
}
