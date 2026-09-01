use expect_test::expect;

use super::ParityBatchCase;

fn atom_one_dark_theme_exact_package_descriptor_origin_and_dependency_contract_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_exact_package_descriptor_origin_and_dependency_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'atom-one-dark-theme package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (package-installed-p
           'atom-one-dark-theme
           '(20260119 1824))
          (file-name-nondirectory
           (locate-library
            "atom-one-dark-theme"))))"##,
        expect![[
            r#"OK (atom-one-dark-theme "20260119.1824" "Atom One Dark color theme." nil nil ((:maintainers ("Jonathan Chu" . "me@jonathanchu.is")) (:authors ("Jonathan Chu" . "me@jonathanchu.is")) (:revdesc . "bba02fb2672a") (:commit . "bba02fb2672a4c439d71920d8e068a3ff2ed463e") (:url . "https://github.com/jonathanchu/atom-one-dark-theme")) t "atom-one-dark-theme.el")"#
        ]],
    )
}

fn atom_one_dark_theme_installed_payload_hashes_only_exact_archive_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_installed_payload_hashes_only_exact_archive_files",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'atom-one-dark-theme package-alist)))
                 (directory
                  (package-desc-dir descriptor))
                 (archive-files
                  '("atom-one-dark-theme-pkg.el"
                    "atom-one-dark-theme.el")))
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
            r#"OK (("atom-one-dark-theme-autoloads.el" :generated t) ("atom-one-dark-theme-pkg.el" :archive 392 "9398e08bb830b7d4560d0ea7f935807b934daaa98cc07869ca488da1caebef0d") ("atom-one-dark-theme.el" :archive 46988 "a5c590aeb7dc5c2b8d36601a4c94a1145e46bd2291571af02807dd7a8552630c"))"#
        ]],
    )
}

fn atom_one_dark_theme_registration_documentation_feature_and_initial_state_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_registration_documentation_feature_and_initial_state_match",
        r##"(list
         (custom-theme-p 'atom-one-dark)
         (custom-theme-name-valid-p
          'atom-one-dark)
         (custom-theme-enabled-p
          'atom-one-dark)
         (get 'atom-one-dark 'theme-feature)
         (get 'atom-one-dark 'theme-documentation)
         (featurep 'atom-one-dark-theme)
         (and
          (memq
           'atom-one-dark
           custom-enabled-themes)
          t)
         (length
          (atom-one-dark-test-face-settings))
         (length
          (atom-one-dark-test-value-settings)))"##,
        expect![[
            r#"OK ((atom-one-dark user changed) t nil atom-one-dark-theme "Atom One Dark - An Emacs port of the Atom One Dark theme from Atom.io." t nil 460 3)"#
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_complete_callable_macro_command_metadata_surface_matches() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_complete_callable_macro_command_metadata_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "atom-one-dark"
                  (symbol-name symbol))
                 (fboundp symbol)
                 (let ((file
                        (symbol-file symbol 'defun)))
                   (and file
                        (string=
                         (file-name-nondirectory file)
                         "atom-one-dark-theme.el"))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (commandp symbol)
             (macrop symbol)
             (interactive-form symbol)
             (prin1-to-string
              (help-function-arglist symbol t))
             (documentation symbol t)
             (get symbol 'lisp-indent-function)
             (file-name-nondirectory
              (symbol-file symbol 'defun))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right))))))"##,
        expect![[
            r#"OK ((atom-one-dark-theme-change-faces-for-mode t nil (interactive nil) "nil" nil nil "atom-one-dark-theme.el") (atom-one-dark-with-color-variables nil t nil "(&rest body)" "Bind the colors list around BODY." 0 "atom-one-dark-theme.el"))"#
        ]],
    )
}

fn atom_one_dark_theme_complete_variable_metadata_defaults_and_hook_surface_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_complete_variable_metadata_defaults_and_hook_surface_match",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (boundp symbol)
            (default-value symbol)
            (special-variable-p symbol)
            (custom-variable-p symbol)
            (documentation-property
             symbol
             'variable-documentation)
            (get symbol 'standard-value)
            (get symbol 'local-variable-if-set)
            (file-name-nondirectory
             (symbol-file symbol 'defvar))))
         '(atom-one-dark-colors-alist
           atom-one-dark-theme-force-faces-for-mode))"##,
        expect![[
            r##"OK ((atom-one-dark-colors-alist t (("atom-one-dark-accent" . "#528BFF") ("atom-one-dark-fg" if nil "color-248" "#ABB2BF") ("atom-one-dark-bg" if nil "color-235" "#282C34") ("atom-one-dark-bg-1" if nil "color-234" "#121417") ("atom-one-dark-bg-hl" if nil "color-236" "#2C323C") ("atom-one-dark-gutter" if nil "color-239" "#4B5363") ("atom-one-dark-insert" . "#43D08A") ("atom-one-dark-change" . "#E0C285") ("atom-one-dark-delete" . "#E05252") ("atom-one-dark-info" . "#6494ED") ("atom-one-dark-success" . "#73C900") ("atom-one-dark-warning" . "#E2C08D") ("atom-one-dark-error" . "#FF6347") ("atom-one-dark-mono-1" if nil "color-248" "#ABB2BF") ("atom-one-dark-mono-2" if nil "color-244" "#828997") ("atom-one-dark-mono-3" if nil "color-240" "#5C6370") ("atom-one-dark-cyan" . "#56B6C2") ("atom-one-dark-blue" . "#61AFEF") ("atom-one-dark-purple" . "#C678DD") ("atom-one-dark-green" . "#98C379") ("atom-one-dark-red-1" . "#E06C75") ("atom-one-dark-red-2" . "#BE5046") ("atom-one-dark-orange-1" . "#D19A66") ("atom-one-dark-orange-2" . "#E5C07B") ("atom-one-dark-gray" if nil "color-237" "#3E4451") ("atom-one-dark-silver" if nil "color-247" "#9DA5B4") ("atom-one-dark-black" if nil "color-233" "#21252B") ("atom-one-dark-ui-fg" if nil "color-247" "#9DA5B4") ("atom-one-dark-level-3-color" if nil "color-233" "#21252B") ("atom-one-dark-border" if nil "color-232" "#181A1F")) t nil "List of Atom One Dark colors." nil nil "atom-one-dark-theme.el") (atom-one-dark-theme-force-faces-for-mode t t t nil "If t, atom-one-dark-theme will use Face Remapping to alter the theme faces for\nthe current buffer based on its mode in an attempt to mimick the Atom One Dark\nTheme from Atom.io as best as possible.\nThe reason this is required is because some modes (html-mode, jyaml-mode, ...)\ndo not provide the necessary faces to do theming without conflicting with other\nmodes.\nCurrent modes, and their faces, impacted by this variable:\n* js2-mode: font-lock-constant-face, font-lock-doc-face, font-lock-variable-name-face\n* html-mode: font-lock-function-name-face, font-lock-variable-name-face\n" nil nil "atom-one-dark-theme.el"))"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_setting_inventory_order_duplicates_and_value_names_are_exact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_setting_inventory_order_duplicates_and_value_names_are_exact",
        r##"(let* ((settings
                  (get 'atom-one-dark 'theme-settings))
                 (face-settings
                  (atom-one-dark-test-face-settings))
                 (faces
                  (mapcar #'cadr face-settings))
                 (value-settings
                  (atom-one-dark-test-value-settings))
                 duplicates
                 seen)
         (dolist (face faces)
           (if
               (memq face seen)
               (push face duplicates)
             (push face seen)))
         (list
          (length settings)
          (length face-settings)
          (length
           (delete-dups
            (copy-sequence faces)))
          (nreverse duplicates)
          (car faces)
          (car
           (last faces))
          (mapcar #'cadr value-settings)
          (mapcar #'car value-settings)
          (mapcar #'caddr value-settings)))"##,
        expect![
            "OK (463 460 459 (helm-grep-finish) default tab-line-highlight (fci-rule-color tetris-x-colors ansi-color-names-vector) (theme-value theme-value theme-value) (atom-one-dark atom-one-dark atom-one-dark))"
        ],
    )
    .fresh_process()
}

fn atom_one_dark_theme_source_reloads_accumulate_settings_but_preserve_defvars_and_deduplicate_paths()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_source_reloads_accumulate_settings_but_preserve_defvars_and_deduplicate_paths",
        r##"(let* ((source
                  (getenv "NEOMACS_PACKAGE_SOURCE"))
                 (directory
                  (file-name-as-directory
                   (file-name-directory source)))
                 observations)
         (setq atom-one-dark-colors-alist
               (copy-tree
                atom-one-dark-colors-alist))
         (setcdr
          (assoc
           "atom-one-dark-accent"
           atom-one-dark-colors-alist)
          "#010203")
         (setq atom-one-dark-theme-force-faces-for-mode
               'user-choice)
         (dolist (_ '(first second))
           (load source nil t t)
           (push
            (list
             (length
              (get 'atom-one-dark 'theme-settings))
             (length
              atom-one-dark-colors-alist)
             (cdr
              (assoc
               "atom-one-dark-accent"
               atom-one-dark-colors-alist))
             atom-one-dark-theme-force-faces-for-mode
             (let ((count 0))
               (dolist
                   (function
                    after-change-major-mode-hook
                    count)
                 (when
                     (eq
                      function
                      'atom-one-dark-theme-change-faces-for-mode)
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
        expect![[
            r##"OK ((926 30 "#010203" user-choice 1 1) (1389 30 "#010203" user-choice 1 1))"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_generated_autoload_registers_paths_prefix_and_feature_only()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_generated_autoload_registers_paths_prefix_and_feature_only",
        r##"(let* ((source
                  (getenv "NEOMACS_PACKAGE_SOURCE"))
                 (directory
                  (file-name-as-directory
                   (file-name-directory source)))
                 (plain-directory
                  (directory-file-name directory))
                 (prefixes
                  (if
                      (hash-table-p definition-prefixes)
                      (gethash
                       "atom-one-dark"
                       definition-prefixes)
                    (cdr
                     (assoc
                      "atom-one-dark"
                      definition-prefixes))))
                 (history
                  (seq-find
                   (lambda (entry)
                     (and
                      (stringp
                       (car entry))
                      (string=
                       (file-name-nondirectory
                        (car entry))
                       "atom-one-dark-theme-autoloads.el")))
                   load-history)))
         (list
          (featurep 'atom-one-dark-theme)
          (featurep
           'atom-one-dark-theme-autoloads)
          (custom-theme-p
           'atom-one-dark)
          (fboundp
           'atom-one-dark-theme-change-faces-for-mode)
          (fboundp
           'atom-one-dark-with-color-variables)
          (boundp
           'atom-one-dark-colors-alist)
          (equal
           (car load-path)
           plain-directory)
          (let ((count 0))
            (dolist (entry load-path count)
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
          (sort
           (delete-dups
            (copy-sequence prefixes))
           #'string<)
          (and history
               (mapcar
                (lambda (event)
                  (list
                   'provide
                   (cdr event)))
                (seq-filter
                 (lambda (event)
                   (and
                    (consp event)
                    (eq
                     (car event)
                     'provide)))
                 (cdr history))))))"##,
        expect![[
            r#"OK (nil t nil nil nil nil t 1 t 1 ("atom-one-dark-theme") ((provide atom-one-dark-theme-autoloads)))"#
        ]],
    )
}

pub(super) fn registry_atom_one_dark_theme_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_one_dark_theme_exact_package_descriptor_origin_and_dependency_contract_match(),
        atom_one_dark_theme_installed_payload_hashes_only_exact_archive_files(),
        atom_one_dark_theme_registration_documentation_feature_and_initial_state_match(),
        atom_one_dark_theme_complete_callable_macro_command_metadata_surface_matches(),
        atom_one_dark_theme_complete_variable_metadata_defaults_and_hook_surface_match(),
        atom_one_dark_theme_setting_inventory_order_duplicates_and_value_names_are_exact(),
        atom_one_dark_theme_source_reloads_accumulate_settings_but_preserve_defvars_and_deduplicate_paths(),
    ]
}

pub(super) fn registry_atom_one_dark_theme_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![atom_one_dark_theme_generated_autoload_registers_paths_prefix_and_feature_only()]
}
