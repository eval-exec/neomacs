use expect_test::expect;

use super::ParityBatchCase;

fn atom_one_dark_theme_enable_applies_faces_and_values_then_disable_restores_deterministic_base()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_enable_applies_faces_and_values_then_disable_restores_deterministic_base",
        r##"(let ((base 'atom-one-dark-parity-base)
               (old-fci
                (and
                 (boundp 'fci-rule-color)
                 (default-value 'fci-rule-color)))
               (old-tetris
                (and
                 (boundp 'tetris-x-colors)
                 (default-value 'tetris-x-colors)))
               (old-ansi
                (and
                 (boundp 'ansi-color-names-vector)
                 (default-value
                  'ansi-color-names-vector)))
               before
               during
               after)
         (custom-declare-theme
          base
          "Deterministic atom-one-dark parity base.")
         (custom-theme-set-faces
          base
          '(default
             ((t
               (:foreground "#eeeeee"
                :background "#101010"))))
          '(font-lock-keyword-face
             ((t
               (:foreground "#112233"
                :weight bold))))
          '(mode-line
             ((t
               (:foreground "#223344"
                :background "#334455")))))
         (set-default
          'fci-rule-color
          "base-fci")
         (set-default
          'tetris-x-colors
          [[1 2 3]])
         (set-default
          'ansi-color-names-vector
          ["b0" "b1"])
         (cl-labels
             ((observe ()
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
                  'font-lock-keyword-face
                  :weight nil t)
                 (face-attribute
                  'mode-line :foreground nil t)
                 (face-attribute
                  'mode-line :background nil t)
                 (default-value 'fci-rule-color)
                 (default-value 'tetris-x-colors)
                 (default-value
                  'ansi-color-names-vector))))
           (unwind-protect
               (progn
                 (enable-theme base)
                 (setq before (observe))
                 (enable-theme 'atom-one-dark)
                 (setq during (observe))
                 (disable-theme 'atom-one-dark)
                 (setq after (observe)))
             (when
                 (custom-theme-enabled-p
                  'atom-one-dark)
               (disable-theme 'atom-one-dark))
             (when
                 (custom-theme-enabled-p base)
               (disable-theme base))
             (set-default
              'fci-rule-color old-fci)
             (set-default
              'tetris-x-colors old-tetris)
             (set-default
              'ansi-color-names-vector
              old-ansi))
           (list
            before
            during
            after
            (equal before after))))"##,
        expect![[
            r##"OK (((atom-one-dark-parity-base) "#eeeeee" "#101010" "#112233" bold "#223344" "#334455" "base-fci" #1=[[1 2 3]] #2=["b0" "b1"]) ((atom-one-dark atom-one-dark-parity-base) "#ABB2BF" "#282C34" "#C678DD" normal "#9DA5B4" "#21252B" "#3E4451" [[229 192 123] [97 175 239] [209 154 102] [224 108 117] [152 195 121] [198 120 221] [86 182 194]] ["#282C34" "#E06C75" "#98C379" "#E5C07B" "#61AFEF" "#C678DD" "#56B6C2" "#ABB2BF"]) ((atom-one-dark-parity-base) "#eeeeee" "#101010" "#112233" bold "#223344" "#334455" "base-fci" #1# #2#) t)"##
        ]],
    )
}

fn atom_one_dark_theme_enable_disable_enable_is_stable_and_idempotent() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_enable_disable_enable_is_stable_and_idempotent",
        r##"(let (first repeated disabled second)
         (set-default
          'fci-rule-color
          "fixture-rule")
         (cl-labels
             ((observe ()
                (list
                 (copy-sequence custom-enabled-themes)
                 (let ((count 0))
                   (dolist
                       (theme custom-enabled-themes count)
                     (when
                         (eq theme 'atom-one-dark)
                       (setq count
                             (1+ count)))))
                 (face-attribute
                  'default :foreground nil t)
                 (face-attribute
                  'font-lock-keyword-face
                  :foreground nil t)
                 (default-value
                  'fci-rule-color))))
           (unwind-protect
               (progn
                 (enable-theme 'atom-one-dark)
                 (setq first (observe))
                 (enable-theme 'atom-one-dark)
                 (setq repeated (observe))
                 (disable-theme 'atom-one-dark)
                 (setq disabled
                       (list
                        (copy-sequence
                         custom-enabled-themes)
                        (custom-theme-enabled-p
                         'atom-one-dark)))
                 (enable-theme 'atom-one-dark)
                 (setq second (observe)))
             (when
                 (custom-theme-enabled-p
                  'atom-one-dark)
               (disable-theme 'atom-one-dark)))
           (list
            first repeated disabled second
            (equal first repeated)
            (equal first second))))"##,
        expect![[
            r##"OK (((atom-one-dark) 1 "#ABB2BF" "#C678DD" "#3E4451") ((atom-one-dark) 1 "#ABB2BF" "#C678DD" "#3E4451") (nil nil) ((atom-one-dark) 1 "#ABB2BF" "#C678DD" "#3E4451") t t)"##
        ]],
    )
}

fn atom_one_dark_theme_repeated_load_theme_does_not_grow_settings() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_repeated_load_theme_does_not_grow_settings",
        r##"(let ((before
                (length
                 (get
                  'atom-one-dark
                  'theme-settings)))
               first
               second)
         (set-default
          'fci-rule-color
          "fixture-rule")
         (unwind-protect
             (progn
               (load-theme
                'atom-one-dark t)
               (setq first
                     (list
                      (copy-sequence
                       custom-enabled-themes)
                      (length
                       (get
                        'atom-one-dark
                        'theme-settings))
                      (face-attribute
                       'default :foreground nil t)
                      (default-value
                       'fci-rule-color)))
               (load-theme
                'atom-one-dark t)
               (setq second
                     (list
                      (copy-sequence
                       custom-enabled-themes)
                      (length
                       (get
                        'atom-one-dark
                        'theme-settings))
                      (face-attribute
                       'default :foreground nil t)
                      (default-value
                       'fci-rule-color))))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark)))
         (list before first second
               (equal first second)))"##,
        expect![[
            r##"OK (463 ((atom-one-dark) 463 "#ABB2BF" "#3E4451") ((atom-one-dark) 463 "#ABB2BF" "#3E4451") t)"##
        ]],
    )
}

fn atom_one_dark_theme_optional_faces_defined_before_enable_receive_exact_values() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_optional_faces_defined_before_enable_receive_exact_values",
        r##"(let ((faces
                '(company-tooltip
                  flycheck-error
                  helm-source-header
                  ivy-current-match
                  magit-section-heading
                  notmuch-tag-unread
                  rainbow-delimiters-depth-6-face
                  web-mode-html-tag-face
                  tabbar-selected
                  ruler-mode-current-column)))
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
               (enable-theme 'atom-one-dark)
               (mapcar
                (lambda (face)
                  (cons
                   face
                   (atom-one-dark-test-face-attributes
                    face
                    '(:foreground
                      :background
                      :weight
                      :underline
                      :inherit
                      :box))))
                faces))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK ((company-tooltip (:foreground "#ABB2BF" "#ABB2BF") (:background "#121417" "#121417") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (flycheck-error (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight unspecified unspecified) (:underline #1=(:color "#FF6347" :style wave) #1#) (:inherit unspecified unspecified) (:box unspecified unspecified)) (helm-source-header (:foreground "#E5C07B" "#E5C07B") (:background "#282C34" "#282C34") (:weight bold bold) (:underline nil nil) (:inherit unspecified unspecified) (:box #2=(:line-width 6 :color "#282C34") #2#)) (ivy-current-match (:foreground unspecified unspecified) (:background "#3E4451" "#3E4451") (:weight normal normal) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (magit-section-heading (:foreground "#E5C07B" "#E5C07B") (:background unspecified unspecified) (:weight bold bold) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (notmuch-tag-unread (:foreground "#E06C75" "#E06C75") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (rainbow-delimiters-depth-6-face (:foreground "#E5C07B" "#E5C07B") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (web-mode-html-tag-face (:foreground "#E06C75" "#E06C75") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (tabbar-selected (:foreground "fixture-fg" "fixture-fg") (:background "fixture-bg" "fixture-bg") (:weight normal normal) (:underline nil nil) (:inherit unspecified unspecified) (:box unspecified unspecified)) (ruler-mode-current-column (:foreground "#528BFF" "#528BFF") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit ruler-mode-default ruler-mode-default) (:box unspecified unspecified)))"##
        ]],
    )
}

fn atom_one_dark_theme_enabled_specs_apply_to_optional_faces_defined_late() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_enabled_specs_apply_to_optional_faces_defined_late",
        r##"(let ((faces
                '(company-tooltip-selection
                  flymake-warning
                  helm-ff-directory
                  ivy-minibuffer-match-face-3
                  magit-diff-hunk-heading
                  notmuch-search-date
                  rainbow-delimiters-depth-11-face
                  web-mode-error-face
                  solaire-default-face
                  undo-tree-visualizer-current-face)))
         (unwind-protect
             (progn
               (enable-theme 'atom-one-dark)
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
                   (atom-one-dark-test-face-attributes
                    face
                    '(:foreground
                      :background
                      :weight
                      :underline
                      :inherit
                      :box))))
                faces))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK ((company-tooltip-selection (:foreground "#ABB2BF" "#ABB2BF") (:background "#3E4451" "#3E4451") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (flymake-warning (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight unspecified unspecified) (:underline #1=(:color "#E2C08D" :style wave) #1#) (:inherit unspecified unspecified) (:box unspecified unspecified)) (helm-ff-directory (:foreground "#56B6C2" "#56B6C2") (:background "#282C34" "#282C34") (:weight bold bold) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (ivy-minibuffer-match-face-3 (:foreground "#98C379" "#98C379") (:background "#21252B" "#21252B") (:weight semi-bold semi-bold) (:underline unspecified unspecified) (:inherit ivy-minibuffer-match-face-2 ivy-minibuffer-match-face-2) (:box unspecified unspecified)) (magit-diff-hunk-heading (:foreground "#828997" "#828997") (:background "#3E4451" "#3E4451") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (notmuch-search-date (:foreground "#C678DD" "#C678DD") (:background unspecified "#282C34") (:weight unspecified normal) (:underline unspecified nil) (:inherit default default) (:box unspecified nil)) (rainbow-delimiters-depth-11-face (:foreground "#C678DD" "#C678DD") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (web-mode-error-face (:foreground "#E06C75" "#E06C75") (:background "#21252B" "#21252B") (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)) (solaire-default-face (:foreground "late-fg" "late-fg") (:background "late-bg" "late-bg") (:weight normal normal) (:underline nil nil) (:inherit unspecified unspecified) (:box unspecified unspecified)) (undo-tree-visualizer-current-face (:foreground "#E06C75" "#E06C75") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified) (:box unspecified unspecified)))"##
        ]],
    )
}

fn atom_one_dark_theme_stacks_over_base_theme_and_reveals_it_after_disable() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_stacks_over_base_theme_and_reveals_it_after_disable",
        r##"(let ((base
                'atom-one-dark-parity-stack)
               during
               after)
         (custom-declare-theme
          base
          "Atom one dark stack fixture.")
         (custom-theme-set-faces
          base
          '(default
             ((t
               (:foreground "base-fg"
                :background "base-bg"))))
          '(font-lock-string-face
             ((t
               (:foreground "base-string")))))
         (custom-theme-set-variables
          base
          '(fci-rule-color
            "base-rule"))
         (set-default
          'fci-rule-color
          "fixture-rule")
         (unwind-protect
             (progn
               (enable-theme base)
               (enable-theme 'atom-one-dark)
               (setq during
                     (list
                      (copy-sequence
                       custom-enabled-themes)
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'font-lock-string-face
                       :foreground nil t)
                      (default-value
                       'fci-rule-color)))
               (disable-theme 'atom-one-dark)
               (setq after
                     (list
                      (copy-sequence
                       custom-enabled-themes)
                      (face-attribute
                       'default :foreground nil t)
                      (face-attribute
                       'font-lock-string-face
                       :foreground nil t)
                      (default-value
                       'fci-rule-color))))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark))
           (when
               (custom-theme-enabled-p base)
             (disable-theme base)))
         (list during after))"##,
        expect![[
            r##"OK (((atom-one-dark atom-one-dark-parity-stack) "#ABB2BF" "#98C379" "#3E4451") ((atom-one-dark-parity-stack) "base-fg" "base-string" "base-rule"))"##
        ]],
    )
}

fn atom_one_dark_theme_duplicate_helm_grep_finish_resolves_later_source_setting() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_duplicate_helm_grep_finish_resolves_later_source_setting",
        r##"(progn
         (face-spec-set
          'helm-grep-finish
          '((t
             (:foreground "fixture")))
          'face-defface-spec)
         (unwind-protect
             (progn
               (enable-theme 'atom-one-dark)
               (list
                (atom-one-dark-test-face-specs
                 'helm-grep-finish)
                (atom-one-dark-test-face-attributes
                 'helm-grep-finish
                 '(:foreground
                   :background
                   :inherit))))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK ((((t (:foreground "#98C379"))) ((t (:foreground "#E06C75")))) ((:foreground "#98C379" "#98C379") (:background unspecified unspecified) (:inherit unspecified unspecified)))"##
        ]],
    )
}

fn atom_one_dark_theme_nested_realgud_arrow_spec_enable_behavior_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_nested_realgud_arrow_spec_enable_behavior_matches",
        r##"(progn
         (dolist
             (face
              '(realgud-overlay-arrow1
                realgud-overlay-arrow2
                realgud-overlay-arrow3))
           (face-spec-set
            face
            '((t
               (:foreground "fixture")))
            'face-defface-spec))
         (let ((result
                (atom-one-dark-test-error
                 (lambda ()
                   (enable-theme
                    'atom-one-dark)))))
           (unwind-protect
               (list
                result
                (custom-theme-enabled-p
                 'atom-one-dark)
                (mapcar
                 (lambda (face)
                   (cons
                    face
                    (atom-one-dark-test-face-attributes
                     face
                     '(:foreground
                       :background
                       :inherit))))
                 '(realgud-overlay-arrow1
                   realgud-overlay-arrow2
                   realgud-overlay-arrow3)))
             (when
                 (custom-theme-enabled-p
                  'atom-one-dark)
               (disable-theme
                'atom-one-dark)))))"##,
        expect![[
            r##"OK ((:ok nil) (atom-one-dark) ((realgud-overlay-arrow1 (:foreground "#98C379" "#98C379") (:background unspecified unspecified) (:inherit unspecified unspecified)) (realgud-overlay-arrow2 (:foreground "fixture" "fixture") (:background unspecified unspecified) (:inherit unspecified unspecified)) (realgud-overlay-arrow3 (:foreground "#D19A66" "#D19A66") (:background unspecified unspecified) (:inherit unspecified unspecified))))"##
        ]],
    )
}

fn atom_one_dark_theme_malformed_lifecycle_requests_preserve_registered_settings() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_malformed_lifecycle_requests_preserve_registered_settings",
        r##"(let ((settings
                (copy-tree
                 (get
                  'atom-one-dark
                  'theme-settings))))
         (list
          (atom-one-dark-test-error
           (lambda ()
             (enable-theme
              'atom-one-dark-missing)))
          (atom-one-dark-test-error
           (lambda ()
             (disable-theme
              'atom-one-dark-missing)))
          (atom-one-dark-test-error
           (lambda ()
             (load-theme
              'atom-one-dark-missing
              t)))
          (custom-theme-p 'atom-one-dark)
          (custom-theme-enabled-p
           'atom-one-dark)
          (equal
           settings
           (get
            'atom-one-dark
            'theme-settings))
          (length settings)))"##,
        expect![[
            r#"OK ((:signal error ("Undefined Custom theme atom-one-dark-missing")) (:ok nil) (:signal error ("Unable to find theme file for ‘atom-one-dark-missing’")) (atom-one-dark user changed) nil t 463)"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_one_dark_theme_enable_applies_faces_and_values_then_disable_restores_deterministic_base(),
        atom_one_dark_theme_enable_disable_enable_is_stable_and_idempotent(),
        atom_one_dark_theme_repeated_load_theme_does_not_grow_settings(),
        atom_one_dark_theme_optional_faces_defined_before_enable_receive_exact_values(),
        atom_one_dark_theme_enabled_specs_apply_to_optional_faces_defined_late(),
        atom_one_dark_theme_stacks_over_base_theme_and_reveals_it_after_disable(),
        atom_one_dark_theme_duplicate_helm_grep_finish_resolves_later_source_setting(),
        atom_one_dark_theme_nested_realgud_arrow_spec_enable_behavior_matches(),
        atom_one_dark_theme_malformed_lifecycle_requests_preserve_registered_settings(),
    ]
}
