use expect_test::expect;

use super::ParityBatchCase;

fn auto_org_md_loads_exact_features_and_public_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_loads_exact_features_and_public_surface",
        r##"(list
         (featurep 'auto-org-md)
         (featurep 'org)
         (featurep 'ox-md)
         (mapcar
          (lambda (symbol)
            (list symbol
                  (fboundp symbol)
                  (commandp symbol)))
          '(auto-org-md-export
            auto-org-md-on
            auto-org-md-off
            auto-org-md-mode)))"##,
        expect![
            "OK (t t t ((auto-org-md-export t nil) (auto-org-md-on t nil) (auto-org-md-off t nil) (auto-org-md-mode t t)))"
        ],
    )
}

fn auto_org_md_public_arglists_and_interactive_contract_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_public_arglists_and_interactive_contract_are_exact",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (help-function-arglist symbol t)
            (interactive-form symbol)
            (documentation symbol)))
         '(auto-org-md-export
           auto-org-md-on
           auto-org-md-off
           auto-org-md-mode))"##,
        expect![[
            r#"OK ((auto-org-md-export nil nil nil) (auto-org-md-on nil nil "Turn on auto-org-md.") (auto-org-md-off nil nil "Turn off auto-org-md.") (auto-org-md-mode (&optional arg) (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) "cycle auto-org-md-mode between on/off\n\nThis is a minor mode.  If called interactively, toggle the ‘Auto-org-md\nmode’ mode.  If the prefix argument is positive, enable the mode, and if\nit is zero or negative, disable the mode.\n\nIf called from Lisp, toggle the mode if ARG is ‘toggle’.  Enable the\nmode if ARG is nil, omitted, or is a positive number.  Disable the mode\nif ARG is a negative number.\n\nTo check whether the minor mode is enabled in the current buffer,\nevaluate the variable ‘auto-org-md-mode’.\n\nThe mode’s hook is called both when the mode is enabled and when it is\ndisabled."))"#
        ]],
    )
}

fn auto_org_md_minor_mode_variable_properties_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_minor_mode_variable_properties_are_exact",
        r##"(list
         auto-org-md-mode
         (default-value 'auto-org-md-mode)
         (local-variable-p 'auto-org-md-mode)
         (get 'auto-org-md-mode 'variable-documentation)
         (get 'auto-org-md-mode 'custom-type)
         (get 'auto-org-md-mode 'standard-value)
         (get 'auto-org-md-mode 'permanent-local))"##,
        expect![[
            r#"OK (nil nil nil "Non-nil if Auto-org-md mode is enabled.\nUse the command `auto-org-md-mode' to change this variable." nil nil nil)"#
        ]],
    )
}

fn auto_org_md_mode_registry_has_exact_lighter_keymap_and_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_mode_registry_has_exact_lighter_keymap_and_hook",
        r##"(list
         (assq 'auto-org-md-mode minor-mode-alist)
         (assq 'auto-org-md-mode minor-mode-map-alist)
         (boundp 'auto-org-md-mode-map)
         (and (boundp 'auto-org-md-mode-map)
              auto-org-md-mode-map)
         (boundp 'auto-org-md-mode-hook)
         auto-org-md-mode-hook)"##,
        expect![[r#"OK ((auto-org-md-mode "org-md") nil nil nil t nil)"#]],
    )
}

fn auto_org_md_package_descriptor_matches_pinned_archive() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_package_descriptor_matches_pinned_archive",
        r##"(let* ((entry (assq 'auto-org-md package-alist))
         (descriptor (cadr entry)))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-reqs descriptor)
          (package-desc-kind descriptor)
          (file-name-nondirectory
           (directory-file-name
            (package-desc-dir descriptor)))
          (package-desc-extras descriptor)))"##,
        expect![[
            r#"OK (auto-org-md "20180213.2343" "Export a markdown file automatically when you save an org-file." ((emacs (24 4))) nil "auto-org-md-20180213.2343" ((:maintainers ("jamcha" . "jamcha.aa@gmail.com")) (:authors ("jamcha" . "jamcha.aa@gmail.com")) (:keywords "org" "markdown") (:revdesc . "9318338bdb7f") (:commit . "9318338bdb7fe8bd698d88f3af89b2d6413efdd2") (:url . "https://github.com/jamcha-aa/auto-org-md")))"#
        ]],
    )
}

fn auto_org_md_load_history_records_all_definitions_and_provide() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_load_history_records_all_definitions_and_provide",
        r##"(let* ((entry
                                 (cl-find-if
                                  (lambda (item)
                                    (member
                                     '(provide . auto-org-md)
                                     (cdr item)))
                                  load-history))
         (definitions (cdr entry)))
         (list
          (file-name-nondirectory (car entry))
          (mapcar
           (lambda (definition)
             (memq definition definitions))
           '((defun . auto-org-md-export)
             (defun . auto-org-md-on)
             (defun . auto-org-md-off)
             (defun . auto-org-md-mode)
             (provide . auto-org-md)))
          (cl-count
           '(provide . auto-org-md)
           definitions
           :test #'equal)))"##,
        expect![[r#"OK ("auto-org-md.el" (nil nil nil nil nil) 1)"#]],
    )
}

fn auto_org_md_autoloads_expose_export_without_loading_runtime() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_autoloads_expose_export_without_loading_runtime",
        r##"(list
         (featurep 'auto-org-md)
         (autoloadp
          (symbol-function 'auto-org-md-export))
         (commandp 'auto-org-md-export)
         (interactive-form 'auto-org-md-export)
         (symbol-function 'auto-org-md-export))"##,
        expect![
            "OK (nil t nil nil #[nil ((if (derived-mode-p 'org-mode) (progn (org-md-export-to-markdown)))) nil])"
        ],
    )
}

fn auto_org_md_autoloads_expose_minor_mode_metadata_without_loading_runtime() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_autoloads_expose_minor_mode_metadata_without_loading_runtime",
        r##"(list
         (featurep 'auto-org-md)
         (boundp 'auto-org-md-mode)
         (and (boundp 'auto-org-md-mode)
              (symbol-value 'auto-org-md-mode))
         (autoloadp
          (symbol-function 'auto-org-md-mode))
         (commandp 'auto-org-md-mode)
         (interactive-form 'auto-org-md-mode)
         (assq 'auto-org-md-mode minor-mode-alist)
         (get 'auto-org-md-mode 'variable-documentation))"##,
        expect![[
            r#"OK (nil nil nil t t (interactive (list (if current-prefix-arg (prefix-numeric-value current-prefix-arg) 'toggle))) (auto-org-md-mode "org-md") "Non-nil if Auto-org-md mode is enabled.\nUse the command `auto-org-md-mode' to change this variable.")"#
        ]],
    )
    .fresh_process()
}

fn auto_org_md_autoload_invocation_loads_runtime_and_installs_local_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_autoload_invocation_loads_runtime_and_installs_local_hook",
        r##"(progn
         (auto-org-md-test-reset-state)
         (with-temp-buffer
           (org-mode)
           (let (messages)
             (cl-letf (((symbol-function 'message)
                        (lambda (format-string &rest arguments)
                          (let ((rendered
                                 (substring-no-properties
                                  (apply #'format
                                         format-string
                                         arguments))))
                            (when
                                (string-prefix-p
                                 "auto-org-md-mode "
                                 rendered)
                              (push rendered messages))))))
               (auto-org-md-mode 1)
               (list
                (featurep 'auto-org-md)
                auto-org-md-mode
                (get 'auto-org-md-mode 'state)
                (local-variable-p 'after-save-hook)
                (memq 'auto-org-md-export
                      after-save-hook)
                (nreverse messages))))))"##,
        expect![[r#"OK (t t t t (auto-org-md-export t) ("auto-org-md-mode is on."))"#]],
    )
}

pub(super) fn registry_auto_org_md_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_org_md_loads_exact_features_and_public_surface(),
        auto_org_md_public_arglists_and_interactive_contract_are_exact(),
        auto_org_md_minor_mode_variable_properties_are_exact(),
        auto_org_md_mode_registry_has_exact_lighter_keymap_and_hook(),
        auto_org_md_package_descriptor_matches_pinned_archive(),
        auto_org_md_load_history_records_all_definitions_and_provide(),
    ]
}

pub(super) fn registry_auto_org_md_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_org_md_autoloads_expose_export_without_loading_runtime(),
        auto_org_md_autoloads_expose_minor_mode_metadata_without_loading_runtime(),
        auto_org_md_autoload_invocation_loads_runtime_and_installs_local_hook(),
    ]
}
