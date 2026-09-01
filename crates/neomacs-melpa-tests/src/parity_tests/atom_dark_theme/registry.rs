use expect_test::expect;

use super::ParityBatchCase;

fn atom_dark_theme_exact_package_descriptor_origin_dependency_and_feature_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_exact_package_descriptor_origin_dependency_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'atom-dark-theme package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'atom-dark-theme)
          (package-installed-p
           'atom-dark-theme
           '(20220114 1902))
          (file-name-nondirectory
           (locate-library "atom-dark-theme"))))"##,
        expect![[
            r#"OK (atom-dark-theme "20220114.1902" "An Emacs port of the Atom Dark theme from Atom.io." nil nil ((:maintainers ("Jeremy Whitlock" . "jwhitlock@apache.org")) (:authors ("Jeremy Whitlock" . "jwhitlock@apache.org")) (:keywords "themes" "atom" "dark") (:revdesc . "2b3c7ad42bbc") (:commit . "2b3c7ad42bbcab3214a131f8957b92e717b36ad3") (:url . "https://github.com/whitlockjc/atom-dark-theme-emacs")) t t "atom-dark-theme.el")"#
        ]],
    )
}

fn atom_dark_theme_installed_payload_inventory_hashes_only_immutable_archive_files()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_installed_payload_inventory_hashes_only_immutable_archive_files",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'atom-dark-theme package-alist)))
                 (directory
                  (package-desc-dir descriptor))
                 (archive-files
                  '("atom-dark-theme-pkg.el"
                    "atom-dark-theme.el")))
         (mapcar
          (lambda (file)
            (let ((path
                   (expand-file-name file directory)))
              (if
                  (member file archive-files)
                  (list
                   file
                   :archive
                   (file-attribute-size
                    (file-attributes path))
                   (with-temp-buffer
                     (insert-file-contents-literally path)
                     (secure-hash
                      'sha256
                      (current-buffer))))
                (list
                 file
                 :generated
                 (file-readable-p path)))))
          (sort
           (seq-filter
            (lambda (file)
              (file-regular-p
               (expand-file-name file directory)))
            (directory-files directory nil "\\`[^.]"))
           #'string<)))"##,
        expect![[
            r#"OK (("atom-dark-theme-autoloads.el" :generated t) ("atom-dark-theme-pkg.el" :archive 463 "77465cdc783ea5c9948b42369bbf0dadf7ae422a714d290e50535a1086ccb0a2") ("atom-dark-theme.el" :archive 12426 "ca1b398ceb1b61709197478dc7f705b8337a0a9631e399948e643520c5557382") ("atom-dark-theme.elc" :generated t))"#
        ]],
    )
}

fn atom_dark_theme_registration_documentation_feature_and_initial_state_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_registration_documentation_feature_and_initial_state_match",
        r##"(list
         (custom-theme-p 'atom-dark)
         (custom-theme-name-valid-p
          'atom-dark)
         (custom-theme-enabled-p
          'atom-dark)
         (get 'atom-dark 'theme-feature)
         (get 'atom-dark 'theme-documentation)
         (featurep 'atom-dark-theme)
         (memq
          'atom-dark
          custom-enabled-themes)
         (mapcar
          #'cadr
          (seq-filter
           (lambda (setting)
             (eq
              (car setting)
              'theme-face))
           (get 'atom-dark 'theme-settings)))
         (mapcar
          #'cadr
          (seq-filter
           (lambda (setting)
             (eq
              (car setting)
              'theme-value))
           (get 'atom-dark 'theme-settings))))"##,
        expect![[
            r#"OK ((atom-dark user changed) t nil atom-dark-theme "Atom Dark - An Emacs port of the Atom Dark theme from Atom.io." t nil (company-tooltip-selection company-tooltip-common-selection company-tooltip-common company-tooltip company-scrollbar-fg company-scrollbar-bg company-preview-search company-preview-common company-preview whitespace-trailing whitespace-tab whitespace-space-before-tab whitespace-space-after-tab whitespace-space whitespace-newline whitespace-line whitespace-indentation whitespace-hspace whitespace-empty speedbar-tag-face speedbar-separator-face speedbar-selected-face speedbar-highlight-face speedbar-file-face speedbar-directory-face speedbar-button-face realgud-overlay-arrow3 realgud-overlay-arrow2 realgud-overlay-arrow1 powerline-active2 minimap-active-region-background js2-jsdoc-value js2-jsdoc-type js2-jsdoc-tag js2-jsdoc-html-tag-name js2-jsdoc-html-tag-delimiter js2-function-param js2-external-variable js2-error markdown-header-rule-face markdown-header-delimiter-face markdown-header-face markdown-blockquote-face flx-highlight-face guide-key/prefix-command-face guide-key/key-face guide-key/highlight-command-face dired-symlink dired-flagged dired-directory diff-hl-insert diff-hl-delete diff-hl-change ido-virtual ido-subdir ido-only-match ido-first-match isearch-fail isearch mode-line-inactive mode-line-highlight mode-line-emphasis mode-line-buffer-id mode-line font-lock-warning-face font-lock-variable-name-face font-lock-type-face font-lock-string-face font-lock-regexp-grouping-construct font-lock-regexp-grouping-backslash font-lock-preprocessor-face font-lock-keyword-face font-lock-function-name-face font-lock-doc-face font-lock-constant-face font-lock-comment-face font-lock-comment-delimiter-face font-lock-builtin-face variable-pitch trailing-whitespace tooltip shadow secondary-selection region query-replace next-error minibuffer-prompt match link-visited link lazy-highlight highlight header-line fixed-pitch escape-glyph default cursor button) nil)"#
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_complete_callable_command_arglist_and_source_surface_matches() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_dark_theme_complete_callable_command_arglist_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "atom-dark"
                  (symbol-name symbol))
                 (fboundp symbol)
                 (let ((file
                        (symbol-file symbol 'defun)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "atom-dark-theme.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (commandp symbol)
             (interactive-form symbol)
             (prin1-to-string
              (help-function-arglist symbol t))
             (documentation symbol t)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right))))))"##,
        expect![[
            r#"OK ((atom-dark-theme-change-faces-for-mode t (interactive nil) "nil" nil "atom-dark-theme.el"))"#
        ]],
    )
}

fn atom_dark_theme_complete_variable_default_documentation_and_hook_surface_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_complete_variable_default_documentation_and_hook_surface_matches",
        r##"(list
         (boundp
          'atom-dark-theme-force-faces-for-mode)
         atom-dark-theme-force-faces-for-mode
         (special-variable-p
          'atom-dark-theme-force-faces-for-mode)
         (custom-variable-p
          'atom-dark-theme-force-faces-for-mode)
         (documentation-property
          'atom-dark-theme-force-faces-for-mode
          'variable-documentation)
         (file-name-nondirectory
          (symbol-file
           'atom-dark-theme-force-faces-for-mode
           'defvar))
         (and
          (memq
           'atom-dark-theme-change-faces-for-mode
           after-change-major-mode-hook)
          t)
         (default-value
          'atom-dark-theme-force-faces-for-mode)
         (get
          'atom-dark-theme-force-faces-for-mode
          'standard-value)
         (get
          'atom-dark-theme-force-faces-for-mode
          'local-variable-if-set)
         (let ((count 0))
           (dolist
               (function
                after-change-major-mode-hook
                count)
             (when
                 (eq
                  function
                  'atom-dark-theme-change-faces-for-mode)
               (setq count
                     (1+ count))))))"##,
        expect![[
            r#"OK (t t t nil "If t, atom-dark-theme will use Face Remapping to alter the theme faces for\nthe current buffer based on its mode in an attempt to mimick the Atom Dark\nTheme from Atom.io as best as possible.\n\nThe reason this is required is because some modes (html-mode, yaml-mode, ...)\ndo not provide the necessary faces to do theming without conflicting with other\nmodes.\n\nCurrent modes, and their faces, impacted by this variable:\n\n* html-mode: font-lock-variable-name-face\n* markdown-mode: default\n* yaml-mode: font-lock-variable-name-face\n" "atom-dark-theme.el" t t nil nil 1)"#
        ]],
    )
}

fn atom_dark_theme_setting_inventory_has_exact_source_shape_order_and_uniqueness() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_dark_theme_setting_inventory_has_exact_source_shape_order_and_uniqueness",
        r##"(let* ((settings
                  (get 'atom-dark 'theme-settings))
                 (faces
                  (mapcar #'cadr settings)))
         (list
          (length settings)
          faces
          (delete-dups
           (mapcar #'car settings))
          (delete-dups
           (mapcar #'caddr settings))
          (length
           (delete-dups
            (copy-sequence faces)))
          (length
           (seq-filter
            (lambda (setting)
              (eq
               (car setting)
               'theme-face))
            settings))
          (mapcar
           #'cadr
           (seq-filter
            (lambda (setting)
              (eq
               (car setting)
               'theme-value))
            settings))))"##,
        expect![
            "OK (98 (company-tooltip-selection company-tooltip-common-selection company-tooltip-common company-tooltip company-scrollbar-fg company-scrollbar-bg company-preview-search company-preview-common company-preview whitespace-trailing whitespace-tab whitespace-space-before-tab whitespace-space-after-tab whitespace-space whitespace-newline whitespace-line whitespace-indentation whitespace-hspace whitespace-empty speedbar-tag-face speedbar-separator-face speedbar-selected-face speedbar-highlight-face speedbar-file-face speedbar-directory-face speedbar-button-face realgud-overlay-arrow3 realgud-overlay-arrow2 realgud-overlay-arrow1 powerline-active2 minimap-active-region-background js2-jsdoc-value js2-jsdoc-type js2-jsdoc-tag js2-jsdoc-html-tag-name js2-jsdoc-html-tag-delimiter js2-function-param js2-external-variable js2-error markdown-header-rule-face markdown-header-delimiter-face markdown-header-face markdown-blockquote-face flx-highlight-face guide-key/prefix-command-face guide-key/key-face guide-key/highlight-command-face dired-symlink dired-flagged dired-directory diff-hl-insert diff-hl-delete diff-hl-change ido-virtual ido-subdir ido-only-match ido-first-match isearch-fail isearch mode-line-inactive mode-line-highlight mode-line-emphasis mode-line-buffer-id mode-line font-lock-warning-face font-lock-variable-name-face font-lock-type-face font-lock-string-face font-lock-regexp-grouping-construct font-lock-regexp-grouping-backslash font-lock-preprocessor-face font-lock-keyword-face font-lock-function-name-face font-lock-doc-face font-lock-constant-face font-lock-comment-face font-lock-comment-delimiter-face font-lock-builtin-face variable-pitch trailing-whitespace tooltip shadow secondary-selection region query-replace next-error minibuffer-prompt match link-visited link lazy-highlight highlight header-line fixed-pitch escape-glyph default cursor button) (theme-face) (atom-dark) 98 98 nil)"
        ],
    )
    .fresh_process()
}

fn atom_dark_theme_source_reloads_accumulate_settings_but_deduplicate_hook_and_load_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_source_reloads_accumulate_settings_but_deduplicate_hook_and_load_path",
        r##"(let* ((source
                  (getenv "NEOMACS_PACKAGE_SOURCE"))
                 (directory
                  (file-name-as-directory
                   (file-name-directory source)))
                 observations)
         (setq atom-dark-theme-force-faces-for-mode
               'preserve-user-choice)
         (dolist (_ '(first second))
           (load source nil t t)
           (push
            (list
             (length
              (get 'atom-dark 'theme-settings))
             atom-dark-theme-force-faces-for-mode
             (let ((count 0))
               (dolist
                   (function
                    after-change-major-mode-hook
                    count)
                 (when
                     (eq
                      function
                      'atom-dark-theme-change-faces-for-mode)
                   (setq count
                         (1+ count)))))
             (let ((count 0))
               (dolist
                   (entry
                    custom-theme-load-path
                    count)
                 (when
                     (equal entry directory)
                   (setq count
                         (1+ count))))))
            observations))
         (nreverse observations))"##,
        expect!["OK ((196 preserve-user-choice 1 1) (294 preserve-user-choice 1 1))"],
    )
}

fn atom_dark_theme_generated_autoload_registers_paths_prefix_and_feature_without_loading_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_generated_autoload_registers_paths_prefix_and_feature_without_loading_source",
        r##"(let* ((source
                  (getenv "NEOMACS_PACKAGE_SOURCE"))
                 (directory
                  (file-name-as-directory
                   (file-name-directory source)))
                 (plain-directory
                  (directory-file-name directory))
                 (history
                  (seq-find
                   (lambda (entry)
                     (and
                      (stringp
                       (car entry))
                      (string=
                       (file-name-nondirectory
                        (car entry))
                       "atom-dark-theme-autoloads.el")))
                   load-history)))
         (list
          (featurep 'atom-dark-theme)
          (featurep
           'atom-dark-theme-autoloads)
          (custom-theme-p
           'atom-dark)
          (fboundp
           'atom-dark-theme-change-faces-for-mode)
          (boundp
           'atom-dark-theme-force-faces-for-mode)
          (equal
           (car load-path)
           plain-directory)
          (let ((count 0))
            (dolist
                (entry load-path count)
              (when
                  (equal entry plain-directory)
                (setq count
                      (1+ count)))))
          (equal
           (car custom-theme-load-path)
           directory)
          (let ((count 0))
            (dolist
                (entry
                 custom-theme-load-path
                 count)
              (when
                  (equal entry directory)
                (setq count
                      (1+ count)))))
          (let ((prefixes
                 (if
                     (hash-table-p definition-prefixes)
                     (gethash
                      "atom-dark"
                      definition-prefixes)
                   (cdr
                    (assoc
                     "atom-dark"
                     definition-prefixes)))))
            (sort
             (delete-dups
              (copy-sequence prefixes))
             #'string<))
          (and history
               (mapcar
                (lambda (event)
                  (if
                      (and
                       (consp event)
                       (eq
                        (car event)
                        'provide))
                      (list
                       'provide
                       (cdr event))
                    event))
                (seq-filter
                 (lambda (event)
                   (and
                    (consp event)
                    (eq
                     (car event)
                     'provide)))
                 (cdr history))))))"##,
        expect![[
            r#"OK (nil t nil nil nil t 1 t 1 ("atom-dark-theme") ((provide atom-dark-theme-autoloads)))"#
        ]],
    )
}

pub(super) fn registry_atom_dark_theme_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_dark_theme_exact_package_descriptor_origin_dependency_and_feature_contract_match(),
        atom_dark_theme_installed_payload_inventory_hashes_only_immutable_archive_files(),
        atom_dark_theme_registration_documentation_feature_and_initial_state_match(),
        atom_dark_theme_complete_callable_command_arglist_and_source_surface_matches(),
        atom_dark_theme_complete_variable_default_documentation_and_hook_surface_matches(),
        atom_dark_theme_setting_inventory_has_exact_source_shape_order_and_uniqueness(),
        atom_dark_theme_source_reloads_accumulate_settings_but_deduplicate_hook_and_load_path(),
    ]
}

pub(super) fn registry_atom_dark_theme_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_dark_theme_generated_autoload_registers_paths_prefix_and_feature_without_loading_source(),
    ]
}
