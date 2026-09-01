use expect_test::expect;

use super::ParityBatchCase;

fn airline_themes_doom_one_preserves_every_state_and_inactive_face_spec() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_doom_one_preserves_every_state_and_inactive_face_spec",
        r##"(progn
         (load-theme 'airline-doom-one t t)
         (let ((settings
                (get 'airline-doom-one 'theme-settings))
               (faces
                '(airline-normal-outer airline-normal-inner
                  airline-normal-center airline-insert-outer
                  airline-insert-inner airline-insert-center
                  airline-visual-outer airline-visual-inner
                  airline-visual-center airline-replace-outer
                  airline-replace-inner airline-replace-center
                  airline-emacs-outer airline-emacs-inner
                  airline-emacs-center powerline-inactive1
                  powerline-inactive2 airline-inactive3
                  mode-line mode-line-inactive
                  mode-line-buffer-id minibuffer-prompt)))
           (mapcar
            (lambda (face)
              (seq-find
               (lambda (setting)
                 (and (eq (car setting) 'theme-face)
                      (eq (nth 1 setting) face)))
               settings))
            faces)))"##,
        expect![[
            r##"OK ((theme-face airline-normal-outer airline-doom-one ((t (:foreground "#1B2229" :background "#51afef")))) (theme-face airline-normal-inner airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-normal-center airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-insert-outer airline-doom-one ((t (:foreground "#1B2229" :background "#98be65")))) (theme-face airline-insert-inner airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-insert-center airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-visual-outer airline-doom-one ((t (:foreground "#1B2229" :background "#4db5bd")))) (theme-face airline-visual-inner airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-visual-center airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-replace-outer airline-doom-one ((t (:foreground "#1B2229" :background "#ff6c6b")))) (theme-face airline-replace-inner airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-replace-center airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-emacs-outer airline-doom-one ((t (:foreground "#1B2229" :background "#a9a1e1")))) (theme-face airline-emacs-inner airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face airline-emacs-center airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b")))) (theme-face powerline-inactive1 airline-doom-one ((t (:foreground "#5B6268" :background "#23272e")))) (theme-face powerline-inactive2 airline-doom-one ((t (:foreground "#5B6268" :background "#23272e")))) (theme-face airline-inactive3 airline-doom-one ((t (:foreground "#5B6268" :background "#23272e")))) (theme-face mode-line airline-doom-one ((t (:foreground "#bbc2cf" :background "#21242b" :box nil :underline nil :overline nil)))) (theme-face mode-line-inactive airline-doom-one ((t (:foreground "#5B6268" :background "#23272e" :box nil :underline nil :overline nil)))) (theme-face mode-line-buffer-id airline-doom-one ((t (:foreground "#1B2229" :background "#51afef" :box nil :underline nil :overline nil)))) (theme-face minibuffer-prompt airline-doom-one ((t (:foreground "#1B2229" :background "#51afef" :box nil)))))"##
        ]],
    )
}

fn airline_themes_light_dark_transparent_palettes_resolve_distinct_practical_colors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_light_dark_transparent_palettes_resolve_distinct_practical_colors",
        r##"(let ((themes
                '(airline-light
                  airline-dark
                  airline-doom-one
                  airline-transparent))
               (faces
                '(airline-normal-outer
                  airline-normal-inner
                  airline-normal-center
                  airline-insert-outer
                  airline-visual-outer
                  airline-replace-outer
                  airline-inactive3
                  mode-line
                  mode-line-inactive)))
         (mapcar
          (lambda (theme)
            (load-theme theme t t)
            (list
             theme
             (mapcar
              (lambda (face)
                (let ((setting
                       (seq-find
                        (lambda (entry)
                          (and
                           (eq (car entry) 'theme-face)
                           (eq (nth 1 entry) face)))
                        (get theme 'theme-settings))))
                  (list face (nth 3 setting))))
              faces)))
          themes))"##,
        expect![[
            r##"OK ((airline-light ((airline-normal-outer ((t (:foreground "#ffffff" :background "#005fff")))) (airline-normal-inner ((t (:foreground "#000087" :background "#00dfff")))) (airline-normal-center ((t (:foreground "#005fff" :background "#afffff")))) (airline-insert-outer ((t (:foreground "#ffffff" :background "#00875f")))) (airline-visual-outer ((t (:foreground "#ffffff" :background "#ff5f00")))) (airline-replace-outer ((t (:foreground "#005f00" :background "#ff0000")))) (airline-inactive3 ((t (:foreground "#a8a8a8" :background "#ffffff")))) (mode-line ((t (:foreground "#005fff" :background "#afffff" . #1=(:box nil :underline nil :overline nil))))) (mode-line-inactive ((t (:foreground "#666666" :background "#b2b2b2" . #2=(:box nil :underline nil :overline nil))))))) (airline-dark ((airline-normal-outer ((t (:foreground "#00005f" :background "#dfff00")))) (airline-normal-inner ((t (:foreground "#ffffff" :background "#444444")))) (airline-normal-center ((t (:foreground "#9cffd3" :background "#202020")))) (airline-insert-outer ((t (:foreground "#00005f" :background "#00dfff")))) (airline-visual-outer ((t (:foreground "#000000" :background "#ffaf00")))) (airline-replace-outer ((t (:foreground "#ffffff" :background "#af0000")))) (airline-inactive3 ((t (:foreground "#4e4e4e" :background "#262626")))) (mode-line ((t (:foreground "#9cffd3" :background "#202020" . #1#)))) (mode-line-inactive ((t (:foreground "#4e4e4e" :background "#1c1c1c" . #2#)))))) (airline-doom-one ((airline-normal-outer ((t (:foreground "#1B2229" :background "#51afef")))) (airline-normal-inner ((t (:foreground "#bbc2cf" :background "#21242b")))) (airline-normal-center ((t (:foreground "#bbc2cf" :background "#21242b")))) (airline-insert-outer ((t (:foreground "#1B2229" :background "#98be65")))) (airline-visual-outer ((t (:foreground "#1B2229" :background "#4db5bd")))) (airline-replace-outer ((t (:foreground "#1B2229" :background "#ff6c6b")))) (airline-inactive3 ((t (:foreground "#5B6268" :background "#23272e")))) (mode-line ((t (:foreground "#bbc2cf" :background "#21242b" . #1#)))) (mode-line-inactive ((t (:foreground "#5B6268" :background "#23272e" . #2#)))))) (airline-transparent ((airline-normal-outer ((t (:foreground "#8d96a1" :background "NONE")))) (airline-normal-inner ((t (:foreground "#3f4b59" :background "NONE")))) (airline-normal-center ((t (:foreground "#3f4b59" :background "NONE")))) (airline-insert-outer ((t (:foreground "#1d1f21" :background "#BBE67E")))) (airline-visual-outer ((t (:foreground "#1d1f21" :background "#F07178")))) (airline-replace-outer ((t (:foreground "#1d1f21" :background "#D4BFFF")))) (airline-inactive3 ((t (:foreground "#3f4b59" :background "NONE")))) (mode-line ((t (:foreground "#3f4b59" :background "NONE" . #1#)))) (mode-line-inactive ((t (:foreground "#1d1f21" :background "NONE" . #2#)))))))"##
        ]],
    )
}

fn airline_themes_sparse_upstream_palettes_apply_normal_state_fallbacks_exactly() -> ParityBatchCase
{
    ParityBatchCase::value(
        "airline_themes_sparse_upstream_palettes_apply_normal_state_fallbacks_exactly",
        r##"(let ((themes
                '(airline-angr
                  airline-blood_red
                  airline-bubblegum
                  airline-desertink
                  airline-owo
                  airline-powerlineish
                  airline-ravenpower
                  airline-serene
                  airline-simple
                  airline-soda
                  airline-violet))
               (faces
                '(airline-normal-inner
                  airline-normal-center
                  airline-visual-inner
                  airline-visual-center
                  airline-replace-inner
                  airline-replace-center)))
         (mapcar
          (lambda (theme)
            (load-theme theme t t)
            (list
             theme
             (mapcar
              (lambda (face)
                (let* ((setting
                        (seq-find
                         (lambda (entry)
                           (and
                            (eq (car entry) 'theme-face)
                            (eq (nth 1 entry) face)))
                         (get theme 'theme-settings)))
                       (attributes
                        (cadr (car (nth 3 setting)))))
                  (list
                   face
                   (plist-get attributes :foreground)
                   (plist-get attributes :background))))
              faces)))
          themes))"##,
        expect![[
            r##"OK ((airline-angr ((airline-normal-inner "#b2b2b2" "#3a3a3a") (airline-normal-center "#b2b2b2" "#444444") (airline-visual-inner "#b2b2b2" "#3a3a3a") (airline-visual-center "#d7afd7" "#444444") (airline-replace-inner "#b2b2b2" "#3a3a3a") (airline-replace-center "#d78787" "#444444"))) (airline-blood_red ((airline-normal-inner "#ffffff" "#8b0000") (airline-normal-center "#c6c6c6" "#3a3a3a") (airline-visual-inner "#ffffff" "#8b0000") (airline-visual-center "#c6c6c6" "#3a3a3a") (airline-replace-inner "#ffffff" "#8b0000") (airline-replace-center "#c6c6c6" "#3a3a3a"))) (airline-bubblegum ((airline-normal-inner "#b2b2b2" "#3a3a3a") (airline-normal-center "#afd787" "#444444") (airline-visual-inner "#b2b2b2" "#3a3a3a") (airline-visual-center "#d7afd7" "#444444") (airline-replace-inner "#b2b2b2" "#3a3a3a") (airline-replace-center "#d78787" "#444444"))) (airline-desertink ((airline-normal-inner "#bbbbbb" "#444444") (airline-normal-center "#ffffff" "#303030") (airline-visual-inner "#bbbbbb" "#444444") (airline-visual-center "#ffffff" "#303030") (airline-replace-inner "#bbbbbb" "#444444") (airline-replace-center "#ffffff" "#303030"))) (airline-owo ((airline-normal-inner "#b2b2b2" "#3a3a3a") (airline-normal-center "#87d7ff" "#444444") (airline-visual-inner "#b2b2b2" "#3a3a3a") (airline-visual-center "#87d787" "#444444") (airline-replace-inner "#b2b2b2" "#3a3a3a") (airline-replace-center "#8787ff" "#444444"))) (airline-powerlineish ((airline-normal-inner "#9e9e9e" "#303030") (airline-normal-center "#ffffff" "#121212") (airline-visual-inner "#9e9e9e" "#303030") (airline-visual-center "#ffffff" "#121212") (airline-replace-inner "#9e9e9e" "#303030") (airline-replace-center "#ffffff" "#121212"))) (airline-ravenpower ((airline-normal-inner "#9e9e9e" "#303030") (airline-normal-center "#c8c8c8" "#2e2e2e") (airline-visual-inner "#9e9e9e" "#303030") (airline-visual-center "#c8c8c8" "#2e2e2e") (airline-replace-inner "#9e9e9e" "#303030") (airline-replace-center "#c8c8c8" "#2e2e2e"))) (airline-serene ((airline-normal-inner "#ff5f00" "#080808") (airline-normal-center "#767676" "#080808") (airline-visual-inner "#ff5f00" "#080808") (airline-visual-center "#767676" "#080808") (airline-replace-inner "#ff5f00" "#080808") (airline-replace-center "#767676" "#080808"))) (airline-simple ((airline-normal-inner "#ff5f00" "#1c1c1c") (airline-normal-center "#767676" "#080808") (airline-visual-inner "#ff5f00" "#1c1c1c") (airline-visual-center "#767676" "#080808") (airline-replace-inner "#ff5f00" "#1c1c1c") (airline-replace-center "#767676" "#080808"))) (airline-soda ((airline-normal-inner "#ffffff" "#875f87") (airline-normal-center "#ffffff" "#5f0087") (airline-visual-inner "#767676" "#ffd75f") (airline-visual-center "#767676" "#ffaf5f") (airline-replace-inner "#ffffff" "#875f87") (airline-replace-center "#ffffff" "#5f0087"))) (airline-violet ((airline-normal-inner "#d75fd7" "#4e4e4e") (airline-normal-center "#c6c6c6" "#3a3a3a") (airline-visual-inner "#d75fd7" "#4e4e4e") (airline-visual-center "#c6c6c6" "#3a3a3a") (airline-replace-inner "#d75fd7" "#4e4e4e") (airline-replace-center "#c6c6c6" "#3a3a3a"))))"##
        ]],
    )
}

fn airline_themes_base16_gui_palette_is_derived_from_real_current_face_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_base16_gui_palette_is_derived_from_real_current_face_colors",
        r##"(progn
         (fset 'display-graphic-p (lambda (&optional _display) t))
         (fset
          'face-foreground
          (lambda (face &optional _frame _inherit)
            (alist-get
             face
             '((link . "#3366ff")
               (font-lock-doc-face . "#778899")
               (success . "#22aa66")
               (warning . "#ffaa22")
               (error . "#ee3344")
               (link-visited . "#8855cc")
               (mode-line-emphasis . "#ddeeff"))
             "#foreground-fallback")))
         (fset
          'face-background
          (lambda (face &optional _frame _inherit)
            (alist-get
             face
             '((highlight . "#101820")
               (fringe . "#202c3a")
               (default . "#05080c"))
             "#background-fallback")))
         (load-theme 'airline-base16-gui-dark t t)
         (let ((settings
                (get 'airline-base16-gui-dark
                     'theme-settings)))
           (mapcar
            (lambda (face)
              (seq-find
               (lambda (entry)
                 (and
                  (eq (car entry) 'theme-face)
                  (eq (nth 1 entry) face)))
               settings))
            '(airline-normal-outer
              airline-normal-inner
              airline-normal-center
              airline-insert-outer
              airline-visual-outer
              airline-replace-outer
              airline-emacs-outer
              powerline-inactive1
              powerline-inactive2
              airline-inactive3))))"##,
        expect![[
            r##"OK ((theme-face airline-normal-outer airline-base16-gui-dark ((t (:foreground "#101820" :background "#3366ff")))) (theme-face airline-normal-inner airline-base16-gui-dark ((t (:foreground "#778899" :background "#202c3a")))) (theme-face airline-normal-center airline-base16-gui-dark ((t (:foreground "#778899" :background "#101820")))) (theme-face airline-insert-outer airline-base16-gui-dark ((t (:foreground "#101820" :background "#22aa66")))) (theme-face airline-visual-outer airline-base16-gui-dark ((t (:foreground "#101820" :background "#ffaa22")))) (theme-face airline-replace-outer airline-base16-gui-dark ((t (:foreground "#101820" :background "#ee3344")))) (theme-face airline-emacs-outer airline-base16-gui-dark ((t (:foreground "#101820" :background "#8855cc")))) (theme-face powerline-inactive1 airline-base16-gui-dark ((t (:foreground "#778899" :background "#foreground-fallback")))) (theme-face powerline-inactive2 airline-base16-gui-dark ((t (:foreground "#778899" :background "#foreground-fallback")))) (theme-face airline-inactive3 airline-base16-gui-dark ((t (:foreground "#778899" :background "#foreground-fallback")))))"##
        ]],
    )
}

fn airline_themes_base16_shell_palette_obeys_terminal_and_graphical_workflows() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_base16_shell_palette_obeys_terminal_and_graphical_workflows",
        r##"(let (terminal graphical)
         (fset 'display-graphic-p
               (lambda (&optional _display) nil))
         (load-theme 'airline-base16-shell-dark t t)
         (setq terminal
               (list
                (custom-theme-p
                 'airline-base16-shell-dark)
                (length
                 (get 'airline-base16-shell-dark
                      'theme-settings))
                (copy-tree
                 (get 'airline-base16-shell-dark
                      'theme-settings))))
         (put 'airline-base16-shell-dark
              'theme-settings nil)
         (setq custom-known-themes
               (delq 'airline-base16-shell-dark
                     custom-known-themes))
         (fset 'display-graphic-p
               (lambda (&optional _display) t))
         (load-theme 'airline-base16-shell-dark t t)
         (setq graphical
               (list
                (custom-theme-p
                 'airline-base16-shell-dark)
                (length
                 (get 'airline-base16-shell-dark
                      'theme-settings))
                (memq 'airline-base16-shell-dark
                      custom-known-themes)))
         (list
          (list (car terminal) (cadr terminal))
          (mapcar
           (lambda (face)
             (seq-find
              (lambda (entry)
                (and
                 (eq (car entry) 'theme-face)
                 (eq (nth 1 entry) face)))
              (nth 2 terminal)))
           '(airline-normal-outer
             airline-insert-outer
             airline-visual-outer
             airline-inactive3))
          graphical))"##,
        expect![[
            r#"OK (((airline-base16-shell-dark . #1=(user changed)) 31) ((theme-face airline-normal-outer airline-base16-shell-dark ((t (:foreground "color-18" :background "blue")))) (theme-face airline-insert-outer airline-base16-shell-dark ((t (:foreground "color-18" :background "green")))) (theme-face airline-visual-outer airline-base16-shell-dark ((t (:foreground "color-18" :background "color-16")))) (theme-face airline-inactive3 airline-base16-shell-dark ((t (:foreground "color-19" :background "color-18"))))) (#2=(airline-base16-shell-dark . #1#) 0 #2#))"#
        ]],
    )
    .fresh_process()
}

fn airline_themes_helm_face_customization_controls_the_complete_palette_extension()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_helm_face_customization_controls_the_complete_palette_extension",
        r##"(let (enabled disabled)
         (setq airline-helm-colors t)
         (load-theme 'airline-catppuccin_mocha t t)
         (setq enabled
               (seq-filter
                (lambda (entry)
                  (memq
                   (nth 1 entry)
                   '(helm-header helm-selection
                     helm-source-header
                     helm-candidate-number
                     helm-selection-line)))
                (get 'airline-catppuccin_mocha
                     'theme-settings)))
         (put 'airline-catppuccin_mocha
              'theme-settings nil)
         (setq custom-known-themes
               (delq 'airline-catppuccin_mocha
                     custom-known-themes)
               airline-helm-colors nil)
         (load-theme 'airline-catppuccin_mocha t t)
         (setq disabled
               (seq-filter
                (lambda (entry)
                  (memq
                   (nth 1 entry)
                   '(helm-header helm-selection
                     helm-source-header
                     helm-candidate-number
                     helm-selection-line)))
                (get 'airline-catppuccin_mocha
                     'theme-settings)))
         (list
          (length enabled)
          (reverse enabled)
          (length disabled)
          disabled))"##,
        expect![[
            r##"OK (5 ((theme-face helm-header airline-catppuccin_mocha ((t (:foreground "#94E2D5" :background "#181825" :bold t)))) (theme-face helm-selection airline-catppuccin_mocha ((t (:foreground "#181825" :background "#94E2D5" :bold t)))) (theme-face helm-source-header airline-catppuccin_mocha ((t (:foreground "#CDD6F4" :background "#1E1E2E" :bold t)))) (theme-face helm-candidate-number airline-catppuccin_mocha ((t (:foreground "#89B4FA" :background "#45475A" :bold t)))) (theme-face helm-selection-line airline-catppuccin_mocha ((t (:foreground "#CDD6F4" :background "#1E1E2E" :bold t))))) 0 nil)"##
        ]],
    )
}

pub(super) fn palettes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        airline_themes_doom_one_preserves_every_state_and_inactive_face_spec(),
        airline_themes_light_dark_transparent_palettes_resolve_distinct_practical_colors(),
        airline_themes_sparse_upstream_palettes_apply_normal_state_fallbacks_exactly(),
        airline_themes_base16_gui_palette_is_derived_from_real_current_face_colors(),
        airline_themes_base16_shell_palette_obeys_terminal_and_graphical_workflows(),
        airline_themes_helm_face_customization_controls_the_complete_palette_extension(),
    ]
}
