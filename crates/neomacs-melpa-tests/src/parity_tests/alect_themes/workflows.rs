use expect_test::expect;

use super::ParityBatchCase;

/// The family as shipped: six themes on `custom-available-themes', each
/// generated from its own column of the shared palette.  Loading light, dark
/// and black in turn registers a different colour for every probed face while
/// the face set, the settings count and the variables the theme sets stay the
/// same.  With the stock `alect-display-class' -- graphical terminals only --
/// none of it is realised on a batch display, in either editor; the display
/// facts are pinned so that reads as a property of the terminal rather than of
/// the theme.
fn each_variant_of_the_family_registers_its_own_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "each_variant_of_the_family_registers_its_own_palette",
        r##"(unwind-protect
    (progn
      (alect-test-reset)
      (list
       :available (cl-remove-if-not
                   (lambda (theme) (string-prefix-p "alect" (symbol-name theme)))
                   (custom-available-themes))
       :display-class alect-display-class
       :display (alect-test-display-facts)
       :palette-names (mapcar #'car alect-colors)
       :variants
       (mapcar (lambda (theme)
                 (alect-test-reset)
                 (load-theme theme t)
                 (list theme
                       :enabled (copy-sequence custom-enabled-themes)
                       :settings (alect-test-settings theme)
                       :variables (alect-test-variables theme)
                       :registered (alect-test-registrations)))
               '(alect-light alect-dark alect-black))
       :resolved-on-this-display (alect-test-resolved '(default cursor link))))
  (alect-test-reset))"##,
        expect![[
            r##"OK (:available (alect-black-alt alect-black alect-dark-alt alect-dark alect-light-alt alect-light) :display-class #1=((type graphic)) :display (:graphic nil :display-type mono :color-cells 0 :matches-graphic nil :matches-256 nil :matches-nil t) :palette-names (light dark black) :variants ((alect-light :enabled (alect-light) :settings (:faces 935 :variables 9) :variables (ansi-color-names-vector diary-entry-marker emms-mode-line-icon-color fci-rule-color gnus-logo-colors gnus-mode-line-image-cache vc-annotate-background vc-annotate-color-map vc-annotate-very-old-color) :registered ((default alect-light #1# (:foreground "#262626" :background "#ded6c5")) (cursor alect-light #1# (:background "#1074cd")) (link alect-light #1# (:foreground "#2c53ca" . #2=(:underline t))) (fringe alect-light #1# (:foreground "#909090" :background "#f6f0e1")) (mode-line alect-light #1# (:foreground "#262626" :background "#f6f0e1" . #3=(:box (:line-width 2 :style released-button)))) (font-lock-keyword-face alect-light #1# (:foreground "#2020cc" . #4=(:weight bold))) (font-lock-string-face alect-light #1# (:foreground "#e43838")) (font-lock-comment-face alect-light #1# (:foreground "#008b45")))) (alect-dark :enabled (alect-dark) :settings (:faces 935 :variables 9) :variables (ansi-color-names-vector diary-entry-marker emms-mode-line-icon-color fci-rule-color gnus-logo-colors gnus-mode-line-image-cache vc-annotate-background vc-annotate-color-map vc-annotate-very-old-color) :registered ((default alect-dark #1# (:foreground "#d5d2be" :background "#3f3f3f")) (cursor alect-dark #1# (:background "#d0d060")) (link alect-dark #1# (:foreground "#94bff3" . #2#)) (fringe alect-dark #1# (:foreground "#9f9f9f" :background "#222222")) (mode-line alect-dark #1# (:foreground "#d5d2be" :background "#222222" . #3#)) (font-lock-keyword-face alect-dark #1# (:foreground "#30a5f5" . #4#)) (font-lock-string-face alect-dark #1# (:foreground "#fa5151")) (font-lock-comment-face alect-dark #1# (:foreground "#3cb370")))) (alect-black :enabled (alect-black) :settings (:faces 935 :variables 9) :variables (ansi-color-names-vector diary-entry-marker emms-mode-line-icon-color fci-rule-color gnus-logo-colors gnus-mode-line-image-cache vc-annotate-background vc-annotate-color-map vc-annotate-very-old-color) :registered ((default alect-black #1# (:foreground "#b2af95" :background "#000000")) (cursor alect-black #1# (:background "#b1c721")) (link alect-black #1# (:foreground "#58b1f3" . #2#)) (fringe alect-black #1# (:foreground "#9b9b9b" :background "#404040")) (mode-line alect-black #1# (:foreground "#b2af95" :background "#404040" . #3#)) (font-lock-keyword-face alect-black #1# (:foreground "#1e7bda" . #4#)) (font-lock-string-face alect-black #1# (:foreground "#ea4141")) (font-lock-comment-face alect-black #1# (:foreground "#319448"))))) :resolved-on-this-display ((default "unspecified-fg" "unspecified-bg") (cursor unspecified "white") (link unspecified unspecified)))"##
        ]],
    )
}

fn switching_between_variants_repaints_the_session_and_disabling_restores_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "switching_between_variants_repaints_the_session_and_disabling_restores_it",
        r##"(unwind-protect
    (progn
      (alect-test-reset)
      (setq alect-display-class nil)
      (let ((baseline (alect-test-resolved)))
        (load-theme 'alect-light t)
        (let ((light (list :enabled (copy-sequence custom-enabled-themes)
                           :registered (alect-test-registered 'default)
                           :resolved (alect-test-resolved))))
          (load-theme 'alect-dark t)
          (let ((dark (list :enabled (copy-sequence custom-enabled-themes)
                            :registered (alect-test-registered 'default)
                            :resolved (alect-test-resolved))))
            (disable-theme 'alect-dark)
            (let ((back-to-light (list :enabled (copy-sequence custom-enabled-themes)
                                       :resolved (alect-test-resolved))))
              (disable-theme 'alect-light)
              (let ((cleared (alect-test-resolved)))
                (list :baseline baseline
                      :light light
                      :dark dark
                      :back-to-light back-to-light
                      :light-restored (equal (plist-get light :resolved)
                                             (plist-get back-to-light :resolved))
                      :cleared cleared
                      :baseline-restored (equal baseline cleared)
                      :still-loaded (and (custom-theme-p 'alect-light) t))))))))
  (alect-test-reset))"##,
        expect![[
            r##"OK (:baseline ((default "unspecified-fg" "unspecified-bg") (cursor unspecified "white") (link unspecified unspecified) (fringe unspecified "gray") (mode-line unspecified unspecified) (font-lock-keyword-face unspecified unspecified) (font-lock-string-face unspecified unspecified) (font-lock-comment-face unspecified unspecified)) :light (:enabled (alect-light) :registered (alect-light nil (:foreground "#262626" :background "#ded6c5")) :resolved ((default "#262626" "#ded6c5") (cursor unspecified "#1074cd") (link "#2c53ca" unspecified) (fringe "#909090" "#f6f0e1") (mode-line "#262626" "#f6f0e1") (font-lock-keyword-face "#2020cc" unspecified) (font-lock-string-face "#e43838" unspecified) (font-lock-comment-face "#008b45" unspecified))) :dark (:enabled (alect-dark alect-light) :registered (alect-dark nil (:foreground "#d5d2be" :background "#3f3f3f")) :resolved ((default "#d5d2be" "#3f3f3f") (cursor unspecified "#d0d060") (link "#94bff3" unspecified) (fringe "#9f9f9f" "#222222") (mode-line "#d5d2be" "#222222") (font-lock-keyword-face "#30a5f5" unspecified) (font-lock-string-face "#fa5151" unspecified) (font-lock-comment-face "#3cb370" unspecified))) :back-to-light (:enabled (alect-light) :resolved ((default "#262626" "#ded6c5") (cursor unspecified "#1074cd") (link "#2c53ca" unspecified) (fringe "#909090" "#f6f0e1") (mode-line "#262626" "#f6f0e1") (font-lock-keyword-face "#2020cc" unspecified) (font-lock-string-face "#e43838" unspecified) (font-lock-comment-face "#008b45" unspecified))) :light-restored t :cleared ((default "unspecified-fg" "unspecified-bg") (cursor unspecified "white") (link unspecified unspecified) (fringe unspecified "gray") (mode-line unspecified unspecified) (font-lock-keyword-face unspecified unspecified) (font-lock-string-face unspecified unspecified) (font-lock-comment-face unspecified unspecified)) :baseline-restored t :still-loaded t)"##
        ]],
    )
}

fn changing_a_palette_colour_repaints_every_face_generated_from_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "changing_a_palette_colour_repaints_every_face_generated_from_it",
        r##"(unwind-protect
    (progn
      (alect-test-reset)
      (setq alect-display-class nil)
      (load-theme 'alect-light t)
      (let ((before (list :colours (list (alect-get-color 'light 'bg-1)
                                         (alect-get-color 'light 'blue-1)
                                         (alect-get-color 'light 'green+1))
                          :registered (alect-test-registered 'default)
                          :resolved (alect-test-resolved '(default link cursor)))))
        (disable-theme 'alect-light)
        (alect-set-color 'light 'bg-1 "#fafafa")
        (alect-set-color 'light 'blue-1 "#0000ff")
        (load-theme 'alect-light t)
        (list :before before
              :after (list :colours (list (alect-get-color 'light 'bg-1)
                                          (alect-get-color 'light 'blue-1)
                                          (alect-get-color 'light 'green+1))
                           :registered (alect-test-registered 'default)
                           :resolved (alect-test-resolved '(default link cursor)))
              :other-variants (list (alect-get-color 'dark 'bg-1)
                                    (alect-get-color 'black 'bg-1))
              :unknown-colour (condition-case failure
                                  (alect-set-color 'light 'no-such-colour "#000000")
                                (error failure))
              :unknown-theme (condition-case failure
                                 (alect-set-color 'no-such-theme 'bg-1 "#000000")
                               (error failure)))))
  (alect-test-reset))"##,
        expect![[
            r##"OK (:before (:colours ("#ded6c5" "#2c53ca" "#008b45") :registered (alect-light nil (:foreground "#262626" :background "#ded6c5")) :resolved ((default "#262626" "#ded6c5") (link "#2c53ca" unspecified) (cursor unspecified "#1074cd"))) :after (:colours ("#fafafa" "#0000ff" "#008b45") :registered (alect-light nil (:foreground "#262626" :background "#fafafa")) :resolved ((default "#262626" "#fafafa") (link "#0000ff" unspecified) (cursor unspecified "#1074cd"))) :other-variants ("#3f3f3f" "#000000") :unknown-colour (error "Color ’no-such-colour’ does not exist") :unknown-theme (error "Theme ’no-such-theme’ does not exist"))"##
        ]],
    )
}

fn overriding_and_ignoring_faces_change_what_the_generator_produces() -> ParityBatchCase {
    ParityBatchCase::value(
        "overriding_and_ignoring_faces_change_what_the_generator_produces",
        r##"(unwind-protect
    (progn
      (alect-test-reset)
      (setq alect-display-class nil)
      (load-theme 'alect-light t)
      (let ((stock (alect-test-registrations '(link mode-line-buffer-id cursor))))
        (disable-theme 'alect-light)
        (setq alect-overriding-faces
              '((mode-line-buffer-id ((t :foreground bg-2 :weight bold)))
                (link ((t :foreground magenta :underline nil)))))
        (load-theme 'alect-light t)
        (let ((overridden (list :registered (alect-test-registrations
                                             '(link mode-line-buffer-id cursor))
                                :resolved (alect-test-resolved
                                           '(link mode-line-buffer-id))
                                :substituted (alect-substitute-colors-in-faces
                                              'dark
                                              (copy-tree alect-overriding-faces)))))
          (disable-theme 'alect-light)
          (setq alect-overriding-faces nil
                alect-ignored-faces '(link cursor))
          (load-theme 'alect-light t)
          (list :stock stock
                :overridden overridden
                :ignored (list :registered (alect-test-registrations
                                            '(link cursor default))
                               :settings (alect-test-settings 'alect-light)
                               :resolved (alect-test-resolved
                                          '(link cursor default)))))))
  (alect-test-reset))"##,
        expect![[
            r##"OK (:stock ((link alect-light nil (:foreground "#2c53ca" :underline t)) (mode-line-buffer-id alect-light nil (:foreground "#2c53ca" :weight bold)) (cursor alect-light nil (:background "#1074cd"))) :overridden (:registered ((link alect-light t (:foreground "#a020f0" :underline nil)) (mode-line-buffer-id alect-light t (:foreground "#f6f0e1" :weight bold)) (cursor alect-light nil (:background "#1074cd"))) :resolved ((link "#a020f0" unspecified) (mode-line-buffer-id "#f6f0e1" unspecified)) :substituted ((mode-line-buffer-id ((t :foreground "#222222" :weight bold))) (link ((t :foreground "#e353b9" :underline nil))))) :ignored (:registered ((link) (cursor) (default alect-light nil (:foreground "#262626" :background "#ded6c5"))) :settings (:faces 933 :variables 9) :resolved ((link unspecified unspecified) (cursor unspecified "black") (default "#262626" "#ded6c5"))))"##
        ]],
    )
    .fresh_process()
}

fn the_alt_variants_invert_the_colour_extremes() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_alt_variants_invert_the_colour_extremes",
        r##"(unwind-protect
    (progn
      (alect-test-reset)
      (setq alect-display-class nil)
      (let ((inversion (mapcar (lambda (colour)
                                 (list colour
                                       (alect-get-color 'light colour)
                                       (alect-get-color 'light colour t)))
                               '(blue-1 blue+1 red-2 green+2 fg-1 gray cursor))))
        (load-theme 'alect-light t)
        (let ((plain (list :registered (alect-test-registrations
                                        '(link font-lock-keyword-face))
                           :resolved (alect-test-resolved
                                      '(link font-lock-keyword-face)))))
          (alect-test-reset)
          (load-theme 'alect-light-alt t)
          (let ((inverted (list :enabled (copy-sequence custom-enabled-themes)
                                :registered (alect-test-registrations
                                             '(link font-lock-keyword-face))
                                :resolved (alect-test-resolved
                                           '(link font-lock-keyword-face)))))
            (list :regexp alect-inverted-color-regexp
                  :inversion inversion
                  :plain plain
                  :inverted inverted
                  :generated (alect-generate-colors
                              '(light dark)
                              '((accent "#ff0000" "#00ff00")
                                (edge "#0000ff" "#ffff00"))))))))
  (alect-test-reset))"##,
        expect![[
            r##"OK (:regexp "^\\(red\\|yellow\\|green\\|cyan\\|blue\\|magenta\\)\\([-+]\\)\\([012]\\)$" :inversion ((blue-1 "#2c53ca" "#2020cc") (blue+1 "#2020cc" "#2c53ca") (red-2 "#fa5151" "#b22222") (green+2 "#077707" "#3cb368") (fg-1 "#505050" "#505050") (gray "#909090" "#909090") (cursor "#1074cd" "#1074cd")) :plain (:registered ((link alect-light nil (:foreground "#2c53ca" . #1=(:underline t))) (font-lock-keyword-face alect-light nil (:foreground "#2020cc" . #2=(:weight bold)))) :resolved ((link "#2c53ca" unspecified) (font-lock-keyword-face "#2020cc" unspecified))) :inverted (:enabled (alect-light-alt) :registered ((link alect-light-alt nil (:foreground "#2020cc" . #1#)) (font-lock-keyword-face alect-light-alt nil (:foreground "#2c53ca" . #2#))) :resolved ((link "#2020cc" unspecified) (font-lock-keyword-face "#2c53ca" unspecified))) :generated ((light (edge . "#0000ff") (accent . "#ff0000")) (dark (edge . "#ffff00") (accent . "#00ff00"))))"##
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        each_variant_of_the_family_registers_its_own_palette(),
        switching_between_variants_repaints_the_session_and_disabling_restores_it(),
        changing_a_palette_colour_repaints_every_face_generated_from_it(),
        overriding_and_ignoring_faces_change_what_the_generator_produces(),
        the_alt_variants_invert_the_colour_extremes(),
    ]
}
