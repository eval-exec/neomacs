use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_nxml_source_registers_feature_public_commands_and_alias() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_source_registers_feature_public_commands_and_alias",
        r##"(list
         (featurep 'auto-complete-nxml)
         (mapcar
          (lambda (symbol)
            (list symbol
                  (fboundp symbol)
                  (commandp symbol)
                  (documentation symbol)))
          '(auto-complete-nxml-ac-start-with-insert
            auto-complete-nxml-popup-help
            auto-complete-nxml-toggle-automatic))
         (eq
          (indirect-function
           'auto-complete-nxml-insert-with-ac-trigger-command)
          (indirect-function
           'auto-complete-nxml-ac-start-with-insert)))"##,
        expect![[
            r#"OK (t ((auto-complete-nxml-ac-start-with-insert t t nil) (auto-complete-nxml-popup-help t t "Popup help about something at point.") (auto-complete-nxml-toggle-automatic t t "Switch value of ‘auto-complete-nxml-automatic-p’.")) t)"#
        ]],
    )
}

fn auto_complete_nxml_custom_variables_have_exact_defaults_types_and_group() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_custom_variables_have_exact_defaults_types_and_group",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (default-value symbol)
            (custom-variable-p symbol)
            (get symbol 'custom-type)
            (get symbol 'custom-group)
            (documentation-property symbol 'variable-documentation)))
         '(auto-complete-nxml-popup-help-key
           auto-complete-nxml-toggle-automatic-key
           auto-complete-nxml-automatic-p))"##,
        expect![[
            r#"OK ((auto-complete-nxml-popup-help-key nil (nil) string nil "Keystroke for popup help about something at point.") (auto-complete-nxml-toggle-automatic-key nil (nil) string nil "Keystroke for toggle on/off automatic completion.") (auto-complete-nxml-automatic-p t (t) boolean nil "Whether start completion automatically."))"#
        ]],
    )
}

fn auto_complete_nxml_sources_expose_exact_auto_complete_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_sources_expose_exact_auto_complete_contracts",
        r##"(mapcar
         (lambda (symbol)
           (list symbol
                 (acnxml-test-source-shape (symbol-value symbol))))
         '(ac-source-nxml-tag
           ac-source-nxml-attr
           ac-source-nxml-attr-value
           ac-source-nxml-css
           ac-source-nxml-css-property
           ac-source-nxml-tag-value-by-nxml
           ac-source-nxml-tag-value-by-myself))"##,
        expect![[
            r#"OK ((ac-source-nxml-tag ((candidates . :function) (prefix . "<\\([a-zA-Z0-9:-]*\\)") (symbol . "t") (document . :function) (requires . 0) (cache) (limit . 500) (action . :function))) (ac-source-nxml-attr ((candidates . :function) (prefix . "\\(?:<[a-zA-Z0-9:-]+\\|[^=]\"\\|[^=]'\\)\\s-+\\([a-zA-Z0-9-]*\\)") (symbol . "a") (document . :function) (requires . 0) (cache) (limit . 500) (action . :function))) (ac-source-nxml-attr-value ((candidates . :function) (prefix . "=\\(?:\"\\|'\\)\\s-*\\([^\"':; ]*\\)") (symbol . "v") (requires . 0) (cache) (limit . 500) (action . :function))) (ac-source-nxml-css ((candidates . :function) (prefix . "\\s-+style=\\(?:\"\\|'\\)\\([^\"']*\\)") (symbol . "c") (requires . 0) (cache) (limit . 500) (action . :function))) (ac-source-nxml-css-property ((candidates . :function) (prefix . :function) (symbol . "p") (requires . 0) (cache) (limit . 500) (action . :function))) (ac-source-nxml-tag-value-by-nxml ((candidates . :function) (prefix . ">\\s-*\\([^<]*\\)") (symbol . "w") (requires . 0) (cache) (limit . 500) (action . :function))) (ac-source-nxml-tag-value-by-myself ((candidates . :function) (symbol . "w") (cache) (limit . 500))))"#
        ]],
    )
}

fn auto_complete_nxml_source_prefixes_match_practical_editing_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_source_prefixes_match_practical_editing_boundaries",
        r##"(mapcar
         (lambda (case)
           (let ((source (symbol-value (car case)))
                 (text (cdr case)))
             (with-temp-buffer
               (insert text)
               (let ((prefix (cdr (assq 'prefix source))))
                 (list
                  (car case)
                  text
                  (cond
                   ((stringp prefix)
                    (and (string-match prefix text)
                         (match-string 1 text)))
                   ((functionp prefix)
                    (funcall prefix))
                   (t prefix)))))))
         '((ac-source-nxml-tag . "<math:su")
           (ac-source-nxml-attr . "<table data-ro")
           (ac-source-nxml-attr-value . "<table role=\"but")
           (ac-source-nxml-css . "<p style=\"font-s")
           (ac-source-nxml-tag-value-by-nxml . "<status>dra")))"##,
        expect![[
            r#"OK ((ac-source-nxml-tag "<math:su" "math:su") (ac-source-nxml-attr "<table data-ro" "data-ro") (ac-source-nxml-attr-value "<table role=\"but" "but") (ac-source-nxml-css "<p style=\"font-s" "font-s") (ac-source-nxml-tag-value-by-nxml "<status>dra" "dra"))"#
        ]],
    )
}

fn auto_complete_nxml_load_history_records_definition_and_provide_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_load_history_records_definition_and_provide_contract",
        r##"(let* ((entry
                                 (cl-find-if
                                  (lambda (item)
                                    (memq
                                     '(provide . auto-complete-nxml)
                                     (cdr item)))
                                  load-history))
              (definitions (cdr entry)))
         (list
          (not (null entry))
          (mapcar
           (lambda (definition)
             (member definition definitions))
           '((defun . auto-complete-nxml-get-candidates)
             (defun . auto-complete-nxml-expand-other-xmlns)
             (defun . auto-complete-nxml-setup)
             (defun . auto-complete-nxml-toggle-automatic)
             (provide . auto-complete-nxml)))))"##,
        expect!["OK (nil (nil nil nil nil nil))"],
    )
}

fn auto_complete_nxml_advice_registry_preserves_active_and_disabled_advice_roles() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_nxml_advice_registry_preserves_active_and_disabled_advice_roles",
        r##"(mapcar
         (lambda (case)
           (let ((function (nth 0 case))
                 (class (nth 1 case))
                 (name (nth 2 case)))
             (list
              function
              name
              (not (null (ad-find-advice function class name)))
              (ad-is-active function))))
         '((rng-set-document-type-and-validate
            around make-doc4ac-in-nxml)
           (rng-c-parse-element
            around auto-complete-nxml-ad-make-doc)
           (rng-c-parse-attribute
            around auto-complete-nxml-ad-make-doc)
           (rng-c-parse-name-class
            after auto-complete-nxml-ad-make-doc)
           (forward-comment
            around auto-complete-nxml-ad-make-doc)
           (rng-c-parse-follow-annotations
            around auto-complete-nxml-ad-make-doc)))"##,
        expect![
            "OK ((rng-set-document-type-and-validate make-doc4ac-in-nxml t t) (rng-c-parse-element auto-complete-nxml-ad-make-doc t nil) (rng-c-parse-attribute auto-complete-nxml-ad-make-doc t nil) (rng-c-parse-name-class auto-complete-nxml-ad-make-doc t nil) (forward-comment auto-complete-nxml-ad-make-doc t nil) (rng-c-parse-follow-annotations auto-complete-nxml-ad-make-doc t nil))"
        ],
    )
}

fn auto_complete_nxml_setup_installs_local_keys_sources_and_trigger_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_setup_installs_local_keys_sources_and_trigger_contract",
        r##"(let ((auto-complete-nxml-popup-help-key "C-:")
             (auto-complete-nxml-toggle-automatic-key "C-c C-t")
             (ac-modes '(fundamental-mode))
             (ac-trigger-commands '(self-insert-command))
             calls)
         (cl-letf (((symbol-function 'auto-complete-mode)
                    (lambda (argument)
                      (push (list :mode argument) calls)))
                   ((symbol-function 'auto-complete-nxml-init-project)
                    (lambda ()
                      (push 'init-project calls))))
           (with-temp-buffer
             (use-local-map (make-sparse-keymap))
             (auto-complete-nxml-setup)
             (list
              ac-sources
              ac-modes
              ac-trigger-commands
              (lookup-key (current-local-map) (kbd "SPC"))
              (lookup-key (current-local-map) (kbd "C-:"))
              (lookup-key (current-local-map) (kbd "C-c C-t"))
              (nreverse calls)))))"##,
        expect![
            "OK ((ac-source-nxml-tag ac-source-nxml-attr ac-source-nxml-attr-value ac-source-nxml-css ac-source-nxml-css-property ac-source-nxml-tag-value-by-nxml ac-source-nxml-tag-value-by-myself) (nxml-mode fundamental-mode) (auto-complete-nxml-ac-start-with-insert self-insert-command) auto-complete-nxml-ac-start-with-insert auto-complete-nxml-popup-help auto-complete-nxml-toggle-automatic ((:mode t) init-project))"
        ],
    )
}

fn auto_complete_nxml_generated_autoload_file_has_no_eager_runtime_side_effects() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_nxml_generated_autoload_file_has_no_eager_runtime_side_effects",
        r##"(list
         (featurep 'auto-complete-nxml)
         (boundp 'auto-complete-nxml-automatic-p)
         (fboundp 'auto-complete-nxml-toggle-automatic)
         (and
          (boundp 'nxml-mode-hook)
          (memq 'auto-complete-nxml-setup nxml-mode-hook))
         (cl-some
          (lambda (entry)
            (memq '(provide . auto-complete-nxml) (cdr entry)))
          load-history))"##,
        expect!["OK (nil nil nil nil nil)"],
    )
}

pub(super) fn registry_auto_complete_nxml_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_nxml_source_registers_feature_public_commands_and_alias(),
        auto_complete_nxml_custom_variables_have_exact_defaults_types_and_group(),
        auto_complete_nxml_sources_expose_exact_auto_complete_contracts(),
        auto_complete_nxml_source_prefixes_match_practical_editing_boundaries(),
        auto_complete_nxml_load_history_records_definition_and_provide_contract(),
        auto_complete_nxml_advice_registry_preserves_active_and_disabled_advice_roles(),
        auto_complete_nxml_setup_installs_local_keys_sources_and_trigger_contract(),
    ]
}

pub(super) fn registry_auto_complete_nxml_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_complete_nxml_generated_autoload_file_has_no_eager_runtime_side_effects()]
}
