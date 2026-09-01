use expect_test::expect;

use super::ParityBatchCase;

/// The install route the package's own summary line documents -- "Make sure to
/// (load-theme 'ahungry)" -- rather than the `enable-theme' the other two files
/// use on an already-registered theme.
///
/// For `load-theme' to find the file at all, the `;;;###autoload' form at the
/// bottom of the theme has to have put the package's own directory on
/// `custom-theme-load-path', so that is asserted first.  Loading then enables
/// the theme and registers 216 settings: 215 face specs across 214 distinct
/// faces -- the two counts differ because `link' is specified twice, which the
/// duplicate-spec workflow takes up -- plus one variable setting.
///
/// The theme also ends with a `custom-theme-set-variables' block declaring a
/// global variable named `red'.  Enabling the theme does not bind it: `red' is
/// not a defcustom, so there is nothing for the theme to set, and the block is
/// inert.  `boundp' is reported separately from the value so that "unbound"
/// and "bound to nil" cannot be confused -- without that the assertion would
/// read the same either way.
fn the_documented_load_theme_route_registers_the_faces_and_a_global_red_variable() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_documented_load_theme_route_registers_the_faces_and_a_global_red_variable",
        r##"(let* ((directory (file-name-directory (getenv "NEOMACS_PACKAGE_SOURCE")))
       (observed nil))
  (ahungry-test-with-theme-off
   (lambda ()
     (push (list :before
                 (list :on-load-path
                       (and (member (file-name-as-directory directory)
                                    custom-theme-load-path)
                            t)
                       :enabled (and (memq 'ahungry custom-enabled-themes) t)
                       :red-bound (boundp 'red)))
           observed)
     (load-theme 'ahungry t)
     (push (list :after-load-theme
                 (list :enabled (and (memq 'ahungry custom-enabled-themes) t)
                       :is-a-theme (and (custom-theme-p 'ahungry) t)
                       :faces-set (length (ahungry-test-theme-faces))
                       :spec-count (length (get 'ahungry 'theme-settings))
                       :red-bound (boundp 'red)
                       :red (and (boundp 'red) (symbol-value 'red))
                       :default-foreground
                       (face-attribute 'default :foreground nil 'default)))
           observed)
     (disable-theme 'ahungry)
     (push (list :after-disable
                 (list :enabled (and (memq 'ahungry custom-enabled-themes) t)
                       :still-a-theme (and (custom-theme-p 'ahungry) t)
                       :red-bound (boundp 'red)
                       :red (and (boundp 'red) (symbol-value 'red))))
           observed)))
  (nreverse observed))"##,
        expect![[
            r##"OK ((:before (:on-load-path t :enabled nil :red-bound nil)) (:after-load-theme (:enabled t :is-a-theme t :faces-set 214 :spec-count 216 :red-bound nil :red nil :default-foreground "#ffffff")) (:after-disable (:enabled nil :still-a-theme t :red-bound nil :red nil)))"##
        ]],
    )
}

fn the_transparent_background_is_a_load_time_display_graphic_p_gate_on_one_face() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_transparent_background_is_a_load_time_display_graphic_p_gate_on_one_face",
        r##"(let ((observed nil))
  (push (list :gate (list :display-graphic-p (display-graphic-p)
                          :mainbg-would-be (when (display-graphic-p) "#101010")))
        observed)
  (push (list :terminal-branch
              (list :stored-default-spec (ahungry-test-stored-spec 'default)
                    :faces-with-no-background
                    (seq-filter
                     (lambda (face)
                       (let ((spec (ahungry-test-stored-spec face)))
                         (and (plist-member (car (cdar spec)) :background)
                              (null (plist-get (car (cdar spec)) :background)))))
                     (ahungry-test-theme-faces))))
        observed)
  (enable-theme 'ahungry)
  (push (list :terminal-branch-resolved
              (list :default-background
                    (face-attribute 'default :background nil 'default)
                    :default-foreground
                    (face-attribute 'default :foreground nil 'default)))
        observed)
  (disable-theme 'ahungry)
  (nreverse observed))"##,
        expect![[
            r##"OK ((:gate (:display-graphic-p nil :mainbg-would-be nil)) (:terminal-branch (:stored-default-spec ((t (:foreground "#ffffff" :background nil :family "Terminus" :foundry "xos4" :slant normal :weight normal :height 130 :width normal))) :faces-with-no-background (erc-timestamp-face erc-prompt-face italic bold default))) (:terminal-branch-resolved (:default-background unspecified :default-foreground "#ffffff")))"##
        ]],
    )
    .fresh_process()
}

fn setting_the_font_settings_variable_only_takes_effect_when_the_theme_is_reloaded()
-> ParityBatchCase {
    ParityBatchCase::value(
        "setting_the_font_settings_variable_only_takes_effect_when_the_theme_is_reloaded",
        r##"(let ((source (getenv "NEOMACS_PACKAGE_SOURCE"))
      (original ahungry-theme-font-settings)
      (observed nil))
  (unwind-protect
      (progn
        (push (list :shipped-default
                    (list :value (copy-tree ahungry-theme-font-settings)
                          :docstring-claims-height
                          (and (string-match ":height \\([0-9]+\\)"
                                             (documentation-property
                                              'ahungry-theme-font-settings
                                              'variable-documentation))
                               (match-string 1
                                             (documentation-property
                                              'ahungry-theme-font-settings
                                              'variable-documentation)))))
              observed)
        (enable-theme 'ahungry)
        (push (list :with-the-shipped-font
                    (list :family (face-attribute 'default :family nil 'default)
                          :foundry (face-attribute 'default :foundry nil 'default)
                          :height (face-attribute 'default :height nil 'default)))
              observed)
        ;; Setting the variable and re-enabling is what a user would try first.
        (setq ahungry-theme-font-settings nil)
        (disable-theme 'ahungry)
        (enable-theme 'ahungry)
        (push (list :set-to-nil-and-re-enabled
                    (list :family (face-attribute 'default :family nil 'default)
                          :height (face-attribute 'default :height nil 'default)))
              observed)
        ;; Only reloading the file re-evaluates the splice.
        (load source nil t t)
        (enable-theme 'ahungry)
        (push (list :set-to-nil-and-reloaded
                    (list :stored-default-spec
                          (ahungry-test-stored-spec 'default)
                          :family (face-attribute 'default :family nil 'default)
                          :height (face-attribute 'default :height nil 'default)))
              observed))
    (setq ahungry-theme-font-settings original)
    (load source nil t t)
    (when (memq 'ahungry custom-enabled-themes) (disable-theme 'ahungry)))
  (nreverse observed))"##,
        expect![[
            r##"OK ((:shipped-default (:value (:family "Terminus" :foundry "xos4" :slant normal :weight normal :height 130 :width normal) :docstring-claims-height "100")) (:with-the-shipped-font (:family "default" :foundry "default" :height 130)) (:set-to-nil-and-re-enabled (:family "default" :height 130)) (:set-to-nil-and-reloaded (:stored-default-spec ((t (:foreground "#ffffff" :background nil))) :family "default" :height 1)))"##
        ]],
    )
}

fn the_duplicate_link_spec_leaves_the_later_colour_dead_and_hackernews_link_mismatched()
-> ParityBatchCase {
    ParityBatchCase::value(
        "the_duplicate_link_spec_leaves_the_later_colour_dead_and_hackernews_link_mismatched",
        r##"(let ((observed nil))
  ;; The theme styles `hackernews-link' for users who have hackernews.el; it is
  ;; not installed here, so stand in for it the way `rendering.rs' does for the
  ;; helm faces.  Without this the face does not exist and `face-attribute'
  ;; signals rather than reporting the theme's colour.
  (unless (facep 'hackernews-link) (make-face 'hackernews-link))
  (push (list :registered
              (list :link-spec-count (ahungry-test-face-spec-count 'link)
                    :every-link-spec (ahungry-test-all-stored-specs 'link)
                    :hackernews-link-spec
                    (ahungry-test-stored-spec 'hackernews-link)))
        observed)
  (enable-theme 'ahungry)
  (push (list :in-force
              (list :link (ahungry-test-resolved 'link ahungry-test-colour)
                    :hackernews-link
                    (ahungry-test-resolved 'hackernews-link ahungry-test-colour)
                    :they-match
                    (equal (ahungry-test-resolved 'link ahungry-test-colour)
                           (ahungry-test-resolved 'hackernews-link
                                                  ahungry-test-colour))))
        observed)
  (with-temp-buffer
    (set-window-buffer (selected-window) (current-buffer))
    (insert (propertize "documentation" 'face 'link) "\n"
            (propertize "front page" 'face 'hackernews-link) "\n")
    (goto-char (point-min))
    (search-forward "documentation")
    (let ((link-position (- (point) (length "documentation"))))
      (search-forward "front page")
      (push (list :rendered
                  (list :link-foreground
                        (face-attribute
                         (get-text-property link-position 'face)
                         :foreground nil 'default)
                        :hackernews-foreground
                        (face-attribute
                         (get-text-property (- (point) (length "front page"))
                                            'face)
                         :foreground nil 'default)))
            observed)))
  (disable-theme 'ahungry)
  (nreverse observed))"##,
        expect![[
            r##"OK ((:registered (:link-spec-count 2 :every-link-spec (((t (:foreground "#af0"))) ((t (:underline t :foreground "#33ff99")))) :hackernews-link-spec ((t (:foreground "#af0"))))) (:in-force (:link ((:foreground . "#33ff99") (:weight . normal) (:slant . normal) (:underline . t)) :hackernews-link ((:foreground . "#af0") (:weight . normal) (:slant . normal)) :they-match nil)) (:rendered (:link-foreground "#33ff99" :hackernews-foreground "#af0")))"##
        ]],
    )
}

fn enabling_the_theme_drops_stock_attributes_from_faces_it_does_not_restate() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_the_theme_drops_stock_attributes_from_faces_it_does_not_restate",
        r##"(let ((observed nil))
  (ahungry-test-with-theme-off
   (lambda ()
     (let* ((themed (ahungry-test-theme-faces))
            (existing (seq-filter #'facep themed))
            (before (ahungry-test-capture existing))
            (after nil)
            (restored nil)
            (losses nil))
       (enable-theme 'ahungry)
       (setq after (ahungry-test-capture existing))
       (setq losses (ahungry-test-losses before after))
       (disable-theme 'ahungry)
       (setq restored (ahungry-test-capture existing))
       (push (list :sizes
                   (list :faces-the-theme-sets (length themed)
                         :already-existing (length existing)
                         :losing-at-least-one-attribute (length losses)))
             observed)
       (push (list :losses losses) observed)
       (push (list :restored-on-disable (equal before restored)) observed))))
  (nreverse observed))"##,
        expect![[
            r#"OK ((:sizes (:faces-the-theme-sets 214 :already-existing 28 :losing-at-least-one-attribute 14)) (:losses ((link (:inherit) ((((class color) (min-colors 88) (background light)) :foreground "RoyalBlue3" :underline t) (((class color) (background light)) :foreground "blue" :underline t) (((class color) (min-colors 88) (background dark)) :foreground "cyan1" :underline t) (((class color) (background dark)) :foreground "cyan" :underline t) (t :inherit underline))) (button (:inherit) ((t :inherit link))) (isearch (:inverse-video) ((((class color) (min-colors 88) (background light)) (:background "magenta3" :foreground "lightskyblue1")) (((class color) (min-colors 88) (background dark)) (:background "palevioletred2" :foreground "brown4")) (((class color) (min-colors 16)) (:background "magenta4" :foreground "cyan1")) (((class color) (min-colors 8)) (:background "magenta4" :foreground "cyan1")) (t (:inverse-video t)))) (font-lock-function-name-face (:inverse-video) ((((class color) (min-colors 88) (background light)) :foreground "Blue1") (((class color) (min-colors 88) (background dark)) :foreground "LightSkyBlue") (((class color) (min-colors 16) (background light)) :foreground "Blue") (((class color) (min-colors 16) (background dark)) :foreground "LightSkyBlue") (((class color) (min-colors 8)) :foreground "blue" :weight bold) (t :inverse-video t :weight bold))) (font-lock-warning-face (:inverse-video :inherit) ((t :inherit error))) (font-lock-type-face (:underline) ((((class grayscale) (background light)) :foreground "Gray90" :weight bold) (((class grayscale) (background dark)) :foreground "DimGray" :weight bold) (((class color) (min-colors 88) (background light)) :foreground "ForestGreen") (((class color) (min-colors 88) (background dark)) :foreground "PaleGreen") (((class color) (min-colors 16) (background light)) :foreground "ForestGreen") (((class color) (min-colors 16) (background dark)) :foreground "PaleGreen") (((class color) (min-colors 8)) :foreground "green") (t :weight bold :underline t))) (font-lock-doc-face (:inherit) ((t :inherit font-lock-string-face))) (font-lock-constant-face (:underline) ((((class grayscale) (background light)) :foreground "LightGray" :weight bold :underline t) (((class grayscale) (background dark)) :foreground "Gray50" :weight bold :underline t) (((class color) (min-colors 88) (background light)) :foreground "dark cyan") (((class color) (min-colors 88) (background dark)) :foreground "Aquamarine") (((class color) (min-colors 16) (background light)) :foreground "CadetBlue") (((class color) (min-colors 16) (background dark)) :foreground "Aquamarine") (((class color) (min-colors 8)) :foreground "magenta") (t :weight bold :underline t))) (match (:inverse-video) ((((class color) (min-colors 88) (background light)) :background "khaki1") (((class color) (min-colors 88) (background dark)) :background "RoyalBlue3") (((class color) (min-colors 8) (background light)) :background "yellow" :foreground "black") (((class color) (min-colors 8) (background dark)) :background "blue" :foreground "white") (((type tty) (class mono)) :inverse-video t) (t :background "gray"))) (region (:inverse-video) ((((class color) (min-colors 88) (background dark)) :background "blue3" :extend t) (((class color) (min-colors 88) (background light)) :background "lightgoldenrod2" :extend t) (((class color) (min-colors 16) (background dark)) :background "blue3" :extend t) (((class color) (min-colors 16) (background light)) :background "lightgoldenrod2" :extend t) (((class color) (min-colors 8)) :background "blue" :foreground "white" :extend t) (((type tty) (class mono)) :inverse-video t) (t :background "gray" :extend t))) (mode-line-inactive (:inverse-video :inherit) ((default :inherit mode-line) (((class color grayscale) (min-colors 88) (background light)) :weight light :box (:line-width -1 :color "grey75" :style nil) :foreground "grey20" :background "grey90") (((class color grayscale) (min-colors 88) (background dark)) :weight light :box (:line-width -1 :color "grey40" :style nil) :foreground "grey80" :background "grey30"))) (mode-line (:inverse-video) ((((class color grayscale) (min-colors 88) (background light)) :box (:line-width -1 :style released-button) :background "grey75" :foreground "black") (((class color grayscale) (min-colors 88) (background dark)) :box (:line-width -1 :style released-button) :background "grey20" :foreground "white") (t :inverse-video t))) (error (:inverse-video) ((default :weight bold) (((class color) (min-colors 88) (background light)) :foreground "Red1") (((class color) (min-colors 88) (background dark)) :foreground "Pink") (((class color) (min-colors 16) (background light)) :foreground "Red1") (((class color) (min-colors 16) (background dark)) :foreground "Pink") (((class color) (min-colors 8)) :foreground "red") (t :inverse-video t))) (highlight (:inverse-video) ((((class color) (min-colors 88) (background light)) :background "darkseagreen2") (((class color) (min-colors 88) (background dark)) :background "darkolivegreen") (((class color) (min-colors 16) (background light)) :background "darkseagreen2") (((class color) (min-colors 16) (background dark)) :background "darkolivegreen") (((class color) (min-colors 8)) :background "green" :foreground "black") (t :inverse-video t))))) (:restored-on-disable t))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_documented_load_theme_route_registers_the_faces_and_a_global_red_variable(),
        the_transparent_background_is_a_load_time_display_graphic_p_gate_on_one_face(),
        setting_the_font_settings_variable_only_takes_effect_when_the_theme_is_reloaded(),
        the_duplicate_link_spec_leaves_the_later_colour_dead_and_hackernews_link_mismatched(),
        enabling_the_theme_drops_stock_attributes_from_faces_it_does_not_restate(),
    ]
}
