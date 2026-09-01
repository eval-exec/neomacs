use expect_test::expect;

use super::ParityBatchCase;

/// Enabling the theme: `load-theme' finds it on the package's own
/// `custom-theme-load-path' entry, registers its face and variable settings,
/// and each face an editing session shows carries the exact palette entry the
/// theme documents -- a wrong colour, a wrong display clause or a dropped
/// face all fail here.  The display facts are pinned too: a batch frame is a
/// 0-colour `mono' display, so the theme's `((class color) (min-colors 89))'
/// clause matches nothing and the registered palette is not realised on this
/// display in either editor.  The quirkier specs are pinned verbatim: the
/// stray `nil,' symbol in `mc/cursor-face', the plain `(t ...)' clauses of
/// `gnus-summary-low-read' and `button', and the nil-attribute registrations
/// of `border-glyph' and `toolbar'.
fn loading_the_theme_registers_its_documented_palette_for_every_probed_face() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_the_theme_registers_its_documented_palette_for_every_probed_face",
        r##"(let ((before (list :themes (copy-sequence custom-enabled-themes)
                    :registered (cyberpunk-test-registered
                                 '(default mode-line font-lock-keyword-face))
                    :resolved (cyberpunk-test-resolved '(default mode-line)))))
  (unwind-protect
      (progn
        (cyberpunk-test-reset)
        (list :before before
              :available (and (memq 'cyberpunk (custom-available-themes)) t)
              :load (list (load-theme 'cyberpunk t)
                          (copy-sequence custom-enabled-themes)
                          (and (custom-theme-p 'cyberpunk) t)
                          (and (custom-theme-enabled-p 'cyberpunk) t)
                          (length (get 'cyberpunk 'theme-settings))
                          (let (kinds)
                            (dolist (setting (get 'cyberpunk 'theme-settings))
                              (cl-pushnew (car setting) kinds))
                            kinds))
              :source (cyberpunk-test-source-state)
              :display (cyberpunk-test-display-facts)
              :registered (cyberpunk-test-registered cyberpunk-test-probed-faces)
              :resolved (cyberpunk-test-resolved
                         '(default mode-line font-lock-keyword-face))))
    (cyberpunk-test-reset)))"##,
        expect![[
            r##"OK (:before (:themes nil :registered ((default :not-registered) (mode-line :not-registered) (font-lock-keyword-face :not-registered)) :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified))) :available t :load (t (cyberpunk) t t 540 (theme-value theme-face)) :source (:upstream-tree "ba60092e03567df0d581b1d8504f7d814c6de122" :feature t :version "20240112.1944" :theme-load-path t) :display (:graphic nil :daemon nil :display-type mono :color-cells 0 :matches-theme-clause nil :matches-any-display t) :registered ((default #1=((class color) (min-colors 89)) (:foreground "#d3d3d3" :background "#000000")) (cursor #1# (:background "#dcdccc")) (fringe #1# (:foreground "#dcdccc" :background "#2b2b2b")) (highlight #1# (:background "#333333")) (mode-line #1# (:foreground "#4c83ff" :background "#333333" :box (:line-width -1 :color "#4c83ff"))) (mode-line-inactive #1# (:foreground "#4D4D4D" :background "#1A1A1A" :box (:line-width -1 :color "#4c83ff"))) (minibuffer-prompt #1# (:foreground "#61CE3C" :background "#000000")) (vertical-border #1# (:foreground "#333333" :background "#000000")) (trailing-whitespace #1# (:background "#ff0000")) (secondary-selection #1# (:background "#5f5f5f")) (link #1# (:foreground "#ffff00" :underline t :weight bold)) (link-visited #1# (:foreground "#d0bf8f" :underline t :weight normal)) (header-line #1# (:foreground "#ffff00" :background "#2b2b2b" :box (:line-width -1 :style released-button))) (font-lock-keyword-face #1# (:foreground "#4c83ff")) (font-lock-string-face #1# (:foreground "#61CE3C")) (font-lock-comment-face #1# (:foreground "#8B8989" :italic t)) (font-lock-function-name-face #1# (:foreground "#ff1493")) (font-lock-variable-name-face #1# (:foreground "#ff69b4")) (font-lock-constant-face #1# (:foreground "#96CBFE")) (font-lock-type-face #1# (:foreground "#afd8af")) (font-lock-builtin-face #1# (:foreground "#4c83ff")) (font-lock-doc-face #1# (:foreground "#FBDE2D")) (font-lock-preprocessor-face #1# (:foreground "#919191")) (font-lock-warning-face #1# (:foreground "#ff69b4")) (font-lock-reference-face #1# (:foreground "#d3d3d3")) (c-annotation-face #1# (:inherit font-lock-constant-face)) (isearch #1# (:foreground "#000000" :background "#ff1493")) (isearch-fail #1# (:background "#8b0000")) (lazy-highlight #1# (:foreground "#000000" :background "#ffff00")) (show-paren-match #1# (:foreground "#000000" :background "#ff1493")) (show-paren-mismatch #1# (:foreground "#9c6363" :background "#000000")) (whitespace-tab #1# (:background "#000000" :foreground "#ff0000")) (whitespace-line #1# (:background "#383838" :foreground "#dc8cc3")) (org-level-1 #1# (:foreground "#ff1493")) (org-level-2 #1# (:foreground "#ffff00")) (org-level-3 #1# (:foreground "#4c83ff")) (org-link #1# (:foreground "#96CBFE" :underline t)) (dired-symlink-face #1# (:foreground "#ff69b4")) (mc/cursor-face #1# (:inverse-video nil :background "#ff69b4" :foreground "#000000")) (gnus-summary-low-read t (:foreground "#00ff00")) (button t (:underline t)) (border-glyph #1# (nil)) (toolbar #1# (nil))) :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified)))"##
        ]],
    )
}

/// The `custom-theme-set-variables' half of the theme: the registered
/// `theme-value' settings (`ansi-color-names-vector', `fci-rule-color'),
/// the live `ansi-color-names-vector' after `load-theme', the documented
/// `cyberpunk-transparent-background' defcustom, and the platform toggle.
/// The toggle is read while the file loads, so each setting gets its own
/// reload; on this platform it is a documented no-op (it only rewrites
/// `cyberpunk-black' for a darwin terminal), which the reload pins as the
/// agreed same-colour outcome.
fn theme_variables_land_and_the_platform_toggle_changes_nothing_here() -> ParityBatchCase {
    ParityBatchCase::value(
        "theme_variables_land_and_the_platform_toggle_changes_nothing_here",
        r##"(unwind-protect
    (progn
      (cyberpunk-test-reset)
      (load-theme 'cyberpunk t)
      (let ((settings (get 'cyberpunk 'theme-settings)))
        (list
         :theme-values
         (let (values)
           (dolist (setting settings)
             (when (eq (car setting) 'theme-value)
               (push (cons (cadr setting)
                           (if (vectorp (cadddr setting))
                               (append (cadddr setting) nil)
                             (cadddr setting)))
                     values)))
           (nreverse values))
         :ansi-color-names-vector (append ansi-color-names-vector nil)
         :defcustom
         (list :p (and (custom-variable-p 'cyberpunk-transparent-background) t)
               :standard (eval (car (get 'cyberpunk-transparent-background
                                         'standard-value)))
               :type (get 'cyberpunk-transparent-background 'custom-type)
               :group (get 'cyberpunk-transparent-background 'custom-group))
         :theme-feature (get 'cyberpunk 'theme-feature)
         :toggle-on-default
         (cyberpunk-test-registered-with 'cyberpunk-transparent-background t
                                         '(default cursor))
         :toggle-off-default
         (cyberpunk-test-registered '(default cursor)))))
  (cyberpunk-test-reset))"##,
        expect![[
            r##"OK (:theme-values ((fci-rule-color . "#383838") (ansi-color-names-vector "#000000" "#8b0000" "#00ff00" "#ffa500" "#7b68ee" "#dc8cc3" "#93e0e3" "#dcdccc")) :ansi-color-names-vector ("#000000" "#8b0000" "#00ff00" "#ffa500" "#7b68ee" "#dc8cc3" "#93e0e3" "#dcdccc") :defcustom (:p t :standard nil :type nil :group nil) :theme-feature cyberpunk-theme :toggle-on-default ((default #1=((class color) (min-colors 89)) (:foreground "#d3d3d3" :background "#000000")) (cursor #1# (:background "#dcdccc"))) :toggle-off-default ((default :not-registered) (cursor :not-registered)))"##
        ]],
    )
}

/// Disabling the theme: the captured baseline appearance, face
/// registrations, and `ansi-color-names-vector' all come back exactly,
/// the theme stays loaded but disabled with its settings intact, and
/// `enable-theme' hands the palette back.
fn disabling_the_theme_restores_the_captured_baseline_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_the_theme_restores_the_captured_baseline_exactly",
        r##"(unwind-protect
    (let ((baseline (list :themes (copy-sequence custom-enabled-themes)
                          :resolved (cyberpunk-test-resolved
                                     cyberpunk-test-probed-faces)
                          :registered (cyberpunk-test-registered
                                       '(default mode-line region))
                          :ansi (append ansi-color-names-vector nil))))
      (cyberpunk-test-reset)
      (load-theme 'cyberpunk t)
      (let ((enabled (list :themes (copy-sequence custom-enabled-themes)
                           :enabled-p (and (custom-theme-enabled-p 'cyberpunk) t)
                           :resolved (cyberpunk-test-resolved
                                      cyberpunk-test-probed-faces)
                           :ansi (append ansi-color-names-vector nil))))
        (disable-theme 'cyberpunk)
        (let ((disabled (list :themes (copy-sequence custom-enabled-themes)
                              :enabled-p (and (custom-theme-enabled-p 'cyberpunk) t)
                              :loaded-p (and (custom-theme-p 'cyberpunk) t)
                              :settings (length (get 'cyberpunk 'theme-settings))
                              :resolved (cyberpunk-test-resolved
                                         cyberpunk-test-probed-faces)
                              :registered (cyberpunk-test-registered
                                           '(default mode-line region))
                              :ansi (append ansi-color-names-vector nil))))
          (list :baseline-restored (equal (plist-get baseline :resolved)
                                          (plist-get disabled :resolved))
                :registration-removed (equal (plist-get baseline :registered)
                                             (plist-get disabled :registered))
                :ansi-restored (equal (plist-get baseline :ansi)
                                      (plist-get disabled :ansi))
                :baseline baseline
                :enabled enabled
                :disabled disabled
                :re-enabled (progn (enable-theme 'cyberpunk)
                                   (list (copy-sequence custom-enabled-themes)
                                         (cyberpunk-test-registered '(default))))))))
  (cyberpunk-test-reset))"##,
        expect![[
            r##"OK (:baseline-restored t :registration-removed t :ansi-restored t :baseline (:themes nil :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (cursor :foreground unspecified :background "white" :weight unspecified :box unspecified) (fringe :foreground unspecified :background "gray" :weight unspecified :box unspecified) (highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-inactive :foreground unspecified :background unspecified :weight unspecified :box unspecified) (minibuffer-prompt :foreground "cyan" :background unspecified :weight unspecified :box unspecified) (vertical-border :foreground unspecified :background unspecified :weight unspecified :box unspecified) (trailing-whitespace :foreground unspecified :background unspecified :weight unspecified :box unspecified) (secondary-selection :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link-visited :foreground unspecified :background unspecified :weight unspecified :box unspecified) (header-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-string-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-comment-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-function-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-variable-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-constant-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-type-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-builtin-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-doc-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-preprocessor-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-warning-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-reference-face :defined nil) (c-annotation-face :defined nil) (isearch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (isearch-fail :foreground unspecified :background unspecified :weight unspecified :box unspecified) (lazy-highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-match :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-mismatch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (whitespace-tab :defined nil) (whitespace-line :defined nil) (org-level-1 :defined nil) (org-level-2 :defined nil) (org-level-3 :defined nil) (org-link :defined nil) (dired-symlink-face :defined nil) (mc/cursor-face :defined nil) (gnus-summary-low-read :defined nil) (button :foreground unspecified :background unspecified :weight unspecified :box unspecified) (border-glyph :defined nil) (toolbar :defined nil)) :registered ((default :not-registered) (mode-line :not-registered) (region :not-registered)) :ansi ("black" "red3" "green3" "yellow3" "blue2" "magenta3" "cyan3" "gray90")) :enabled (:themes (cyberpunk) :enabled-p t :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (cursor :foreground unspecified :background "white" :weight unspecified :box unspecified) (fringe :foreground unspecified :background "gray" :weight unspecified :box unspecified) (highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-inactive :foreground unspecified :background unspecified :weight unspecified :box unspecified) (minibuffer-prompt :foreground "cyan" :background unspecified :weight unspecified :box unspecified) (vertical-border :foreground unspecified :background unspecified :weight unspecified :box unspecified) (trailing-whitespace :foreground unspecified :background unspecified :weight unspecified :box unspecified) (secondary-selection :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link-visited :foreground unspecified :background unspecified :weight unspecified :box unspecified) (header-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-string-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-comment-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-function-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-variable-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-constant-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-type-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-builtin-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-doc-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-preprocessor-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-warning-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-reference-face :defined nil) (c-annotation-face :defined nil) (isearch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (isearch-fail :foreground unspecified :background unspecified :weight unspecified :box unspecified) (lazy-highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-match :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-mismatch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (whitespace-tab :defined nil) (whitespace-line :defined nil) (org-level-1 :defined nil) (org-level-2 :defined nil) (org-level-3 :defined nil) (org-link :defined nil) (dired-symlink-face :defined nil) (mc/cursor-face :defined nil) (gnus-summary-low-read :defined nil) (button :foreground unspecified :background unspecified :weight unspecified :box unspecified) (border-glyph :defined nil) (toolbar :defined nil)) :ansi ("#000000" "#8b0000" "#00ff00" "#ffa500" "#7b68ee" "#dc8cc3" "#93e0e3" "#dcdccc")) :disabled (:themes nil :enabled-p nil :loaded-p t :settings 540 :resolved ((default :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :box nil) (cursor :foreground unspecified :background "white" :weight unspecified :box unspecified) (fringe :foreground unspecified :background "gray" :weight unspecified :box unspecified) (highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (mode-line-inactive :foreground unspecified :background unspecified :weight unspecified :box unspecified) (minibuffer-prompt :foreground "cyan" :background unspecified :weight unspecified :box unspecified) (vertical-border :foreground unspecified :background unspecified :weight unspecified :box unspecified) (trailing-whitespace :foreground unspecified :background unspecified :weight unspecified :box unspecified) (secondary-selection :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link :foreground unspecified :background unspecified :weight unspecified :box unspecified) (link-visited :foreground unspecified :background unspecified :weight unspecified :box unspecified) (header-line :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-keyword-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-string-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-comment-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-function-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-variable-name-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-constant-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-type-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-builtin-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-doc-face :foreground unspecified :background unspecified :weight unspecified :box unspecified) (font-lock-preprocessor-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-warning-face :foreground unspecified :background unspecified :weight bold :box unspecified) (font-lock-reference-face :defined nil) (c-annotation-face :defined nil) (isearch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (isearch-fail :foreground unspecified :background unspecified :weight unspecified :box unspecified) (lazy-highlight :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-match :foreground unspecified :background unspecified :weight unspecified :box unspecified) (show-paren-mismatch :foreground unspecified :background unspecified :weight unspecified :box unspecified) (whitespace-tab :defined nil) (whitespace-line :defined nil) (org-level-1 :defined nil) (org-level-2 :defined nil) (org-level-3 :defined nil) (org-link :defined nil) (dired-symlink-face :defined nil) (mc/cursor-face :defined nil) (gnus-summary-low-read :defined nil) (button :foreground unspecified :background unspecified :weight unspecified :box unspecified) (border-glyph :defined nil) (toolbar :defined nil)) :registered ((default :not-registered) (mode-line :not-registered) (region :not-registered)) :ansi ("black" "red3" "green3" "yellow3" "blue2" "magenta3" "cyan3" "gray90")) :re-enabled ((cyberpunk) ((default ((class color) (min-colors 89)) (:foreground "#d3d3d3" :background "#000000")))))"##
        ]],
    )
}

/// A personal overlay theme loaded on top takes precedence for the faces it
/// declares and hands them back to cyberpunk when disabled.
fn a_second_theme_loaded_on_top_takes_precedence_and_disabling_it_hands_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_second_theme_loaded_on_top_takes_precedence_and_disabling_it_hands_back",
        r##"(unwind-protect
    (progn
      (cyberpunk-test-reset)
      (load-theme 'cyberpunk t)
      (let ((cyberpunk-only (list :themes (copy-sequence custom-enabled-themes)
                                  :registered (cyberpunk-test-registered
                                               '(default region
                                                 font-lock-string-face)))))
        (eval '(deftheme cyberpunk-test-overlay "A personal overlay theme.") t)
        (custom-theme-set-faces 'cyberpunk-test-overlay
                                '(default ((((class color) (min-colors 89))
                                            (:background "#101010"))))
                                '(region ((((class color) (min-colors 89))
                                           (:background "#3584e4")))))
        (enable-theme 'cyberpunk-test-overlay)
        (let ((with-overlay
               (list :themes (copy-sequence custom-enabled-themes)
                     :default-registration (copy-tree (get 'default 'theme-face))
                     :region-registration (copy-tree (get 'region 'theme-face))
                     :string-registration (copy-tree (get 'font-lock-string-face
                                                          'theme-face)))))
          (disable-theme 'cyberpunk-test-overlay)
          (list :cyberpunk-only cyberpunk-only
                :with-overlay with-overlay
                :after-removing-overlay
                (list :themes (copy-sequence custom-enabled-themes)
                      :registered (cyberpunk-test-registered
                                   '(default region)))))))
  (cyberpunk-test-reset))"##,
        expect![[
            r##"OK (:cyberpunk-only (:themes (cyberpunk) :registered ((default #1=((class color) (min-colors 89)) #2=(:foreground "#d3d3d3" :background "#000000")) (region #1# #3=(:background "#7F073F")) (font-lock-string-face #1# (:foreground "#61CE3C")))) :with-overlay (:themes (cyberpunk-test-overlay cyberpunk) :default-registration ((cyberpunk-test-overlay ((((class color) (min-colors 89)) (:background "#101010")))) (cyberpunk ((((class color) (min-colors 89)) (:foreground "#d3d3d3" :background "#000000"))))) :region-registration ((cyberpunk-test-overlay ((((class color) (min-colors 89)) (:background "#3584e4")))) (cyberpunk ((((class color) (min-colors 89)) (:background "#7F073F"))))) :string-registration ((cyberpunk ((((class color) (min-colors 89)) (:foreground "#61CE3C")))))) :after-removing-overlay (:themes (cyberpunk) :registered ((default #1# #2#) (region #1# #3#))))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_the_theme_registers_its_documented_palette_for_every_probed_face(),
        theme_variables_land_and_the_platform_toggle_changes_nothing_here(),
        disabling_the_theme_restores_the_captured_baseline_exactly(),
        a_second_theme_loaded_on_top_takes_precedence_and_disabling_it_hands_back(),
    ]
}
