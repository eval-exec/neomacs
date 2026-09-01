use expect_test::expect;

use super::ParityBatchCase;

/// The shared core: the kaolin-themes customization surface with its
/// documented defaults and types, the payload verification, and the
/// `custom-theme-load-path' registration the core file installs.
fn the_shared_core_configuration_and_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_shared_core_configuration_and_payload",
        r####"(unwind-protect
    (progn
      (kaolin-test-reset)
      (list
       :source (kaolin-test-source-state)
       :options
       (mapcar
        (lambda (option)
          (list :option option
                :custom-variable-p (and (custom-variable-p option) t)
                :standard (eval (car (get option 'standard-value)))
                :type (get option 'custom-type)))
        '(kaolin-themes-bold
          kaolin-themes-italic
          kaolin-themes-underline
          kaolin-themes-underline-wave
          kaolin-themes-hl-line-colored
          kaolin-themes-distinct-fringe
          kaolin-themes-distinct-company-scrollbar
          kaolin-themes-distinct-metakeys
          kaolin-themes-distinct-paren))
       :available-count
       (length (cl-remove-if-not
                (lambda (theme)
                  (string-prefix-p "kaolin-" (symbol-name theme)))
                (custom-available-themes)))))
  (kaolin-test-reset))"####,
        expect![[
            r#"OK (:source (:upstream-tree "e615d9047d53b6d3153af6a0724a1a64f722d769" :feature t :version "20260619.2211" :autothemer "20260530.2349" :theme-load-path t) :options ((:option kaolin-themes-bold :custom-variable-p t :standard t :type nil) (:option kaolin-themes-italic :custom-variable-p t :standard t :type nil) (:option kaolin-themes-underline :custom-variable-p t :standard t :type nil) (:option kaolin-themes-underline-wave :custom-variable-p t :standard t :type nil) (:option kaolin-themes-hl-line-colored :custom-variable-p t :standard nil :type nil) (:option kaolin-themes-distinct-fringe :custom-variable-p t :standard nil :type nil) (:option kaolin-themes-distinct-company-scrollbar :custom-variable-p t :standard nil :type nil) (:option kaolin-themes-distinct-metakeys :custom-variable-p t :standard t :type nil) (:option kaolin-themes-distinct-paren :custom-variable-p nil :standard nil :type nil)) :available-count 15)"#
        ]],
    )
}

/// Loading the dark aurora theme registers the palette: the 540+ settings
/// include theme-face entries for the probed faces, with the exact
/// autothemer display clause and attribute plists.
fn loading_aurora_registers_the_dark_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_aurora_registers_the_dark_palette",
        r####"(unwind-protect
    (progn
      (kaolin-test-reset)
      (let ((before (list :themes (copy-sequence custom-enabled-themes)
                          :registered
                          (kaolin-test-registered
                           'kaolin-aurora '(default cursor region)))))
        (load-theme 'kaolin-aurora t)
        (list :before before
              :load (list (and (custom-theme-p 'kaolin-aurora) t)
                          (and (custom-theme-enabled-p 'kaolin-aurora) t)
                          (length (get 'kaolin-aurora 'theme-settings)))
              :registered
              (kaolin-test-registered
               'kaolin-aurora
               '(default
                 cursor
                 region
                 highlight
                 font-lock-keyword-face
                 font-lock-string-face
                 font-lock-comment-face
                 font-lock-function-name-face
                 font-lock-type-face
                 mode-line
                 hl-line)))))
  (kaolin-test-reset))"####,
        expect![[
            r##"OK (:before (:themes nil :registered ((default :not-registered) (cursor :not-registered) (region :not-registered))) :load (t t 1252) :registered ((default #1=((class color) (min-colors 16777215)) (:background "#14191e" :foreground "#d4d4d6")) (cursor #1# (:background "#f2f2f2")) (region #1# (:background "#252D35" :foreground "#bebec4")) (highlight #1# (:background "#454459" :foreground "#e6e6e8")) (font-lock-keyword-face #1# (:foreground "#9d81ba")) (font-lock-string-face #1# (:foreground "#f5c791")) (font-lock-comment-face #1# (:background unspecified :foreground "#454459" :italic nil)) (font-lock-function-name-face #1# (:foreground "#0bc9cf")) (font-lock-type-face #1# (:foreground "#62D2DB")) (mode-line #1# (:background "#191F26" :foreground "#bebec4" :bold nil :box (:line-width 2 :color "#1F272E"))) (hl-line #1# (:background "#1F272E"))))"##
        ]],
    )
}

/// A light theme registers a DIFFERENT palette: kaolin-blossom's default
/// and syntax faces carry the light background and its own colors, so a
/// palette mixup between themes fails here.
fn loading_blossom_registers_the_light_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_blossom_registers_the_light_palette",
        r####"(unwind-protect
    (progn
      (kaolin-test-reset)
      (load-theme 'kaolin-blossom t)
      (list
       :load (list (and (custom-theme-enabled-p 'kaolin-blossom) t)
                   (length (get 'kaolin-blossom 'theme-settings)))
       :registered
       (kaolin-test-registered
        'kaolin-blossom
        '(default
          region
          font-lock-keyword-face
          font-lock-string-face
          font-lock-comment-face))))
  (kaolin-test-reset))"####,
        expect![[
            r##"OK (:load (t 1252) :registered ((default #1=((class color) (min-colors 16777215)) (:background "#2E2025" :foreground "#EEEED3")) (region #1# (:background "#453038" :foreground "#bebec4")) (font-lock-keyword-face #1# (:foreground "#dbb68f")) (font-lock-string-face #1# (:foreground "#8ee6d6")) (font-lock-comment-face #1# (:background unspecified :foreground "#6B4B53" :italic nil))))"##
        ]],
    )
}

/// The lifecycle: loading a second theme stacks it on top (both
/// registered), disabling removes exactly its registrations and leaves
/// the first enabled, and re-enabling hands the palette back.
fn the_theme_lifecycle_stacks_disables_and_restores() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_theme_lifecycle_stacks_disables_and_restores",
        r####"(unwind-protect
    (progn
      (kaolin-test-reset)
      (load-theme 'kaolin-aurora t)
      (let ((aurora-only
             (list :themes (copy-sequence custom-enabled-themes)
                   :registered
                   (kaolin-test-registered 'kaolin-aurora '(default)))))
        (load-theme 'kaolin-blossom t)
        (let ((stacked
               (list :themes (copy-sequence custom-enabled-themes)
                     :default-stack
                     (mapcar (lambda (entry) (car entry))
                             (get 'default 'theme-face)))))
          (disable-theme 'kaolin-blossom)
          (let ((after-disable
                 (list :themes (copy-sequence custom-enabled-themes)
                       :default-stack
                       (mapcar (lambda (entry) (car entry))
                               (get 'default 'theme-face)))))
            (list :aurora-only aurora-only
                  :stacked stacked
                  :after-disable after-disable
                  :re-enabled
                  (progn
                    (enable-theme 'kaolin-blossom)
                    (list (copy-sequence custom-enabled-themes)
                          (kaolin-test-registered
                           'kaolin-blossom '(default)))))))))
  (kaolin-test-reset))"####,
        expect![[
            r##"OK (:aurora-only (:themes (kaolin-aurora) :registered ((default #1=((class color) (min-colors 16777215)) (:background "#14191e" :foreground "#d4d4d6")))) :stacked (:themes (kaolin-blossom kaolin-aurora) :default-stack (kaolin-blossom kaolin-aurora)) :after-disable (:themes (kaolin-aurora) :default-stack (kaolin-aurora)) :re-enabled ((kaolin-blossom kaolin-aurora) ((default #1# (:background "#2E2025" :foreground "#EEEED3")))))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_shared_core_configuration_and_payload(),
        loading_aurora_registers_the_dark_palette(),
        loading_blossom_registers_the_light_palette(),
        the_theme_lifecycle_stacks_disables_and_restores(),
    ]
}
