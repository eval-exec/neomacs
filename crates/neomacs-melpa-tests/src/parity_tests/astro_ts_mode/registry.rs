use expect_test::expect;

use super::ParityBatchCase;

fn descriptor_records_exact_pin_dependency_and_installed_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "descriptor_records_exact_pin_dependency_and_installed_payload",
        r##"(let* ((desc (cadr (assq 'astro-ts-mode package-alist)))
              (dir (package-desc-dir desc)))
          (list
           (package-version-join (package-desc-version desc))
           (package-desc-reqs desc)
           (package-desc-kind desc)
           (sort
            (mapcar #'file-name-nondirectory
                    (directory-files dir t "^[^.].*"))
            #'string<)))"##,
        expect![[
            r#"OK ("20260417.101" ((emacs (30))) nil ("astro-ts-mode-autoloads.el" "astro-ts-mode-pkg.el" "astro-ts-mode.el" "astro-ts-mode.elc"))"#
        ]],
    )
}

fn installed_source_has_exact_content_hash_feature_and_emacs_30_requirement() -> ParityBatchCase {
    ParityBatchCase::value(
        "installed_source_has_exact_content_hash_feature_and_emacs_30_requirement",
        r##"(let ((source (locate-library "astro-ts-mode"))
              (desc (cadr (assq 'astro-ts-mode package-alist))))
          (list
           (file-name-nondirectory source)
           (with-temp-buffer
             (set-buffer-multibyte nil)
             (insert-file-contents-literally source)
             (secure-hash 'sha256 (current-buffer)))
           (featurep 'astro-ts-mode)
           (package-desc-reqs desc)))"##,
        expect![[
            r#"OK ("astro-ts-mode.el" "830194fce49caf655c31a0036cd01eb97f8e3ca75856c5855d86464da39dde73" t ((emacs (30))))"#
        ]],
    )
}

fn complete_declared_callable_surface_has_exact_arities_and_command_status() -> ParityBatchCase {
    ParityBatchCase::value(
        "complete_declared_callable_surface_has_exact_arities_and_command_status",
        r##"(mapcar
          (lambda (symbol)
            (list symbol
                  (help-function-arglist symbol t)
                  (commandp symbol)
                  (macrop symbol)
                  (documentation symbol)))
          '(astro-ts-mode--prefix-font-lock-features
            astro-ts-mode--treesit-language-at-point
            astro-ts-mode))"##,
        expect![[
            r#"OK ((astro-ts-mode--prefix-font-lock-features (prefix settings) nil nil "Prefix with PREFIX the font lock features in SETTINGS.") (astro-ts-mode--treesit-language-at-point (point) nil nil "Return the language at POINT.") (astro-ts-mode nil t nil "Major mode for editing Astro templates, powered by tree-sitter.\n\nIn addition to any hooks its parent mode ‘html-mode’ might have run,\nthis mode runs the hook ‘astro-ts-mode-hook’, as the final or\npenultimate step during initialization.\n\n"))"#
        ]],
    )
    .fresh_process()
}

fn complete_declared_variable_surface_has_exact_defaults_kinds_groups_and_docs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "complete_declared_variable_surface_has_exact_defaults_kinds_groups_and_docs",
        r##"(mapcar
          (lambda (symbol)
            (list symbol
                  (boundp symbol)
                  (cond
                   ((integerp (symbol-value symbol)) 'integer)
                   ((listp (symbol-value symbol)) 'list)
                   (t (type-of (symbol-value symbol))))
                  (get symbol 'custom-type)
                  (get symbol 'custom-group)
                  (documentation-property symbol
                                          'variable-documentation)))
          '(astro-ts-mode-indent-offset
            astro-ts-mode--indent-rules
            astro-ts-mode--font-lock-settings
            astro-ts-mode--range-settings
            astro-ts-mode-hook
            astro-ts-mode-map
            astro-ts-mode-syntax-table
            astro-ts-mode-abbrev-table))"##,
        expect![[
            r#"OK ((astro-ts-mode-indent-offset t integer integer nil "Number of spaces for each indentation step in ‘astro-ts-mode’.") (astro-ts-mode--indent-rules t list nil nil "Tree-sitter indentation rules for ‘astro-ts-mode’.") (astro-ts-mode--font-lock-settings t list nil nil "Tree-sitter font-lock settings for ‘astro-ts-mode’.") (astro-ts-mode--range-settings t list nil nil nil) (astro-ts-mode-hook t list nil nil "Hook run after entering ‘astro-ts-mode’.\nNo problems result if this variable is not bound.\n‘add-hook’ automatically binds it.  (This is true for all hook variables.)") (astro-ts-mode-map t list nil nil "Keymap for ‘astro-ts-mode’.") (astro-ts-mode-syntax-table t char-table nil nil "Syntax table for ‘astro-ts-mode’.") (astro-ts-mode-abbrev-table t obarray nil nil "Abbrev table for ‘astro-ts-mode’."))"#
        ]],
    )
}

fn indent_registry_covers_astro_css_and_tsx_with_real_anchor_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "indent_registry_covers_astro_css_and_tsx_with_real_anchor_rules",
        r##"(list
          (mapcar #'car astro-ts-mode--indent-rules)
          (mapcar
           (lambda (language)
             (let ((rules
                    (alist-get language
                               astro-ts-mode--indent-rules)))
               (list language
                     (length rules)
                     (seq-take rules 4)
                     (seq-take (reverse rules) 3))))
           '(astro css tsx))
          (secure-hash
           'sha256
           (prin1-to-string astro-ts-mode--indent-rules)))"##,
        expect![[
            r#"OK ((astro css tsx) ((astro 11 (((parent-is "document") column-0 0) ((node-is "frontmatter") column-0 0) ((node-is "/>") parent-bol 0) ((node-is ">") parent-bol 0)) (((parent-is "self_closing_tag") parent-bol astro-ts-mode-indent-offset) ((parent-is "start_tag") parent-bol astro-ts-mode-indent-offset) ((parent-is "style_element") parent-bol astro-ts-mode-indent-offset))) (css 8 (((parent-is "stylesheet") parent-bol 0) ((node-is "}") parent-bol 0) ((node-is ")") parent-bol 0) ((node-is "]") parent-bol 0)) (((match nil "declaration" nil 3) (nth-sibling 2) 0) ((match nil "declaration" nil 0 3) parent-bol css-indent-offset) ((parent-is "arguments") parent-bol css-indent-offset))) (tsx 46 (((parent-is "program") parent-bol 0) ((parent-is "program") column-0 0) ((node-is "}") standalone-parent 0) ((node-is ")") parent-bol 0)) ((no-node parent-bol 0) ((parent-is "jsx_self_closing_element") parent typescript-ts-indent-offset) ((match "/" "jsx_self_closing_element") parent 0)))) "2f0dfaa1a709d09de37767ff80a4623c3ee308bd66d4ac15047ddf1bb599a619")"#
        ]],
    )
}

fn font_lock_registry_has_prefixed_embedded_features_and_native_astro_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_registry_has_prefixed_embedded_features_and_native_astro_rules",
        r##"(let ((features
                (delete-dups
                 (mapcar
                  (lambda (setting) (nth 2 setting))
                  astro-ts-mode--font-lock-settings))))
          (list
           (length astro-ts-mode--font-lock-settings)
           (length features)
           (seq-filter
            (lambda (feature)
              (string-prefix-p "tsx-" (symbol-name feature)))
            features)
           (seq-filter
            (lambda (feature)
              (string-prefix-p "css-" (symbol-name feature)))
            features)
           (seq-filter
            (lambda (feature)
              (string-prefix-p "astro-" (symbol-name feature)))
            features)
           (mapcar
            (lambda (setting)
              (list
               (type-of (nth 0 setting))
               (nth 1 setting)
               (nth 2 setting)
               (nth 3 setting)))
            astro-ts-mode--font-lock-settings)))"##,
        expect![
            "OK (33 33 (tsx-comment tsx-constant tsx-keyword tsx-string tsx-declaration tsx-identifier tsx-property tsx-expression tsx-function tsx-pattern tsx-jsx tsx-number tsx-operator tsx-bracket tsx-delimiter tsx-escape-sequence) (css-comment css-string css-keyword css-variable css-operator css-selector css-property css-function css-constant css-query css-bracket css-error) (astro-comment astro-keyword astro-definition astro-string astro-bracket) ((treesit-compiled-query t tsx-comment nil) (treesit-compiled-query t tsx-constant nil) (treesit-compiled-query t tsx-keyword nil) (treesit-compiled-query t tsx-string nil) (treesit-compiled-query t tsx-declaration t) (treesit-compiled-query t tsx-identifier nil) (treesit-compiled-query t tsx-property nil) (treesit-compiled-query t tsx-expression nil) (treesit-compiled-query t tsx-function nil) (treesit-compiled-query t tsx-pattern nil) (treesit-compiled-query t tsx-jsx nil) (treesit-compiled-query t tsx-number nil) (treesit-compiled-query t tsx-operator nil) (treesit-compiled-query t tsx-bracket nil) (treesit-compiled-query t tsx-delimiter nil) (treesit-compiled-query t tsx-escape-sequence t) (treesit-compiled-query t css-comment nil) (treesit-compiled-query t css-string nil) (treesit-compiled-query t css-keyword nil) (treesit-compiled-query t css-variable nil) (treesit-compiled-query t css-operator nil) (treesit-compiled-query t css-selector nil) (treesit-compiled-query t css-property nil) (treesit-compiled-query t css-function nil) (treesit-compiled-query t css-constant nil) (treesit-compiled-query t css-query nil) (treesit-compiled-query t css-bracket nil) (treesit-compiled-query t css-error nil) (treesit-compiled-query t astro-comment nil) (treesit-compiled-query t astro-keyword nil) (treesit-compiled-query t astro-definition nil) (treesit-compiled-query t astro-string nil) (treesit-compiled-query t astro-bracket nil)))"
        ],
    )
}

fn range_registry_embeds_tsx_and_css_in_every_documented_astro_context() -> ParityBatchCase {
    ParityBatchCase::value(
        "range_registry_embeds_tsx_and_css_in_every_documented_astro_context",
        r##"(list
          (length astro-ts-mode--range-settings)
          (mapcar
           (lambda (setting)
             (list
              (type-of (nth 0 setting))
              (nth 1 setting)
              (nth 2 setting)
              (nth 3 setting)
              (nth 4 setting)))
           astro-ts-mode--range-settings))"##,
        expect![
            "OK (2 ((treesit-compiled-query tsx t nil nil) (treesit-compiled-query css t nil nil)))"
        ],
    )
}

fn mode_metadata_auto_association_and_source_ownership_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_metadata_auto_association_and_source_ownership_are_exact",
        r##"(list
          (get 'astro-ts-mode 'derived-mode-parent)
          (cdr (assoc "\\.astro\\'" auto-mode-alist))
          (get 'astro-ts-mode 'custom-mode-group)
          (mapcar
           (lambda (symbol)
             (list symbol
                   (and (symbol-file symbol 'defun)
                        (file-name-nondirectory
                         (symbol-file symbol 'defun)))
                   (and (symbol-file symbol 'defvar)
                        (file-name-nondirectory
                         (symbol-file symbol 'defvar)))))
           '(astro-ts-mode
             astro-ts-mode--prefix-font-lock-features
             astro-ts-mode-indent-offset
             astro-ts-mode--range-settings
             astro-ts-mode-hook
             astro-ts-mode-map
             astro-ts-mode-syntax-table
             astro-ts-mode-abbrev-table)))"##,
        expect![[
            r#"OK (html-mode astro-ts-mode astro ((astro-ts-mode "astro-ts-mode.el" nil) (astro-ts-mode--prefix-font-lock-features "astro-ts-mode.el" nil) (astro-ts-mode-indent-offset nil "astro-ts-mode.el") (astro-ts-mode--range-settings nil "astro-ts-mode.el") (astro-ts-mode-hook nil "astro-ts-mode.el") (astro-ts-mode-map nil "astro-ts-mode.el") (astro-ts-mode-syntax-table nil "astro-ts-mode.el") (astro-ts-mode-abbrev-table nil "astro-ts-mode.el")))"#
        ]],
    )
}

fn repeated_source_loading_is_idempotent_for_feature_association_and_api() -> ParityBatchCase {
    ParityBatchCase::value(
        "repeated_source_loading_is_idempotent_for_feature_association_and_api",
        r##"(let ((source (locate-library "astro-ts-mode"))
              snapshots)
          (dotimes (_ 3)
            (load source nil 'nomessage)
            (push
             (list
              (cl-count 'astro-ts-mode features)
              (cl-count '("\\.astro\\'" . astro-ts-mode)
                        auto-mode-alist :test #'equal)
              (help-function-arglist
               'astro-ts-mode--treesit-language-at-point t)
              (symbol-value 'astro-ts-mode-indent-offset))
             snapshots))
          (list
           (cl-count 'astro-ts-mode features)
           (and (equal (nth 0 snapshots) (nth 1 snapshots))
                (equal (nth 1 snapshots) (nth 2 snapshots)))))"##,
        expect!["OK (1 t)"],
    )
}

fn autoload_file_signals_when_treesit_was_not_preloaded_like_upstream() -> ParityBatchCase {
    ParityBatchCase::signal(
        "autoload_file_signals_when_treesit_was_not_preloaded_like_upstream",
        r##"(list
          (featurep 'astro-ts-mode)
          (fboundp 'astro-ts-mode))"##,
        expect!["ERR (void-function treesit-ready-p)"],
    )
    .setup_outcome()
}

pub(super) fn registry_astro_ts_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        descriptor_records_exact_pin_dependency_and_installed_payload(),
        installed_source_has_exact_content_hash_feature_and_emacs_30_requirement(),
        complete_declared_callable_surface_has_exact_arities_and_command_status(),
        complete_declared_variable_surface_has_exact_defaults_kinds_groups_and_docs(),
        indent_registry_covers_astro_css_and_tsx_with_real_anchor_rules(),
        font_lock_registry_has_prefixed_embedded_features_and_native_astro_rules(),
        range_registry_embeds_tsx_and_css_in_every_documented_astro_context(),
        mode_metadata_auto_association_and_source_ownership_are_exact(),
        repeated_source_loading_is_idempotent_for_feature_association_and_api(),
    ]
}

pub(super) fn registry_astro_ts_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![autoload_file_signals_when_treesit_was_not_preloaded_like_upstream()]
}
