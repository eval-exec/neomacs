use expect_test::expect;

use super::ParityBatchCase;

fn atom_dark_theme_enable_applies_representative_faces_then_disable_restores_baseline()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_enable_applies_representative_faces_then_disable_restores_baseline",
        r##"(let ((base 'atom-dark-parity-deterministic-base)
               (requests
                '((default :foreground)
                  (default :background)
                  (default :weight)
                  (cursor :background)
                  (region :background)
                  (mode-line :foreground)
                  (mode-line :background)
                  (font-lock-keyword-face :foreground)
                  (font-lock-string-face :foreground)
                  (font-lock-warning-face :foreground)
                  (font-lock-warning-face :weight)
                  (trailing-whitespace :background)))
               before
               during
               after)
         (custom-declare-theme
          base
          "Deterministic baseline for atom-dark parity.")
         (custom-theme-set-faces
          base
          '(default
             ((t
               (:foreground "#eeeeee"
                :background "#101010"
                :weight light))))
          '(cursor
             ((t
               (:background "#112233"))))
          '(region
             ((t
               (:background "#223344"))))
          '(mode-line
             ((t
               (:foreground "#334455"
                :background "#445566"))))
          '(font-lock-keyword-face
             ((t
               (:foreground "#556677"))))
          '(font-lock-string-face
             ((t
               (:foreground "#667788"))))
          '(font-lock-warning-face
             ((t
               (:foreground "#778899"
                :weight normal))))
          '(trailing-whitespace
             ((t
               (:background "#8899aa")))))
         (cl-labels
             ((observe ()
                (list
                 (copy-sequence custom-enabled-themes)
                 (custom-theme-enabled-p 'atom-dark)
                 (mapcar
                  (lambda (request)
                    (list
                     (car request)
                     (cadr request)
                     (face-attribute
                      (car request)
                      (cadr request)
                      nil
                      t)))
                  requests))))
           (unwind-protect
               (progn
                 (enable-theme base)
                 (setq before (observe))
                 (enable-theme 'atom-dark)
                 (setq during (observe))
                 (disable-theme 'atom-dark)
                 (setq after (observe)))
             (when
                 (custom-theme-enabled-p 'atom-dark)
               (disable-theme 'atom-dark))
             (when
                 (custom-theme-enabled-p base)
               (disable-theme base)))
           (list
            before
            during
            after
            (equal before after))))"##,
        expect![[
            r##"OK (((atom-dark-parity-deterministic-base) nil ((default :foreground "#eeeeee") (default :background "#101010") (default :weight light) (cursor :background "#112233") (region :background "#223344") (mode-line :foreground "#334455") (mode-line :background "#445566") (font-lock-keyword-face :foreground "#556677") (font-lock-string-face :foreground "#667788") (font-lock-warning-face :foreground "#778899") (font-lock-warning-face :weight normal) (trailing-whitespace :background "#8899aa"))) ((atom-dark atom-dark-parity-deterministic-base) (atom-dark atom-dark-parity-deterministic-base) ((default :foreground "#c5c8c6") (default :background "#1d1f21") (default :weight normal) (cursor :background "white") (region :background "grey70") (mode-line :foreground "#96CBFE") (mode-line :background "grey10") (font-lock-keyword-face :foreground "#96CBFE") (font-lock-string-face :foreground "#8AE234") (font-lock-warning-face :foreground "#ff982d") (font-lock-warning-face :weight bold) (trailing-whitespace :background "#562d56"))) ((atom-dark-parity-deterministic-base) nil ((default :foreground "#eeeeee") (default :background "#101010") (default :weight light) (cursor :background "#112233") (region :background "#223344") (mode-line :foreground "#334455") (mode-line :background "#445566") (font-lock-keyword-face :foreground "#556677") (font-lock-string-face :foreground "#667788") (font-lock-warning-face :foreground "#778899") (font-lock-warning-face :weight normal) (trailing-whitespace :background "#8899aa"))) t)"##
        ]],
    )
}

fn atom_dark_theme_enable_disable_enable_cycle_is_stable_and_does_not_duplicate_enabled_entry()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_enable_disable_enable_cycle_is_stable_and_does_not_duplicate_enabled_entry",
        r##"(let (first repeated disabled second)
         (cl-labels
             ((observe ()
                (list
                 (copy-sequence custom-enabled-themes)
                 (let ((count 0))
                   (dolist
                       (theme custom-enabled-themes count)
                     (when
                         (eq theme 'atom-dark)
                       (setq count (1+ count)))))
                 (face-attribute
                  'font-lock-keyword-face
                  :foreground nil t)
                 (face-attribute
                  'font-lock-warning-face
                  :foreground nil t)
                 (face-attribute
                  'font-lock-warning-face
                  :weight nil t))))
           (unwind-protect
               (progn
                 (enable-theme 'atom-dark)
                 (setq first (observe))
                 (enable-theme 'atom-dark)
                 (setq repeated (observe))
                 (disable-theme 'atom-dark)
                 (setq disabled
                       (list
                        (copy-sequence custom-enabled-themes)
                        (custom-theme-enabled-p 'atom-dark)))
                 (enable-theme 'atom-dark)
                 (setq second (observe)))
             (when
                 (custom-theme-enabled-p 'atom-dark)
               (disable-theme 'atom-dark)))
           (list
            first
            repeated
            disabled
            second
            (equal first repeated)
            (equal first second))))"##,
        expect![[
            r##"OK (((atom-dark) 1 "#96CBFE" "#ff982d" bold) ((atom-dark) 1 "#96CBFE" "#ff982d" bold) (nil nil) ((atom-dark) 1 "#96CBFE" "#ff982d" bold) t t)"##
        ]],
    )
}

fn atom_dark_theme_load_theme_from_source_install_is_repeatable_without_setting_growth()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_load_theme_from_source_install_is_repeatable_without_setting_growth",
        r##"(let ((before
                (length
                 (get 'atom-dark 'theme-settings)))
               first
               second)
         (unwind-protect
             (progn
               (load-theme 'atom-dark t)
               (setq first
                     (list
                      (copy-sequence custom-enabled-themes)
                      (length
                       (get 'atom-dark 'theme-settings))
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'default :background nil t)))
               (load-theme 'atom-dark t)
               (setq second
                     (list
                      (copy-sequence custom-enabled-themes)
                      (length
                       (get 'atom-dark 'theme-settings))
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'default :background nil t))))
           (when
               (custom-theme-enabled-p 'atom-dark)
             (disable-theme 'atom-dark)))
         (list before first second
               (equal first second)
               custom-enabled-themes))"##,
        expect![[
            r##"OK (98 ((atom-dark) 98 "#c5c8c6" "#1d1f21") ((atom-dark) 98 "#c5c8c6" "#1d1f21") t nil)"##
        ]],
    )
}

fn atom_dark_theme_optional_integration_faces_defined_before_enable_receive_exact_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_optional_integration_faces_defined_before_enable_receive_exact_values",
        r##"(let ((faces
                '(company-preview-common
                  company-scrollbar-bg
                  company-tooltip
                  company-tooltip-common-selection
                  diff-hl-delete
                  guide-key/key-face
                  markdown-header-delimiter-face
                  js2-jsdoc-tag
                  minimap-active-region-background
                  realgud-overlay-arrow2
                  speedbar-selected-face
                  whitespace-space-after-tab)))
         (dolist (face faces)
           (face-spec-set
            face
            '((t
               (:foreground "fixture-fg"
                :background "fixture-bg"
                :weight normal
                :underline nil)))
            'face-defface-spec))
         (unwind-protect
             (progn
               (enable-theme 'atom-dark)
               (mapcar
                (lambda (face)
                  (cons
                   face
                   (atom-dark-test-face-attributes
                    face
                    '(:foreground
                      :background
                      :weight
                      :underline
                      :inherit))))
                faces))
           (when
               (custom-theme-enabled-p 'atom-dark)
             (disable-theme 'atom-dark))))"##,
        expect![[
            r##"OK ((company-preview-common (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight unspecified unspecified) (:underline "#96CBFE" "#96CBFE") (:inherit company-preview company-preview)) (company-scrollbar-bg (:foreground unspecified "#1d1f21") (:background "dim grey" "dim grey") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit company-tooltip company-tooltip)) (company-tooltip (:foreground "#1d1f21" "#1d1f21") (:background "#c5c8c6" "#c5c8c6") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (company-tooltip-common-selection (:foreground unspecified unspecified) (:background "#96CBFE" "#96CBFE") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit company-tooltip-selection company-tooltip-selection)) (diff-hl-delete (:foreground "#CC6666" "#CC6666") (:background "#7a3d3d" "#7a3d3d") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (guide-key/key-face (:foreground unspecified "#ff982d") (:background unspecified unspecified) (:weight unspecified bold) (:underline unspecified unspecified) (:inherit #1=(font-lock-warning-face) #1#)) (markdown-header-delimiter-face (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit #2=(markdown-header-face) #2#)) (js2-jsdoc-tag (:foreground unspecified "#8AE234") (:background unspecified unspecified) (:weight bold bold) (:underline unspecified unspecified) (:inherit #3=(font-lock-doc-face) #3#)) (minimap-active-region-background (:foreground unspecified unspecified) (:background unspecified "#444") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit #4=(highlight) #4#)) (realgud-overlay-arrow2 (:foreground "#5FAF44" "#5FAF44") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (speedbar-selected-face (:foreground "#FFFFFF" "#FFFFFF") (:background "#4182C4" "#4182C4") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (whitespace-space-after-tab (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit #5=(whitespace-empty) #5#)))"##
        ]],
    )
}

fn atom_dark_theme_enabled_specs_apply_when_optional_faces_are_defined_after_enable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_enabled_specs_apply_when_optional_faces_are_defined_after_enable",
        r##"(let ((faces
                '(company-preview-search
                  company-tooltip-selection
                  diff-hl-insert
                  flx-highlight-face
                  js2-function-param
                  markdown-header-rule-face
                  powerline-active2
                  speedbar-directory-face
                  whitespace-trailing)))
         (unwind-protect
             (progn
               (enable-theme 'atom-dark)
               (dolist (face faces)
                 (face-spec-set
                  face
                  '((t
                     (:foreground "late-fg"
                      :background "late-bg"
                      :weight normal
                      :underline nil)))
                  'face-defface-spec))
               (mapcar
                (lambda (face)
                  (cons
                   face
                   (atom-dark-test-face-attributes
                    face
                    '(:foreground
                      :background
                      :weight
                      :underline
                      :inherit))))
                faces))
           (when
               (custom-theme-enabled-p 'atom-dark)
             (disable-theme 'atom-dark))))"##,
        expect![[
            r##"OK ((company-preview-search (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit company-preview company-preview)) (company-tooltip-selection (:foreground unspecified unspecified) (:background "#96CBFE" "#96CBFE") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit company-tooltip company-tooltip)) (diff-hl-insert (:foreground "#A8FF60" "#A8FF60") (:background "#547f30" "#547f30") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (flx-highlight-face (:foreground unspecified "#96CBFE") (:background unspecified unspecified) (:weight bold bold) (:underline unspecified t) (:inherit #1=(link) #1#)) (js2-function-param (:foreground "#C6C5FE" "#C6C5FE") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (markdown-header-rule-face (:foreground unspecified "#7C7C7C") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit #2=(font-lock-comment-face) #2#)) (powerline-active2 (:foreground unspecified unspecified) (:background "grey10" "grey10") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (speedbar-directory-face (:foreground unspecified "#96CBFE") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit #3=(font-lock-keyword-face) #3#)) (whitespace-trailing (:foreground unspecified "#FD5FF1") (:background unspecified "#562d56") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit #4=(trailing-whitespace) #4#)))"##
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_stacks_over_an_existing_theme_and_reveals_it_after_disable() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_stacks_over_an_existing_theme_and_reveals_it_after_disable",
        r##"(let ((base 'atom-dark-parity-base)
               during
               after)
         (custom-declare-theme base "Atom dark parity base.")
         (custom-theme-set-faces
          base
          '(default
             ((t
               (:foreground "base-fg"
                :background "base-bg"))))
          '(font-lock-keyword-face
             ((t
               (:foreground "base-keyword"))))
          '(mode-line
             ((t
               (:foreground "base-modeline-fg"
                :background "base-modeline-bg")))))
         (unwind-protect
             (progn
               (enable-theme base)
               (enable-theme 'atom-dark)
               (setq during
                     (list
                      (copy-sequence custom-enabled-themes)
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'default :background nil t)
                      (face-attribute
                       'font-lock-keyword-face
                       :foreground nil t)
                      (face-attribute
                       'mode-line :foreground nil t)
                      (face-attribute
                       'mode-line :background nil t)))
               (disable-theme 'atom-dark)
               (setq after
                     (list
                      (copy-sequence custom-enabled-themes)
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'default :background nil t)
                      (face-attribute
                       'font-lock-keyword-face
                       :foreground nil t)
                      (face-attribute
                       'mode-line :foreground nil t)
                      (face-attribute
                       'mode-line :background nil t))))
           (when
               (custom-theme-enabled-p 'atom-dark)
             (disable-theme 'atom-dark))
           (when
               (custom-theme-enabled-p base)
             (disable-theme base)))
         (list during after))"##,
        expect![[
            r##"OK (((atom-dark atom-dark-parity-base) "#c5c8c6" "#1d1f21" "#96CBFE" "#96CBFE" "grey10") ((atom-dark-parity-base) "base-fg" "base-bg" "base-keyword" "base-modeline-fg" "base-modeline-bg"))"##
        ]],
    )
}

fn atom_dark_theme_later_theme_wins_and_disabling_it_restores_atom_dark() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_later_theme_wins_and_disabling_it_restores_atom_dark",
        r##"(let ((overlay 'atom-dark-parity-overlay)
               atom
               overlaid
               restored)
         (custom-declare-theme overlay "Atom dark parity overlay.")
         (custom-theme-set-faces
          overlay
          '(default
             ((t
               (:foreground "overlay-fg"
                :background "overlay-bg"))))
          '(font-lock-string-face
             ((t
               (:foreground "overlay-string")))))
         (unwind-protect
             (progn
               (enable-theme 'atom-dark)
               (setq atom
                     (list
                      (copy-sequence custom-enabled-themes)
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'font-lock-string-face
                       :foreground nil t)))
               (enable-theme overlay)
               (setq overlaid
                     (list
                      (copy-sequence custom-enabled-themes)
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'font-lock-string-face
                       :foreground nil t)))
               (disable-theme overlay)
               (setq restored
                     (list
                      (copy-sequence custom-enabled-themes)
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'font-lock-string-face
                       :foreground nil t))))
           (when
               (custom-theme-enabled-p overlay)
             (disable-theme overlay))
           (when
               (custom-theme-enabled-p 'atom-dark)
             (disable-theme 'atom-dark)))
         (list atom overlaid restored
               (equal atom restored)))"##,
        expect![[
            r##"OK (((atom-dark) "#c5c8c6" "#8AE234") ((atom-dark-parity-overlay atom-dark) "overlay-fg" "overlay-string") ((atom-dark) "#c5c8c6" "#8AE234") t)"##
        ]],
    )
}

fn atom_dark_theme_font_families_and_inheritance_resolve_in_practical_rendering() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_dark_theme_font_families_and_inheritance_resolve_in_practical_rendering",
        r##"(let ((faces
                '(button
                  link
                  link-visited
                  fixed-pitch
                  variable-pitch
                  tooltip
                  font-lock-comment-delimiter-face
                  font-lock-doc-face
                  font-lock-variable-name-face
                  dired-directory
                  whitespace-tab)))
         (dolist
             (face
              '(dired-directory whitespace-tab))
           (face-spec-set
            face
            '((t (:foreground "fixture")))
            'face-defface-spec))
         (unwind-protect
             (progn
               (enable-theme 'atom-dark)
               (mapcar
                (lambda (face)
                  (cons
                   face
                   (atom-dark-test-face-attributes
                    face
                    '(:family
                      :foreground
                      :background
                      :underline
                      :inherit))))
                faces))
           (when
               (custom-theme-enabled-p 'atom-dark)
             (disable-theme 'atom-dark))))"##,
        expect![[
            r##"OK ((button (:family unspecified unspecified) (:foreground unspecified "#96CBFE") (:background unspecified unspecified) (:underline unspecified t) (:inherit #1=(link) #1#)) (link (:family unspecified unspecified) (:foreground unspecified "#96CBFE") (:background unspecified unspecified) (:underline t t) (:inherit font-lock-keyword-face font-lock-keyword-face)) (link-visited (:family unspecified unspecified) (:foreground unspecified "#96CBFE") (:background unspecified unspecified) (:underline unspecified t) (:inherit #2=(link) #2#)) (fixed-pitch (:family "Monospace" "Monospace") (:foreground unspecified unspecified) (:background unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (variable-pitch (:family "Sans Serif" "Sans Serif") (:foreground unspecified unspecified) (:background unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (tooltip (:family unspecified "Sans Serif") (:foreground "#333" "#333") (:background "#fff" "#fff") (:underline unspecified unspecified) (:inherit variable-pitch variable-pitch)) (font-lock-comment-delimiter-face (:family unspecified unspecified) (:foreground unspecified "#7C7C7C") (:background unspecified unspecified) (:underline unspecified unspecified) (:inherit #3=(font-lock-comment-face) #3#)) (font-lock-doc-face (:family unspecified unspecified) (:foreground unspecified "#8AE234") (:background unspecified unspecified) (:underline unspecified unspecified) (:inherit #4=(font-lock-string-face) #4#)) (font-lock-variable-name-face (:family unspecified "default") (:foreground unspecified "#c5c8c6") (:background unspecified "#1d1f21") (:underline unspecified nil) (:inherit #5=(default) #5#)) (dired-directory (:family unspecified unspecified) (:foreground unspecified "#96CBFE") (:background unspecified unspecified) (:underline unspecified unspecified) (:inherit #6=(font-lock-keyword-face) #6#)) (whitespace-tab (:family unspecified unspecified) (:foreground unspecified unspecified) (:background unspecified unspecified) (:underline unspecified unspecified) (:inherit #7=(whitespace-empty) #7#)))"##
        ]],
    )
}

fn atom_dark_theme_malformed_lifecycle_requests_signal_without_corrupting_registered_theme()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_malformed_lifecycle_requests_signal_without_corrupting_registered_theme",
        r##"(let ((settings-before
                (copy-tree
                 (get 'atom-dark 'theme-settings))))
         (list
          (atom-dark-test-error
           (lambda ()
             (enable-theme 'atom-dark-missing)))
          (atom-dark-test-error
           (lambda ()
             (disable-theme 'atom-dark-missing)))
          (atom-dark-test-error
           (lambda ()
             (load-theme
              'atom-dark-missing
              t)))
          (custom-theme-p 'atom-dark)
          (custom-theme-enabled-p 'atom-dark)
          (equal
           settings-before
           (get 'atom-dark 'theme-settings))
          (length
           (get 'atom-dark 'theme-settings))))"##,
        expect![[
            r#"OK ((:signal error ("Undefined Custom theme atom-dark-missing")) (:ok nil) (:signal error ("Unable to find theme file for ‘atom-dark-missing’")) (atom-dark user changed) nil t 98)"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_dark_theme_enable_applies_representative_faces_then_disable_restores_baseline(),
        atom_dark_theme_enable_disable_enable_cycle_is_stable_and_does_not_duplicate_enabled_entry(
        ),
        atom_dark_theme_load_theme_from_source_install_is_repeatable_without_setting_growth(),
        atom_dark_theme_optional_integration_faces_defined_before_enable_receive_exact_values(),
        atom_dark_theme_enabled_specs_apply_when_optional_faces_are_defined_after_enable(),
        atom_dark_theme_stacks_over_an_existing_theme_and_reveals_it_after_disable(),
        atom_dark_theme_later_theme_wins_and_disabling_it_restores_atom_dark(),
        atom_dark_theme_font_families_and_inheritance_resolve_in_practical_rendering(),
        atom_dark_theme_malformed_lifecycle_requests_signal_without_corrupting_registered_theme(),
    ]
}
